//! Decides, for each Dock read, what to send to the overlay, what to log,
//! whether the overlay window should be shown, and how soon to read again.
//! The `dock-tracker` thread in `lib.rs` only performs what this returns.

use std::time::Duration;

use crate::dock::{self, DockError, DockItem, RawDockChild};
use crate::geometry::{Display, ScreenPoint, ScreenRect};

/// A rect field must move this far from the last frame sent to count as a change.
const TOLERANCE: f64 = 0.5;
/// The cursor polls fast within this many points of the Dock.
const PROXIMITY: f64 = 150.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollRate {
    /// 60 Hz, while the cursor is near the Dock and magnification may run.
    Fast,
    /// 2 Hz, otherwise.
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
    pub rate: PollRate,
}

pub struct Tracker {
    display: Display,
    last_sent: Option<Vec<DockItem>>,
    last_error: Option<DockError>,
    overlay: Visibility,
}

impl Tracker {
    /// The overlay window is shown at startup, so the tracker starts there.
    pub fn new(display: Display) -> Self {
        Self {
            display,
            last_sent: None,
            last_error: None,
            overlay: Visibility::Shown,
        }
    }

    pub fn set_display(&mut self, display: Display) {
        self.display = display;
    }

    /// A new subscriber has seen nothing, so the next tick sends the current frame.
    pub fn resubscribe(&mut self) {
        self.last_sent = None;
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
        let send = changed(self.last_sent.as_deref(), &frame).then(|| {
            self.last_sent = Some(frame.clone());
            frame
        });
        let overlay = send.as_deref().and_then(|sent| self.order(sent));
        Tick {
            send,
            log,
            overlay,
            rate: poll_rate(cursor, bounds, self.display),
        }
    }

    fn order(&mut self, sent: &[DockItem]) -> Option<Visibility> {
        let wanted = if sent.is_empty() {
            Visibility::Hidden
        } else {
            Visibility::Shown
        };
        (wanted != self.overlay).then(|| {
            self.overlay = wanted;
            wanted
        })
    }
}

/// True when `next` differs from the last frame sent: nothing sent yet, a
/// different length, another app at any index, or any rect field moved by
/// at least `TOLERANCE`. Comparing against the last frame *sent* means slow
/// drift is still sent once it adds up.
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
    (new - old).abs() >= TOLERANCE
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

/// `Fast` when the cursor is within `PROXIMITY` of the Dock's bounds. When
/// those bounds are off the display (an auto-hidden Dock, or no read yet),
/// the band is measured from the display's bottom edge instead, so moving
/// toward a hidden Dock speeds the poll up before it slides in.
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

/// How long to wait before the next read. A read that overran its interval
/// is followed by the next one at once; missed ticks are skipped, not caught up.
pub fn next_delay(rate: PollRate, read_took: Duration) -> Duration {
    rate.interval().saturating_sub(read_took)
}

#[cfg(test)]
mod tests;
