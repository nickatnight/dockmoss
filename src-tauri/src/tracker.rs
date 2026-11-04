use std::time::{Duration, Instant};

use crate::dock::{self, DockError, DockItem, RawDockChild};
use crate::geometry::{Display, ScreenPoint, ScreenRect};
use crate::usage::DockApp;

/// Overlay-local logical pixels a rect field must move, from the last frame
/// sent, to count as a change.
const MIN_RECT_CHANGE_PX: f64 = 0.5;
/// The cursor polls fast within this many points of the Dock.
const PROXIMITY: f64 = 150.0;
/// Long enough to follow a magnification or reveal that is still settling.
const FAST_AFTER_SEND: Duration = Duration::from_millis(250);
const REST_POINTER_CHECK_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollRate {
    Fast,
    Slow,
}

impl PollRate {
    pub fn interval(self) -> Duration {
        match self {
            Self::Fast => Duration::from_nanos(16_666_667),
            Self::Slow => Duration::from_millis(500),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Shown,
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickLog {
    ReadFailed(DockError),
    Recovered,
}

#[derive(Debug, PartialEq)]
pub struct Tick {
    /// The frame to send to the overlay, if it changed.
    pub send: Option<Vec<DockItem>>,
    pub log: Option<TickLog>,
    /// The overlay window's new visibility, if it changed.
    pub overlay: Option<Visibility>,
    /// The Dock's apps, when a sent frame lists other apps than the last
    /// non-empty frame did. Empty frames (Dock hidden or unreadable) never
    /// report, so growth keeps the last known apps.
    pub apps: Option<Vec<DockApp>>,
    pub proximity_rate: PollRate,
}

pub struct Tracker {
    display: Display,
    last_sent: Option<Vec<DockItem>>,
    last_apps: Option<Vec<DockApp>>,
    last_error: Option<DockError>,
    /// Recorded when the ordering is issued, before the main thread applies it.
    ordered: Visibility,
    /// What `ordered` was before that, restored if the ordering fails.
    before_order: Visibility,
}

impl Tracker {
    /// `overlay` is the window's visibility when tracking starts, so the first
    /// tick orders it only if the Dock disagrees.
    pub fn new(display: Display, overlay: Visibility) -> Self {
        Self {
            display,
            last_sent: None,
            last_apps: None,
            last_error: None,
            ordered: overlay,
            before_order: overlay,
        }
    }

    /// Converts later reads against `display`. Returns false when it is the
    /// display already in use, so a repeated notification changes nothing.
    pub fn set_display(&mut self, display: Display) -> bool {
        let changed = display != self.display;
        self.display = display;
        changed
    }

    pub fn resend_on_next_tick(&mut self) {
        self.last_sent = None;
    }

    pub fn order_failed(&mut self) {
        self.ordered = self.before_order;
        self.resend_on_next_tick();
    }

    pub fn tick(
        &mut self,
        read: Result<Vec<RawDockChild>, DockError>,
        cursor: Option<ScreenPoint>,
    ) -> Tick {
        let (frame, bounds, log) = match read {
            Ok(children) => {
                let bounds = dock_bounds(&children);
                let log = self.last_error.take().map(|_| TickLog::Recovered);
                (dock::app_items(children, self.display), bounds, log)
            }
            Err(error) => {
                let log = (self.last_error.as_ref() != Some(&error))
                    .then(|| TickLog::ReadFailed(error.clone()));
                self.last_error = Some(error);
                (Vec::new(), None, log)
            }
        };
        let send = if changed(self.last_sent.as_deref(), &frame) {
            self.last_sent = Some(frame.clone());
            Some(frame)
        } else {
            None
        };
        let overlay = send.as_deref().and_then(|sent| self.order(sent));
        let apps = send.as_deref().and_then(|sent| self.apps_changed(sent));
        Tick {
            send,
            log,
            overlay,
            apps,
            proximity_rate: poll_rate(cursor, bounds, self.display),
        }
    }

    fn apps_changed(&mut self, sent: &[DockItem]) -> Option<Vec<DockApp>> {
        if sent.is_empty() {
            return None;
        }
        let apps: Vec<DockApp> = sent
            .iter()
            .map(|item| DockApp {
                bundle_id: item.bundle_id.clone(),
                app_path: item.app_path.clone(),
            })
            .collect();
        if self.last_apps.as_ref() == Some(&apps) {
            return None;
        }
        self.last_apps = Some(apps.clone());
        Some(apps)
    }

    fn order(&mut self, sent: &[DockItem]) -> Option<Visibility> {
        let wanted = if sent.is_empty() {
            Visibility::Hidden
        } else {
            Visibility::Shown
        };
        if wanted == self.ordered {
            return None;
        }
        self.before_order = self.ordered;
        self.ordered = wanted;
        Some(wanted)
    }
}

pub fn changed(sent: Option<&[DockItem]>, next: &[DockItem]) -> bool {
    let Some(sent) = sent else {
        return true;
    };
    sent.len() != next.len()
        || sent.iter().zip(next).any(|(old, new)| {
            old.bundle_id != new.bundle_id
                || old.app_path != new.app_path
                || moved(old.rect.x, new.rect.x)
                || moved(old.rect.y, new.rect.y)
                || moved(old.rect.width, new.rect.width)
                || moved(old.rect.height, new.rect.height)
        })
}

fn moved(old: f64, new: f64) -> bool {
    (new - old).abs() >= MIN_RECT_CHANGE_PX
}

/// The union of every finite child rect (apps, folders, Trash, separators),
/// in AX screen points.
pub fn dock_bounds(children: &[RawDockChild]) -> Option<ScreenRect> {
    children
        .iter()
        .filter_map(|child| child.rect)
        .filter(|rect| rect.is_finite())
        .reduce(|a, b| {
            let left = a.x.min(b.x);
            let top = a.y.min(b.y);
            let right = (a.x + a.width).max(b.x + b.width);
            let bottom = (a.y + a.height).max(b.y + b.height);
            ScreenRect {
                x: left,
                y: top,
                width: right - left,
                height: bottom - top,
            }
        })
}

pub fn poll_rate(
    cursor: Option<ScreenPoint>,
    dock_bounds: Option<ScreenRect>,
    display: Display,
) -> PollRate {
    let Some(cursor) = cursor else {
        return PollRate::Slow;
    };
    let frame = display.frame();
    let near = match dock_bounds.filter(|bounds| overlaps(*bounds, frame)) {
        Some(bounds) => distance(cursor, bounds) <= PROXIMITY,
        None => near_bottom_edge(cursor, frame),
    };
    if near { PollRate::Fast } else { PollRate::Slow }
}

fn overlaps(a: ScreenRect, b: ScreenRect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

fn distance(point: ScreenPoint, rect: ScreenRect) -> f64 {
    let dx = (rect.x - point.x)
        .max(point.x - (rect.x + rect.width))
        .max(0.0);
    let dy = (rect.y - point.y)
        .max(point.y - (rect.y + rect.height))
        .max(0.0);
    dx.hypot(dy)
}

fn near_bottom_edge(point: ScreenPoint, frame: ScreenRect) -> bool {
    let bottom = frame.y + frame.height;
    (frame.x..=frame.x + frame.width).contains(&point.x)
        && (bottom - PROXIMITY..=bottom).contains(&point.y)
}

/// Keeps the poll at `Fast` only while something there moves: the pointer
/// moved since the last tick, or a frame was sent within `FAST_AFTER_SEND`.
#[derive(Debug, Default)]
pub struct Activity {
    last_cursor: Option<ScreenPoint>,
    last_sent: Option<Instant>,
    resting: bool,
}

impl Activity {
    pub fn rate(
        &mut self,
        proximity_rate: PollRate,
        cursor: Option<ScreenPoint>,
        sent: bool,
        now: Instant,
    ) -> PollRate {
        if sent {
            self.last_sent = Some(now);
        }
        let moved = cursor != self.last_cursor;
        self.last_cursor = cursor;
        let settling = self
            .last_sent
            .is_some_and(|sent| now.saturating_duration_since(sent) <= FAST_AFTER_SEND);
        let near = proximity_rate == PollRate::Fast;
        let moving = moved || settling;
        self.resting = near && !moving;
        if near && moving {
            PollRate::Fast
        } else {
            PollRate::Slow
        }
    }

    /// Waits, through `wait_until`, for the next Dock read, and returns the
    /// instant it became due: `read_due`, or sooner if the pointer rests in
    /// the band and moves. While it rests there, the pointer alone is checked
    /// every `REST_POINTER_CHECK_INTERVAL`, with no Dock read.
    pub fn wait_for_read(
        &self,
        read_due: Instant,
        now: Instant,
        mut wait_until: impl FnMut(Instant),
        mut cursor: impl FnMut() -> Option<ScreenPoint>,
    ) -> Instant {
        let mut checked = now;
        while self.resting
            && let Some(check) = checked.checked_add(REST_POINTER_CHECK_INTERVAL)
            && check < read_due
        {
            wait_until(check);
            if cursor() != self.last_cursor {
                return check;
            }
            checked = check;
        }
        wait_until(read_due);
        read_due
    }
}

/// When the tick after one due at `previous` is due: one interval later, so
/// a late wake-up or a slow read does not stretch the interval. When `now` is
/// already past that, the deadlines it missed are skipped, not run in a burst.
pub fn next_deadline(previous: Instant, rate: PollRate, now: Instant) -> Instant {
    let interval = rate.interval();
    let behind = now.saturating_duration_since(previous);
    let missed = behind.as_nanos() / interval.as_nanos().max(1);
    let intervals = u32::try_from(missed.saturating_add(1)).unwrap_or(u32::MAX);
    interval
        .checked_mul(intervals)
        .and_then(|ahead| previous.checked_add(ahead))
        .unwrap_or(now)
}

#[cfg(test)]
mod tests;
