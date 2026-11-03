use std::path::PathBuf;
use std::time::Duration;

use super::{
    PollRate, Tick, TickLog, Tracker, Visibility, changed, dock_bounds, next_delay, poll_rate,
};
use crate::bundle_id::BundleId;
use crate::dock::{DockError, DockItem, RawDockChild};
use crate::geometry::{Display, LogicalRect, ScreenPoint, ScreenRect};

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
    let mut tracker = Tracker::new(PRIMARY_2X);
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
    let dock = Some(rect(440.0, 920.0, 630.0, 56.0));
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
        assert_eq!(poll_rate(cursor, dock, PRIMARY_2X), expected, "{case}");
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
    let mut tracker = Tracker::new(PRIMARY_2X);

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

    assert_eq!(near.rate, PollRate::Fast, "near the Dock");
    assert_eq!(far.rate, PollRate::Slow, "away from the Dock");
    assert_eq!(
        failed_near_bottom.rate,
        PollRate::Fast,
        "failed read, near the bottom edge"
    );
}

#[test]
fn tracker_sends_changes_and_logs_errors_once() {
    let safari_frame = || Ok(vec![raw_app("Safari", 700.0)]);
    let expected_frame = vec![item(
        "Safari",
        "/Applications/Safari.app",
        LogicalRect { x: 700.0, ..ICON },
    )];
    let mut tracker = Tracker::new(PRIMARY_2X);
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
    let mut tracker = Tracker::new(PRIMARY_2X);

    let tick = tracker.tick(Err(DockError::ApiDisabled), None);

    assert_eq!(tick.send, Some(vec![]));
    assert_eq!(tick.log, Some(TickLog::ReadFailed(DockError::ApiDisabled)));
}

#[test]
fn hidden_dock_after_an_error_sends_nothing_new() {
    let mut tracker = Tracker::new(PRIMARY_2X);
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
fn resubscribe_resends_the_current_frame() {
    let mut tracker = Tracker::new(PRIMARY_2X);
    tracker.tick(Ok(vec![raw_app("Safari", 700.0)]), None);

    tracker.resubscribe();
    let tick = tracker.tick(Ok(vec![raw_app("Safari", 700.0)]), None);

    assert_eq!(tick.send.map(|frame| frame.len()), Some(1));
}

#[test]
fn resubscribe_during_an_error_resends_the_empty_list_without_logging() {
    let mut tracker = Tracker::new(PRIMARY_2X);
    tracker.tick(Err(DockError::CannotComplete), None);

    tracker.resubscribe();
    let tick = tracker.tick(Err(DockError::CannotComplete), None);

    assert_eq!(tick.send, Some(vec![]));
    assert_eq!(tick.log, None);
}

#[test]
fn new_display_converts_the_next_read_against_it() {
    let mut tracker = Tracker::new(PRIMARY_2X);
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
    let mut tracker = Tracker::new(PRIMARY_2X);
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
    let mut tracker = Tracker::new(PRIMARY_2X);

    let tick = tracker.tick(Err(DockError::ApiDisabled), None);

    assert_eq!(tick.overlay, Some(Visibility::Hidden));
}

#[test]
fn next_delay_is_the_interval_minus_the_read() {
    let fast = PollRate::Fast.interval();
    let slow = PollRate::Slow.interval();
    let cases = [
        (
            "fast, short read",
            PollRate::Fast,
            Duration::from_millis(5),
            Duration::from_nanos(11_666_667),
        ),
        (
            "fast, read equal to the interval",
            PollRate::Fast,
            fast,
            Duration::ZERO,
        ),
        (
            "fast, read longer than the interval",
            PollRate::Fast,
            Duration::from_millis(40),
            Duration::ZERO,
        ),
        (
            "slow, short read",
            PollRate::Slow,
            Duration::from_millis(5),
            Duration::from_millis(495),
        ),
        (
            "slow, read equal to the interval",
            PollRate::Slow,
            slow,
            Duration::ZERO,
        ),
        (
            "slow, read longer than the interval",
            PollRate::Slow,
            Duration::from_secs(2),
            Duration::ZERO,
        ),
    ];

    for (case, rate, read_took, expected) in cases {
        assert_eq!(next_delay(rate, read_took), expected, "{case}");
    }
}

#[test]
fn rates_are_60_hz_and_2_hz() {
    assert_eq!(PollRate::Fast.interval(), Duration::from_nanos(16_666_667));
    assert_eq!(PollRate::Slow.interval(), Duration::from_millis(500));
}

#[test]
fn set_display_reports_whether_the_display_changed() {
    let mut tracker = Tracker::new(PRIMARY_2X);
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
