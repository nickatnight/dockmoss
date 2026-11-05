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
/// With the Dock hidden, a pointer this close to the bottom edge may be
/// waiting for it to slide in.
const REVEAL_EDGE: f64 = 4.0;
/// `Fast` holds this long for a pointer resting at that edge: longer than the
/// Dock's default auto-hide delay, after which its slide sends frames. Bounded,
/// so a pointer parked over a hidden Dock goes back to `Slow`.
const REVEAL_WAIT: Duration = Duration::from_secs(1);

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
    /// The Dock is hidden and the pointer is at the edge it slides in from.
    pub awaits_reveal: bool,
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
            awaits_reveal: awaits_reveal(cursor, bounds, self.display),
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
        None => near_bottom_edge(cursor, frame, PROXIMITY),
    };
    if near { PollRate::Fast } else { PollRate::Slow }
}

pub fn awaits_reveal(
    cursor: Option<ScreenPoint>,
    dock_bounds: Option<ScreenRect>,
    display: Display,
) -> bool {
    let frame = display.frame();
    let hidden = !dock_bounds.is_some_and(|bounds| overlaps(bounds, frame));
    hidden && cursor.is_some_and(|cursor| near_bottom_edge(cursor, frame, REVEAL_EDGE))
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

fn near_bottom_edge(point: ScreenPoint, frame: ScreenRect, reach: f64) -> bool {
    let bottom = frame.y + frame.height;
    (frame.x..=frame.x + frame.width).contains(&point.x)
        && (bottom - reach..=bottom).contains(&point.y)
}

/// Keeps the poll at `Fast` only while something there moves: the pointer
/// moved since the last tick, a frame was sent within `FAST_AFTER_SEND`, or
/// the pointer moved within `REVEAL_WAIT` and waits at a hidden Dock's edge.
#[derive(Debug, Default)]
pub struct Activity {
    last_cursor: Option<ScreenPoint>,
    last_moved: Option<Instant>,
    last_sent: Option<Instant>,
    resting: bool,
}

impl Activity {
    pub fn rate(
        &mut self,
        proximity_rate: PollRate,
        awaits_reveal: bool,
        cursor: Option<ScreenPoint>,
        sent: bool,
        now: Instant,
    ) -> PollRate {
        if sent {
            self.last_sent = Some(now);
        }
        let moved = cursor != self.last_cursor;
        self.last_cursor = cursor;
        if moved {
            self.last_moved = Some(now);
        }
        let within = |since: Option<Instant>, window: Duration| {
            since.is_some_and(|since| now.saturating_duration_since(since) <= window)
        };
        let settling = within(self.last_sent, FAST_AFTER_SEND);
        let revealing = awaits_reveal && within(self.last_moved, REVEAL_WAIT);
        let near = proximity_rate == PollRate::Fast;
        let moving = moved || settling || revealing;
        self.resting = near && !moving;
        if near && moving {
            PollRate::Fast
        } else {
            PollRate::Slow
        }
    }

    pub fn wait_for_read(
        &self,
        read_due: Instant,
        now: Instant,
        mut wait_until: impl FnMut(Instant) -> Woken,
        mut cursor: impl FnMut() -> Option<ScreenPoint>,
    ) -> Instant {
        let longer_than_a_fast_interval =
            read_due.saturating_duration_since(now) > PollRate::Fast.interval();
        // A hover read comes no sooner than one Fast interval after the last
        // read, so hovers alone cannot read faster than Fast does.
        let earliest_hover_read = now.checked_add(PollRate::Fast.interval()).unwrap_or(now);
        let mut wait = |until: Instant| loop {
            match wait_until(until) {
                Woken::Due => return None,
                Woken::DockHover { at } if longer_than_a_fast_interval => {
                    if at < earliest_hover_read {
                        while wait_until(earliest_hover_read) != Woken::Due {}
                        return Some(earliest_hover_read);
                    }
                    return Some(at);
                }
                Woken::DockHover { .. } => {}
            }
        };
        let mut checked = now;
        while self.resting
            && let Some(check) = checked.checked_add(REST_POINTER_CHECK_INTERVAL)
            && check < read_due
        {
            if let Some(hover) = wait(check) {
                return hover;
            }
            if cursor() != self.last_cursor {
                return check;
            }
            checked = check;
        }
        wait(read_due).unwrap_or(read_due)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Woken {
    Due,
    DockHover { at: Instant },
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
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::{
        Activity, PollRate, Tick, TickLog, Tracker, Visibility, Woken, awaits_reveal, changed,
        dock_bounds, next_deadline, poll_rate,
    };
    use crate::bundle_id::BundleId;
    use crate::dock::{DockError, DockItem, RawDockChild};
    use crate::geometry::{Display, LogicalRect, ScreenPoint, ScreenRect};
    use crate::usage::DockApp;

    const PRIMARY_2X: Display = Display {
        physical_x: 0,
        physical_y: 0,
        physical_width: 3024,
        physical_height: 1964,
        scale_factor: 2.0,
    };

    const ICON: LogicalRect = LogicalRect {
        x: 700.0,
        y: 920.0,
        width: 56.0,
        height: 56.0,
    };

    fn item(bundle_id: &str, path: &str, rect: LogicalRect) -> DockItem {
        DockItem {
            bundle_id: BundleId::new(bundle_id).expect("non-empty bundle id"),
            app_path: PathBuf::from(path),
            rect,
        }
    }

    fn safari(rect: LogicalRect) -> DockItem {
        item("com.apple.Safari", "/Applications/Safari.app", rect)
    }

    fn finder(rect: LogicalRect) -> DockItem {
        item(
            "com.apple.finder",
            "/System/Library/CoreServices/Finder.app",
            rect,
        )
    }

    fn raw_app(bundle_id: &str, x: f64) -> RawDockChild {
        RawDockChild {
            subrole: Some("AXApplicationDockItem".into()),
            rect: Some(ScreenRect {
                x,
                y: 920.0,
                width: 56.0,
                height: 56.0,
            }),
            app_path: Some(PathBuf::from(format!("/Applications/{bundle_id}.app"))),
            bundle_id: Some(bundle_id.into()),
        }
    }

    fn raw_other(subrole: &str, rect: ScreenRect) -> RawDockChild {
        RawDockChild {
            subrole: Some(subrole.into()),
            rect: Some(rect),
            app_path: None,
            bundle_id: None,
        }
    }

    fn point(x: f64, y: f64) -> ScreenPoint {
        ScreenPoint { x, y }
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> ScreenRect {
        ScreenRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn nothing_sent_yet_is_a_change() {
        assert!(changed(None, &[safari(ICON)]));
        assert!(changed(None, &[]));
    }

    #[test]
    fn frame_changes_against_the_last_sent_frame() {
        let sent = [safari(ICON)];
        let moved = |rect: LogicalRect| vec![safari(rect)];
        let cases = [
            ("identical", vec![safari(ICON)], false),
            (
                "x drifts 0.49",
                moved(LogicalRect { x: 700.49, ..ICON }),
                false,
            ),
            (
                "x moves exactly 0.5",
                moved(LogicalRect { x: 700.5, ..ICON }),
                true,
            ),
            (
                "x moves -0.5",
                moved(LogicalRect { x: 699.5, ..ICON }),
                true,
            ),
            ("y moves 0.5", moved(LogicalRect { y: 920.5, ..ICON }), true),
            (
                "width grows 0.5",
                moved(LogicalRect {
                    width: 56.5,
                    ..ICON
                }),
                true,
            ),
            (
                "height grows 0.5",
                moved(LogicalRect {
                    height: 56.5,
                    ..ICON
                }),
                true,
            ),
            (
                "y drifts 0.49",
                moved(LogicalRect { y: 920.49, ..ICON }),
                false,
            ),
            (
                "width drifts 0.49",
                moved(LogicalRect {
                    width: 56.49,
                    ..ICON
                }),
                false,
            ),
            (
                "height drifts 0.49",
                moved(LogicalRect {
                    height: 56.49,
                    ..ICON
                }),
                false,
            ),
            ("one more item", vec![safari(ICON), finder(ICON)], true),
            ("one item fewer", vec![], true),
            (
                "another bundle id at the same index",
                vec![item(
                    "com.apple.Safari.beta",
                    "/Applications/Safari.app",
                    ICON,
                )],
                true,
            ),
            (
                "another app path at the same index",
                vec![item("com.apple.Safari", "/Applications/Safari 2.app", ICON)],
                true,
            ),
        ];

        for (case, next, expected) in cases {
            assert_eq!(changed(Some(&sent), &next), expected, "{case}");
        }
    }

    #[test]
    fn slow_drift_is_sent_once_it_adds_up_to_the_tolerance() {
        let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
        let first = tracker.tick(Ok(vec![raw_app("Safari", 700.0)]), None);
        let drifted = tracker.tick(Ok(vec![raw_app("Safari", 700.3)]), None);
        let drifted_again = tracker.tick(Ok(vec![raw_app("Safari", 700.6)]), None);

        assert!(first.send.is_some(), "first frame");
        assert_eq!(drifted.send, None, "0.3 from the sent frame");
        let sent_x = drifted_again
            .send
            .as_deref()
            .and_then(<[DockItem]>::first)
            .map(|item| item.rect.x);
        assert_eq!(sent_x, Some(700.6), "0.6 from the sent frame");
    }

    #[test]
    fn cursor_near_the_dock_polls_fast() {
        let visible_bottom_dock = Some(rect(440.0, 920.0, 630.0, 56.0));
        let cases = [
            (
                "inside the Dock box",
                Some(point(700.0, 950.0)),
                PollRate::Fast,
            ),
            (
                "150 pt above the box",
                Some(point(700.0, 770.0)),
                PollRate::Fast,
            ),
            (
                "150.5 pt above the box",
                Some(point(700.0, 769.5)),
                PollRate::Slow,
            ),
            (
                "150 pt left of the box",
                Some(point(290.0, 950.0)),
                PollRate::Fast,
            ),
            (
                "150.5 pt right of the box",
                Some(point(1220.5, 950.0)),
                PollRate::Slow,
            ),
            (
                "150 pt from a corner, diagonally",
                Some(point(440.0 - 106.0, 920.0 - 106.0)),
                PollRate::Fast,
            ),
            (
                "over 150 pt from a corner, diagonally",
                Some(point(440.0 - 107.0, 920.0 - 107.0)),
                PollRate::Slow,
            ),
            ("far away", Some(point(700.0, 100.0)), PollRate::Slow),
            ("cursor unknown", None, PollRate::Slow),
        ];

        for (case, cursor, expected) in cases {
            assert_eq!(
                poll_rate(cursor, visible_bottom_dock, PRIMARY_2X),
                expected,
                "{case}"
            );
        }
    }

    #[test]
    fn hidden_dock_measures_the_band_from_the_display_bottom_edge() {
        let hidden = Some(rect(440.0, 987.0, 630.0, 56.0));
        let cases = [
            (
                "hidden, 100 pt above the bottom edge",
                hidden,
                Some(point(700.0, 882.0)),
                PollRate::Fast,
            ),
            (
                "hidden, 400 pt above the bottom edge",
                hidden,
                Some(point(700.0, 582.0)),
                PollRate::Slow,
            ),
            (
                "hidden, 150 pt above the bottom edge and 155 pt from the hidden Dock",
                hidden,
                Some(point(700.0, 832.0)),
                PollRate::Fast,
            ),
            (
                "hidden, 150.5 pt above the bottom edge",
                hidden,
                Some(point(700.0, 831.5)),
                PollRate::Slow,
            ),
            (
                "Dock bounds off the display to the right, measured from the bottom edge",
                Some(rect(1600.0, 920.0, 630.0, 56.0)),
                Some(point(700.0, 882.0)),
                PollRate::Fast,
            ),
            (
                "Dock top exactly on the bottom edge counts as off the display",
                Some(rect(440.0, 982.0, 630.0, 56.0)),
                Some(point(100.0, 900.0)),
                PollRate::Fast,
            ),
            (
                "hidden, at the bottom edge",
                hidden,
                Some(point(700.0, 982.0)),
                PollRate::Fast,
            ),
            (
                "no read yet, 100 pt above the bottom edge",
                None,
                Some(point(700.0, 882.0)),
                PollRate::Fast,
            ),
            (
                "no read yet, 400 pt above the bottom edge",
                None,
                Some(point(700.0, 582.0)),
                PollRate::Slow,
            ),
            ("hidden, cursor unknown", hidden, None, PollRate::Slow),
            (
                "hidden, cursor on a display to the right",
                hidden,
                Some(point(1600.0, 900.0)),
                PollRate::Slow,
            ),
        ];

        for (case, dock, cursor, expected) in cases {
            assert_eq!(poll_rate(cursor, dock, PRIMARY_2X), expected, "{case}");
        }
    }

    #[test]
    fn dock_bounds_is_the_union_of_every_child_rect() {
        let children = [
            raw_app("Finder", 450.0),
            raw_other("AXSeparatorDockItem", rect(510.0, 925.0, 10.0, 50.0)),
            raw_app("Safari", 530.0),
            raw_other("AXTrashDockItem", rect(1000.0, 918.0, 60.0, 60.0)),
            RawDockChild {
                rect: None,
                ..raw_app("NoRect", 0.0)
            },
            RawDockChild {
                rect: Some(rect(f64::NAN, 900.0, 10.0, 10.0)),
                ..raw_app("Broken", 0.0)
            },
        ];

        assert_eq!(
            dock_bounds(&children),
            Some(rect(450.0, 918.0, 610.0, 60.0))
        );
    }

    #[test]
    fn dock_bounds_of_no_rects_is_none() {
        let children = [RawDockChild {
            rect: None,
            ..raw_app("NoRect", 0.0)
        }];

        assert_eq!(dock_bounds(&children), None);
        assert_eq!(dock_bounds(&[]), None);
    }

    #[test]
    fn tick_reports_the_rate_for_the_cursor_and_the_read() {
        let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);

        let near = tracker.tick(
            Ok(vec![raw_app("Safari", 700.0)]),
            Some(point(720.0, 850.0)),
        );
        let far = tracker.tick(
            Ok(vec![raw_app("Safari", 700.0)]),
            Some(point(720.0, 300.0)),
        );
        let failed_near_bottom =
            tracker.tick(Err(DockError::CannotComplete), Some(point(720.0, 900.0)));

        assert_eq!(near.proximity_rate, PollRate::Fast, "near the Dock");
        assert_eq!(far.proximity_rate, PollRate::Slow, "away from the Dock");
        assert_eq!(
            failed_near_bottom.proximity_rate,
            PollRate::Fast,
            "failed read, near the bottom edge"
        );
    }

    #[test]
    fn tick_reports_a_pointer_awaiting_a_reveal_for_the_cursor_and_the_read() {
        let hidden_read = || {
            Ok(vec![RawDockChild {
                rect: Some(rect(700.0, 990.0, 56.0, 56.0)),
                ..raw_app("Safari", 700.0)
            }])
        };
        let edge = Some(point(700.0, 982.0));
        let cases = [
            (
                "hidden Dock, pointer at the edge",
                hidden_read(),
                edge,
                true,
            ),
            (
                "hidden Dock, pointer 100 pt above the edge",
                hidden_read(),
                Some(point(700.0, 882.0)),
                false,
            ),
            (
                "shown Dock, pointer at the edge",
                Ok(vec![raw_app("Safari", 700.0)]),
                edge,
                false,
            ),
            (
                "unreadable Dock, pointer at the edge",
                Err(DockError::CannotComplete),
                edge,
                true,
            ),
        ];

        for (case, read, cursor, expected) in cases {
            let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
            assert_eq!(tracker.tick(read, cursor).awaits_reveal, expected, "{case}");
        }
    }

    #[test]
    fn tracker_sends_changes_and_logs_errors_once() {
        let safari_frame = || Ok(vec![raw_app("Safari", 700.0)]);
        let expected_frame = vec![item(
            "Safari",
            "/Applications/Safari.app",
            LogicalRect { x: 700.0, ..ICON },
        )];
        let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
        let steps = [
            (
                "first read sends",
                safari_frame(),
                Some(expected_frame.clone()),
                None,
            ),
            ("unchanged read sends nothing", safari_frame(), None, None),
            (
                "error sends one empty list and logs",
                Err(DockError::CannotComplete),
                Some(vec![]),
                Some(TickLog::ReadFailed(DockError::CannotComplete)),
            ),
            (
                "same error logs and sends nothing",
                Err(DockError::CannotComplete),
                None,
                None,
            ),
            (
                "different error logs",
                Err(DockError::NotRunning),
                None,
                Some(TickLog::ReadFailed(DockError::NotRunning)),
            ),
            (
                "recovery sends the frame and logs",
                safari_frame(),
                Some(expected_frame.clone()),
                Some(TickLog::Recovered),
            ),
            ("recovered and unchanged", safari_frame(), None, None),
            ("unchanged again", safari_frame(), None, None),
        ];

        for (case, read, send, log) in steps {
            let Tick {
                send: sent,
                log: logged,
                ..
            } = tracker.tick(read, None);

            assert_eq!((sent, logged), (send, log), "{case}");
        }
    }

    #[test]
    fn error_before_any_frame_sends_an_empty_list() {
        let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);

        let tick = tracker.tick(Err(DockError::ApiDisabled), None);

        assert_eq!(tick.send, Some(vec![]));
        assert_eq!(tick.log, Some(TickLog::ReadFailed(DockError::ApiDisabled)));
    }

    #[test]
    fn hidden_dock_after_an_error_sends_nothing_new() {
        let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
        let hidden_read = || {
            Ok(vec![raw_app("Safari", 700.0)]
                .into_iter()
                .map(|child| RawDockChild {
                    rect: Some(rect(700.0, 990.0, 56.0, 56.0)),
                    ..child
                })
                .collect())
        };

        let failed = tracker.tick(Err(DockError::CannotComplete), None);
        let hidden = tracker.tick(hidden_read(), None);

        assert_eq!(failed.send, Some(vec![]), "error");
        assert_eq!(hidden.send, None, "an empty frame after an empty list");
        assert_eq!(
            hidden.log,
            Some(TickLog::Recovered),
            "the read itself recovered"
        );
    }

    #[test]
    fn resend_on_next_tick_resends_the_current_frame() {
        let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
        tracker.tick(Ok(vec![raw_app("Safari", 700.0)]), None);

        tracker.resend_on_next_tick();
        let tick = tracker.tick(Ok(vec![raw_app("Safari", 700.0)]), None);

        assert_eq!(tick.send.map(|frame| frame.len()), Some(1));
    }

    #[test]
    fn resend_during_an_error_resends_the_empty_list_without_logging() {
        let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
        tracker.tick(Err(DockError::CannotComplete), None);

        tracker.resend_on_next_tick();
        let tick = tracker.tick(Err(DockError::CannotComplete), None);

        assert_eq!(tick.send, Some(vec![]));
        assert_eq!(tick.log, None);
    }

    #[test]
    fn new_display_converts_the_next_read_against_it() {
        let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
        tracker.tick(Ok(vec![raw_app("Safari", 700.0)]), None);

        let _changed = tracker.set_display(Display {
            physical_x: 0,
            physical_y: 0,
            physical_width: 1920,
            physical_height: 1080,
            scale_factor: 1.0,
        });
        let tick = tracker.tick(Ok(vec![raw_app("Safari", 700.0)]), None);

        assert_eq!(tick.send, None, "same AX rect, same overlay-local rect");
        let below_old_bottom = Ok(vec![RawDockChild {
            rect: Some(rect(700.0, 1000.0, 56.0, 56.0)),
            ..raw_app("Safari", 700.0)
        }]);
        let tick = tracker.tick(below_old_bottom, None);
        assert_eq!(
            tick.send.as_deref().map(<[DockItem]>::len),
            Some(1),
            "y 1000 is inside the 1080 pt display"
        );
    }

    #[test]
    fn overlay_is_ordered_out_for_an_empty_frame_and_in_for_a_full_one() {
        let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
        let hidden = || {
            Ok(vec![RawDockChild {
                rect: Some(rect(700.0, 990.0, 56.0, 56.0)),
                ..raw_app("Safari", 700.0)
            }])
        };
        let visible = || Ok(vec![raw_app("Safari", 700.0)]);
        let steps = [
            (
                "starts shown; a full frame changes nothing",
                visible(),
                None,
            ),
            ("Dock hides", hidden(), Some(Visibility::Hidden)),
            ("still hidden", hidden(), None),
            (
                "read fails while hidden",
                Err(DockError::CannotComplete),
                None,
            ),
            ("Dock shows", visible(), Some(Visibility::Shown)),
            (
                "Dock moves while shown",
                Ok(vec![raw_app("Safari", 720.0)]),
                None,
            ),
            (
                "read fails while shown",
                Err(DockError::CannotComplete),
                Some(Visibility::Hidden),
            ),
            ("recovers", visible(), Some(Visibility::Shown)),
        ];

        for (case, read, expected) in steps {
            assert_eq!(tracker.tick(read, None).overlay, expected, "{case}");
        }
    }

    #[test]
    fn first_empty_frame_orders_the_overlay_out() {
        let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);

        let tick = tracker.tick(Err(DockError::ApiDisabled), None);

        assert_eq!(tick.overlay, Some(Visibility::Hidden));
    }

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    struct RateCase {
        case: &'static str,
        proximity_rate: PollRate,
        cursor: Option<ScreenPoint>,
        sent: bool,
        at_ms: u64,
        expected: PollRate,
    }

    #[test]
    fn fast_holds_only_while_the_pointer_moves_or_a_frame_was_just_sent() {
        let start = Instant::now();
        let near = PollRate::Fast;
        let rest = Some(point(720.0, 850.0));
        let moved = Some(point(721.0, 850.0));
        let cases = [
            RateCase {
                case: "first tick near the Dock",
                proximity_rate: near,
                cursor: rest,
                sent: false,
                at_ms: 0,
                expected: PollRate::Fast,
            },
            RateCase {
                case: "pointer at rest, no frame for 300 ms",
                proximity_rate: near,
                cursor: rest,
                sent: false,
                at_ms: 300,
                expected: PollRate::Slow,
            },
            RateCase {
                case: "pointer moved",
                proximity_rate: near,
                cursor: moved,
                sent: false,
                at_ms: 310,
                expected: PollRate::Fast,
            },
            RateCase {
                case: "at rest, frame sent now",
                proximity_rate: near,
                cursor: moved,
                sent: true,
                at_ms: 320,
                expected: PollRate::Fast,
            },
            RateCase {
                case: "at rest, frame sent 200 ms ago",
                proximity_rate: near,
                cursor: moved,
                sent: false,
                at_ms: 520,
                expected: PollRate::Fast,
            },
            RateCase {
                case: "at rest, frame sent 250 ms ago",
                proximity_rate: near,
                cursor: moved,
                sent: false,
                at_ms: 570,
                expected: PollRate::Fast,
            },
            RateCase {
                case: "at rest, frame sent 300 ms ago",
                proximity_rate: near,
                cursor: moved,
                sent: false,
                at_ms: 620,
                expected: PollRate::Slow,
            },
            RateCase {
                case: "moving, but far from the Dock",
                proximity_rate: PollRate::Slow,
                cursor: Some(point(721.0, 300.0)),
                sent: true,
                at_ms: 630,
                expected: PollRate::Slow,
            },
            RateCase {
                case: "Activity alone: an unknown cursor after a known one counts as moved",
                proximity_rate: near,
                cursor: None,
                sent: false,
                at_ms: 1200,
                expected: PollRate::Fast,
            },
            RateCase {
                case: "Activity alone: an unknown cursor that stays unknown is at rest",
                proximity_rate: near,
                cursor: None,
                sent: false,
                at_ms: 1500,
                expected: PollRate::Slow,
            },
        ];
        let mut activity = Activity::default();

        for step in cases {
            let now = start.checked_add(ms(step.at_ms)).expect("in range");
            let rate = activity.rate(step.proximity_rate, false, step.cursor, step.sent, now);
            assert_eq!(rate, step.expected, "{}", step.case);
        }
    }

    struct RevealCase {
        case: &'static str,
        awaits_reveal: bool,
        cursor: Option<ScreenPoint>,
        at_ms: u64,
        expected: PollRate,
    }

    #[test]
    fn a_pointer_at_a_hidden_docks_edge_holds_fast_for_a_bounded_wait() {
        let start = Instant::now();
        let edge = Some(point(700.0, 982.0));
        let above = Some(point(700.0, 882.0));
        let cases = [
            RevealCase {
                case: "hidden Dock, the pointer reaches the edge",
                awaits_reveal: true,
                cursor: edge,
                at_ms: 0,
                expected: PollRate::Fast,
            },
            RevealCase {
                case: "hidden Dock, at rest at the edge for 500 ms",
                awaits_reveal: true,
                cursor: edge,
                at_ms: 500,
                expected: PollRate::Fast,
            },
            RevealCase {
                case: "hidden Dock, at rest at the edge for 1000 ms",
                awaits_reveal: true,
                cursor: edge,
                at_ms: 1000,
                expected: PollRate::Fast,
            },
            RevealCase {
                case: "hidden Dock, at rest at the edge for 1100 ms: the wait is over",
                awaits_reveal: true,
                cursor: edge,
                at_ms: 1100,
                expected: PollRate::Slow,
            },
            RevealCase {
                case: "hidden Dock, moves 100 pt above the edge",
                awaits_reveal: false,
                cursor: above,
                at_ms: 1200,
                expected: PollRate::Fast,
            },
            RevealCase {
                case: "hidden Dock, at rest 100 pt above the edge",
                awaits_reveal: false,
                cursor: above,
                at_ms: 1500,
                expected: PollRate::Slow,
            },
            RevealCase {
                case: "hidden Dock, back at the edge: a new move opens a new wait",
                awaits_reveal: true,
                cursor: edge,
                at_ms: 1600,
                expected: PollRate::Fast,
            },
            RevealCase {
                case: "hidden Dock, at rest at the edge 500 ms after the later move",
                awaits_reveal: true,
                cursor: edge,
                at_ms: 2100,
                expected: PollRate::Fast,
            },
            RevealCase {
                case: "hidden Dock, at rest at the edge 1000 ms after the later move",
                awaits_reveal: true,
                cursor: edge,
                at_ms: 2600,
                expected: PollRate::Fast,
            },
            RevealCase {
                case: "hidden Dock, at rest at the edge 1100 ms after the later move",
                awaits_reveal: true,
                cursor: edge,
                at_ms: 2700,
                expected: PollRate::Slow,
            },
        ];
        let mut activity = Activity::default();

        for step in cases {
            let now = start.checked_add(ms(step.at_ms)).expect("in range");
            let rate = activity.rate(PollRate::Fast, step.awaits_reveal, step.cursor, false, now);
            assert_eq!(rate, step.expected, "{}", step.case);
        }
    }

    #[test]
    fn a_pointer_awaits_a_reveal_only_at_a_hidden_docks_bottom_edge() {
        let hidden = Some(rect(440.0, 987.0, 630.0, 56.0));
        let shown = Some(rect(440.0, 920.0, 630.0, 56.0));
        let cases = [
            (
                "hidden, at the bottom edge",
                hidden,
                Some(point(700.0, 982.0)),
                true,
            ),
            (
                "hidden, 4 pt above the edge",
                hidden,
                Some(point(700.0, 978.0)),
                true,
            ),
            (
                "hidden, 5 pt above the edge",
                hidden,
                Some(point(700.0, 977.0)),
                false,
            ),
            (
                "no read, at the bottom edge",
                None,
                Some(point(700.0, 982.0)),
                true,
            ),
            (
                "shown, at the bottom edge",
                shown,
                Some(point(700.0, 982.0)),
                false,
            ),
            ("hidden, cursor unknown", hidden, None, false),
            (
                "hidden, left of the display",
                hidden,
                Some(point(-10.0, 982.0)),
                false,
            ),
        ];

        for (case, bounds, cursor, expected) in cases {
            assert_eq!(
                awaits_reveal(cursor, bounds, PRIMARY_2X),
                expected,
                "{case}"
            );
        }
    }

    fn wait_for_read(
        activity: &Activity,
        start: Instant,
        now_ms: u64,
        read_due_ms: u64,
        pointer: &[Option<ScreenPoint>],
    ) -> (u64, Vec<u64>) {
        let (read, checks) =
            wait_for_read_with_hovers(activity, start, ms(now_ms), ms(read_due_ms), pointer, &[]);
        (u64::try_from(read.as_millis()).expect("in range"), checks)
    }

    /// Runs `wait_for_read` on a simulated clock at `start` plus the given
    /// offsets. A wait whose end is at or after the next of `hovers` is woken
    /// there by a Dock hover change. Returns when the read was due and the
    /// pointer checks (ms), and checks that the last wait ended then.
    fn wait_for_read_with_hovers(
        activity: &Activity,
        start: Instant,
        now: Duration,
        read_due: Duration,
        pointer: &[Option<ScreenPoint>],
        hovers: &[Duration],
    ) -> (Duration, Vec<u64>) {
        let at = |offset: Duration| start.checked_add(offset).expect("in range");
        let since_start = |instant: Instant| {
            u64::try_from(instant.duration_since(start).as_millis()).expect("in range")
        };
        let clock = std::cell::Cell::new(at(now));
        let mut hovers = hovers.iter().map(|&hover| at(hover)).peekable();
        let mut checks = Vec::new();
        let read = activity.wait_for_read(
            at(read_due),
            at(now),
            |until| {
                if let Some(hover) = hovers.next_if(|&hover| hover <= until) {
                    clock.set(hover.max(clock.get()));
                    return Woken::DockHover { at: clock.get() };
                }
                clock.set(until);
                Woken::Due
            },
            || {
                let pointer = pointer
                    .get(checks.len())
                    .or(pointer.last())
                    .copied()
                    .flatten();
                checks.push(since_start(clock.get()));
                pointer
            },
        );
        assert_eq!(clock.get(), read, "the wait ended when the read was due");
        (read.duration_since(start), checks)
    }

    struct RestCase {
        case: &'static str,
        proximity_rate: PollRate,
        cursor_at_read: Option<ScreenPoint>,
        pointer_at_checks: Vec<Option<ScreenPoint>>,
        expected_read_ms: u64,
        expected_checks_ms: Vec<u64>,
    }

    #[test]
    fn a_pointer_resting_in_the_band_is_checked_at_10_hz_until_it_moves_or_the_read_is_due() {
        const NOW_MS: u64 = 300;
        const READ_DUE_MS: u64 = 800;
        let start = Instant::now();
        let rest = Some(point(720.0, 850.0));
        let moved = Some(point(722.0, 850.0));
        let cases = [
            RestCase {
                case: "at rest: checks every 100 ms, then the Slow read",
                proximity_rate: PollRate::Fast,
                cursor_at_read: rest,
                pointer_at_checks: vec![rest],
                expected_read_ms: 800,
                expected_checks_ms: vec![400, 500, 600, 700],
            },
            RestCase {
                case: "moves before the first check: read at that check",
                proximity_rate: PollRate::Fast,
                cursor_at_read: rest,
                pointer_at_checks: vec![moved],
                expected_read_ms: 400,
                expected_checks_ms: vec![400],
            },
            RestCase {
                case: "moves before the third check: read at that check",
                proximity_rate: PollRate::Fast,
                cursor_at_read: rest,
                pointer_at_checks: vec![rest, rest, moved],
                expected_read_ms: 600,
                expected_checks_ms: vec![400, 500, 600],
            },
            RestCase {
                case: "cursor unknown at rest: checks, then the Slow read",
                proximity_rate: PollRate::Fast,
                cursor_at_read: None,
                pointer_at_checks: vec![None],
                expected_read_ms: 800,
                expected_checks_ms: vec![400, 500, 600, 700],
            },
            RestCase {
                case: "away from the Dock: no checks",
                proximity_rate: PollRate::Slow,
                cursor_at_read: Some(point(720.0, 300.0)),
                pointer_at_checks: vec![moved],
                expected_read_ms: 800,
                expected_checks_ms: vec![],
            },
        ];

        for RestCase {
            case,
            proximity_rate,
            cursor_at_read,
            pointer_at_checks,
            expected_read_ms,
            expected_checks_ms,
        } in cases
        {
            let mut activity = Activity::default();
            activity.rate(proximity_rate, false, cursor_at_read, false, start);
            let rate = activity.rate(
                proximity_rate,
                false,
                cursor_at_read,
                false,
                start.checked_add(ms(NOW_MS)).expect("in range"),
            );
            assert_eq!(rate, PollRate::Slow, "{case}");

            let (read, checks) =
                wait_for_read(&activity, start, NOW_MS, READ_DUE_MS, &pointer_at_checks);

            assert_eq!(read, expected_read_ms, "{case}: read");
            assert_eq!(checks, expected_checks_ms, "{case}: checks");
        }
    }

    #[test]
    fn a_moving_pointer_near_the_dock_gets_no_pointer_checks_between_fast_reads() {
        let start = Instant::now();
        let mut activity = Activity::default();
        let rate = activity.rate(
            PollRate::Fast,
            false,
            Some(point(720.0, 850.0)),
            false,
            start,
        );
        assert_eq!(rate, PollRate::Fast);

        let (read, checks) = wait_for_read(&activity, start, 0, 17, &[Some(point(730.0, 850.0))]);

        assert_eq!(read, 17);
        assert_eq!(checks, Vec::<u64>::new());
    }

    #[test]
    fn a_magnification_started_from_rest_is_read_at_fast_within_one_check() {
        let start = Instant::now();
        let rest = Some(point(720.0, 850.0));
        let moved = Some(point(724.0, 850.0));
        let mut activity = Activity::default();
        activity.rate(PollRate::Fast, false, rest, false, start);
        assert_eq!(
            activity.rate(
                PollRate::Fast,
                false,
                rest,
                false,
                start.checked_add(ms(300)).expect("in range")
            ),
            PollRate::Slow
        );

        let (read, _) = wait_for_read(&activity, start, 300, 800, &[moved]);
        let rate = activity.rate(
            PollRate::Fast,
            false,
            moved,
            false,
            start.checked_add(ms(read)).expect("in range"),
        );

        assert!(read - 300 <= 100, "read {read} ms");
        assert_eq!(rate, PollRate::Fast);
    }

    /// An `Activity` whose pointer has stood still at `cursor` since `start`,
    /// as of `now_ms`: the second `rate` call sees no move, so the rate is
    /// `Slow` and, in the band, the pointer rests.
    fn settled_activity(
        proximity_rate: PollRate,
        cursor: Option<ScreenPoint>,
        start: Instant,
        now_ms: u64,
    ) -> Activity {
        let mut activity = Activity::default();
        activity.rate(proximity_rate, false, cursor, false, start);
        activity.rate(
            proximity_rate,
            false,
            cursor,
            false,
            start.checked_add(ms(now_ms)).expect("in range"),
        );
        activity
    }

    const HOVER_NOW_MS: u64 = 300;

    const FAR: ScreenPoint = ScreenPoint { x: 720.0, y: 300.0 };
    const IN_BAND: ScreenPoint = ScreenPoint { x: 720.0, y: 850.0 };

    /// Still and away from the Dock: `Slow`, with no pointer checks.
    fn still_far(start: Instant) -> Activity {
        settled_activity(PollRate::Slow, Some(FAR), start, HOVER_NOW_MS)
    }

    /// Still in the band: `Slow`, with a pointer check every 100 ms.
    fn resting_in_band(start: Instant) -> Activity {
        settled_activity(PollRate::Fast, Some(IN_BAND), start, HOVER_NOW_MS)
    }

    /// Moving in the band: `Fast`.
    fn moving_in_band(start: Instant) -> Activity {
        let mut activity = Activity::default();
        activity.rate(PollRate::Fast, false, Some(IN_BAND), false, start);
        let rate = activity.rate(
            PollRate::Fast,
            false,
            Some(point(724.0, 850.0)),
            false,
            start.checked_add(ms(HOVER_NOW_MS)).expect("in range"),
        );
        assert_eq!(rate, PollRate::Fast);
        activity
    }

    struct HoverCase {
        case: &'static str,
        activity: fn(Instant) -> Activity,
        read_due: Duration,
        hovers: Vec<Duration>,
        expected_read: Duration,
        expected_checks_ms: Vec<u64>,
    }

    #[test]
    fn a_dock_hover_change_makes_a_slow_read_due_at_once_and_leaves_fast_reads_alone() {
        let start = Instant::now();
        let now = ms(HOVER_NOW_MS);
        let fast = PollRate::Fast.interval();
        let fast_read_due = next_deadline(
            start.checked_add(ms(297)).expect("in range"),
            PollRate::Fast,
            start.checked_add(now).expect("in range"),
        )
        .duration_since(start);
        let cases = [
            HoverCase {
                case: "slow, pointer away from the Dock: read at the hover",
                activity: still_far,
                read_due: ms(800),
                hovers: vec![ms(420)],
                expected_read: ms(420),
                expected_checks_ms: vec![],
            },
            HoverCase {
                case: "slow, pointer resting in the band: read at the hover, before the next check",
                activity: resting_in_band,
                read_due: ms(800),
                hovers: vec![ms(350)],
                expected_read: ms(350),
                expected_checks_ms: vec![],
            },
            HoverCase {
                case: "slow, pointer resting in the band: a hover after one pointer check",
                activity: resting_in_band,
                read_due: ms(800),
                hovers: vec![ms(450)],
                expected_read: ms(450),
                expected_checks_ms: vec![400],
            },
            HoverCase {
                case: "slow: the first of a pair of hovers makes the read due",
                activity: still_far,
                read_due: ms(800),
                hovers: vec![ms(420), ms(422)],
                expected_read: ms(420),
                expected_checks_ms: vec![],
            },
            HoverCase {
                case: "slow, no hover: the Slow read",
                activity: still_far,
                read_due: ms(800),
                hovers: vec![],
                expected_read: ms(800),
                expected_checks_ms: vec![],
            },
            HoverCase {
                case: "slow: a hover 2 ms after the last read is read one Fast interval after it",
                activity: still_far,
                read_due: ms(800),
                hovers: vec![ms(302)],
                expected_read: now + fast,
                expected_checks_ms: vec![],
            },
            HoverCase {
                case: "fast: hovers between Fast reads wait for the Fast read",
                activity: moving_in_band,
                read_due: fast_read_due,
                hovers: vec![ms(305), ms(306)],
                expected_read: fast_read_due,
                expected_checks_ms: vec![],
            },
            HoverCase {
                case: "a wait of exactly one Fast interval: the hover changes nothing",
                activity: still_far,
                read_due: now + fast,
                hovers: vec![ms(305)],
                expected_read: now + fast,
                expected_checks_ms: vec![],
            },
        ];

        for HoverCase {
            case,
            activity,
            read_due,
            hovers,
            expected_read,
            expected_checks_ms,
        } in cases
        {
            let activity = activity(start);
            let cursor = if case.contains("band") {
                Some(IN_BAND)
            } else {
                Some(FAR)
            };

            let (read, checks) =
                wait_for_read_with_hovers(&activity, start, now, read_due, &[cursor], &hovers);

            assert_eq!(read, expected_read, "{case}: read");
            assert_eq!(checks, expected_checks_ms, "{case}: checks");
        }
    }

    #[test]
    fn a_pair_of_hovers_with_the_pointer_still_reads_at_most_once_per_fast_interval() {
        let start = Instant::now();
        let activity = still_far(start);

        let (first, _) = wait_for_read_with_hovers(
            &activity,
            start,
            ms(HOVER_NOW_MS),
            ms(800),
            &[Some(FAR)],
            &[ms(420), ms(422)],
        );
        let (second, _) =
            wait_for_read_with_hovers(&activity, start, ms(421), ms(921), &[Some(FAR)], &[ms(422)]);

        assert_eq!(first, ms(420));
        assert_eq!(second, ms(421) + PollRate::Fast.interval());
    }

    #[test]
    fn deadlines_follow_the_previous_deadline_not_the_end_of_the_read() {
        let start = Instant::now();
        let fast = PollRate::Fast.interval();
        let at = |nanos: u64| {
            start
                .checked_add(Duration::from_nanos(nanos))
                .expect("in range")
        };

        assert_eq!(
            next_deadline(start, PollRate::Fast, at(5_500_000)),
            at(16_666_667)
        );
        assert_eq!(
            next_deadline(start, PollRate::Slow, at(5_000_000)),
            at(500_000_000)
        );
        assert_eq!(
            next_deadline(at(16_666_667), PollRate::Fast, at(20_000_000)),
            at(33_333_334)
        );
        let overrun = next_deadline(start, PollRate::Fast, at(40_000_000));
        assert_eq!(overrun, at(50_000_001));
        assert!(overrun > at(40_000_000));
        assert_eq!(
            next_deadline(start, PollRate::Fast, at(16_666_667)),
            start.checked_add(fast * 2).expect("in range")
        );
    }

    #[test]
    fn sixty_deadlines_span_one_second() {
        let start = Instant::now();
        let mut deadline = start;
        for _ in 0..60 {
            let now = deadline
                .checked_add(Duration::from_micros(3_500))
                .expect("in range");
            deadline = next_deadline(deadline, PollRate::Fast, now);
        }

        let span = deadline.duration_since(start);
        assert!(
            span.abs_diff(Duration::from_secs(1)) < Duration::from_micros(1),
            "{span:?}"
        );
    }

    #[test]
    fn a_dock_that_stops_answering_or_running_clears_and_orders_out_on_that_tick() {
        for error in [DockError::CannotComplete, DockError::NotRunning] {
            let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
            tracker.tick(Ok(vec![raw_app("Safari", 700.0)]), None);

            let tick = tracker.tick(Err(error.clone()), None);

            assert_eq!(tick.send, Some(Vec::new()), "{error}");
            assert_eq!(tick.overlay, Some(Visibility::Hidden), "{error}");
        }
    }

    #[test]
    fn rates_are_60_hz_and_2_hz() {
        assert_eq!(PollRate::Fast.interval(), Duration::from_nanos(16_666_667));
        assert_eq!(PollRate::Slow.interval(), Duration::from_millis(500));
    }

    #[test]
    fn set_display_reports_whether_the_display_changed() {
        let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
        let external = Display {
            physical_x: 0,
            physical_y: 0,
            physical_width: 1920,
            physical_height: 1080,
            scale_factor: 1.0,
        };

        assert!(!tracker.set_display(PRIMARY_2X), "same display");
        assert!(tracker.set_display(external), "new display");
        assert!(!tracker.set_display(external), "same again");
    }

    #[test]
    fn tracker_started_hidden_orders_the_overlay_in_only_for_a_full_frame() {
        let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Hidden);

        let empty = tracker.tick(Err(DockError::ApiDisabled), None);
        let full = tracker.tick(Ok(vec![raw_app("Safari", 700.0)]), None);

        assert_eq!(empty.overlay, None, "already hidden");
        assert_eq!(full.overlay, Some(Visibility::Shown));
    }

    #[test]
    fn a_failed_ordering_is_issued_again_on_the_next_tick() {
        let cases = [
            (
                "ordering out failed",
                Visibility::Shown,
                Err(DockError::CannotComplete),
                Visibility::Hidden,
            ),
            (
                "ordering in failed",
                Visibility::Hidden,
                Ok(vec![raw_app("Safari", 700.0)]),
                Visibility::Shown,
            ),
        ];
        for (case, start, read, ordered) in cases {
            let mut retried = Tracker::new(PRIMARY_2X, start);
            let mut control = Tracker::new(PRIMARY_2X, start);
            assert_eq!(
                retried.tick(read.clone(), None).overlay,
                Some(ordered),
                "{case}"
            );
            assert_eq!(
                control.tick(read.clone(), None).overlay,
                Some(ordered),
                "{case}"
            );

            retried.order_failed();

            assert_eq!(
                retried.tick(read.clone(), None).overlay,
                Some(ordered),
                "{case}: issued again"
            );
            assert_eq!(
                control.tick(read, None).overlay,
                None,
                "{case}: without the failure the ordering stands"
            );
        }
    }

    #[test]
    fn a_failed_ordering_after_a_successful_one_is_issued_again() {
        let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
        let full = || Ok(vec![raw_app("Safari", 700.0)]);
        assert_eq!(
            tracker.tick(Err(DockError::CannotComplete), None).overlay,
            Some(Visibility::Hidden),
            "the first ordering, out, succeeds"
        );
        assert_eq!(
            tracker.tick(full(), None).overlay,
            Some(Visibility::Shown),
            "the second ordering, in, is issued"
        );

        tracker.order_failed();

        assert_eq!(
            tracker.tick(full(), None).overlay,
            Some(Visibility::Shown),
            "the failed ordering in is issued again"
        );
    }

    fn dock_app(bundle_id: &str) -> DockApp {
        DockApp {
            bundle_id: BundleId::new(bundle_id).expect("non-empty bundle id"),
            app_path: PathBuf::from(format!("/Applications/{bundle_id}.app")),
        }
    }

    #[test]
    fn tick_reports_the_dock_apps_when_the_list_changes() {
        let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
        let hidden = || {
            Ok(vec![RawDockChild {
                rect: Some(rect(700.0, 990.0, 56.0, 56.0)),
                ..raw_app("Safari", 700.0)
            }])
        };
        let steps = [
            (
                "first frame",
                Ok(vec![raw_app("Safari", 700.0)]),
                Some(vec![dock_app("Safari")]),
            ),
            (
                "an app added",
                Ok(vec![raw_app("Safari", 700.0), raw_app("Chess", 760.0)]),
                Some(vec![dock_app("Safari"), dock_app("Chess")]),
            ),
            (
                "rects move (magnification)",
                Ok(vec![raw_app("Safari", 690.0), raw_app("Chess", 770.0)]),
                None,
            ),
            ("Dock hidden: an empty frame", hidden(), None),
            (
                "read fails: an empty frame",
                Err(DockError::CannotComplete),
                None,
            ),
            (
                "shown again with the same list",
                Ok(vec![raw_app("Safari", 700.0), raw_app("Chess", 760.0)]),
                None,
            ),
            (
                "reordered",
                Ok(vec![raw_app("Chess", 700.0), raw_app("Safari", 760.0)]),
                Some(vec![dock_app("Chess"), dock_app("Safari")]),
            ),
            ("hidden again", hidden(), None),
            (
                "shown with an app removed",
                Ok(vec![raw_app("Chess", 700.0)]),
                Some(vec![dock_app("Chess")]),
            ),
        ];

        for (case, read, apps) in steps {
            assert_eq!(tracker.tick(read, None).apps, apps, "{case}");
        }
    }

    #[test]
    fn a_resent_frame_does_not_report_the_apps_again() {
        let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
        tracker.tick(Ok(vec![raw_app("Safari", 700.0)]), None);

        tracker.resend_on_next_tick();
        let tick = tracker.tick(Ok(vec![raw_app("Safari", 700.0)]), None);

        assert!(tick.send.is_some(), "the frame is resent");
        assert_eq!(tick.apps, None);
    }
}
