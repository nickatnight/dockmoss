use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{
    Activity, PollRate, Tick, TickLog, Tracker, Visibility, changed, dock_bounds, next_deadline,
    poll_rate,
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

#[test]
fn fast_holds_only_while_the_pointer_moves_or_a_frame_was_just_sent() {
    let start = Instant::now();
    let near = PollRate::Fast;
    let cases = [
        (
            "first tick near the Dock",
            near,
            Some(point(720.0, 850.0)),
            false,
            0,
            PollRate::Fast,
        ),
        (
            "pointer at rest, no frame for 300 ms",
            near,
            Some(point(720.0, 850.0)),
            false,
            300,
            PollRate::Slow,
        ),
        (
            "pointer moved",
            near,
            Some(point(721.0, 850.0)),
            false,
            310,
            PollRate::Fast,
        ),
        (
            "at rest, frame sent now",
            near,
            Some(point(721.0, 850.0)),
            true,
            320,
            PollRate::Fast,
        ),
        (
            "at rest, frame sent 200 ms ago",
            near,
            Some(point(721.0, 850.0)),
            false,
            520,
            PollRate::Fast,
        ),
        (
            "at rest, frame sent 250 ms ago",
            near,
            Some(point(721.0, 850.0)),
            false,
            570,
            PollRate::Fast,
        ),
        (
            "at rest, frame sent 300 ms ago",
            near,
            Some(point(721.0, 850.0)),
            false,
            620,
            PollRate::Slow,
        ),
        (
            "moving, but far from the Dock",
            PollRate::Slow,
            Some(point(721.0, 300.0)),
            true,
            630,
            PollRate::Slow,
        ),
        ("cursor unknown", near, None, false, 1200, PollRate::Fast),
        (
            "cursor still unknown",
            near,
            None,
            false,
            1500,
            PollRate::Slow,
        ),
    ];
    let mut activity = Activity::default();

    for (case, spatial, cursor, sent, at, expected) in cases {
        let now = start.checked_add(ms(at)).expect("in range");
        assert_eq!(
            activity.rate(spatial, cursor, sent, now),
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
    let at = |millis: u64| start.checked_add(ms(millis)).expect("in range");
    let since_start = |instant: Instant| {
        u64::try_from(instant.duration_since(start).as_millis()).expect("in range")
    };
    let clock = std::cell::Cell::new(at(now_ms));
    let mut checks = Vec::new();
    let read = activity.wait_for_read(
        at(read_due_ms),
        at(now_ms),
        |until| clock.set(until),
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
    (since_start(read), checks)
}

#[test]
fn a_pointer_resting_in_the_band_is_checked_at_10_hz_until_it_moves_or_the_read_is_due() {
    let start = Instant::now();
    let rest = Some(point(720.0, 850.0));
    let moved = Some(point(722.0, 850.0));
    let far = Some(point(720.0, 300.0));
    let cases = [
        (
            "at rest: checks every 100 ms, then the Slow read",
            PollRate::Fast,
            rest,
            vec![rest],
            800,
            800,
            vec![400, 500, 600, 700],
        ),
        (
            "moves before the first check: read at that check",
            PollRate::Fast,
            rest,
            vec![moved],
            800,
            400,
            vec![400],
        ),
        (
            "moves before the third check: read at that check",
            PollRate::Fast,
            rest,
            vec![rest, rest, moved],
            800,
            600,
            vec![400, 500, 600],
        ),
        (
            "cursor unknown at rest: checks, then the Slow read",
            PollRate::Fast,
            None,
            vec![None],
            800,
            800,
            vec![400, 500, 600, 700],
        ),
        (
            "away from the Dock: no checks",
            PollRate::Slow,
            far,
            vec![moved],
            800,
            800,
            vec![],
        ),
    ];

    for (case, spatial, cursor, pointer, read_due, expected_read, expected_checks) in cases {
        let mut activity = Activity::default();
        activity.rate(spatial, cursor, false, start);
        let rate = activity.rate(
            spatial,
            cursor,
            false,
            start.checked_add(ms(300)).expect("in range"),
        );
        assert_eq!(rate, PollRate::Slow, "{case}");

        let (read, checks) = wait_for_read(&activity, start, 300, read_due, &pointer);

        assert_eq!(read, expected_read, "{case}: read");
        assert_eq!(checks, expected_checks, "{case}: checks");
    }
}

#[test]
fn a_moving_pointer_near_the_dock_gets_no_pointer_checks_between_fast_reads() {
    let start = Instant::now();
    let mut activity = Activity::default();
    let rate = activity.rate(PollRate::Fast, Some(point(720.0, 850.0)), false, start);
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
    activity.rate(PollRate::Fast, rest, false, start);
    assert_eq!(
        activity.rate(
            PollRate::Fast,
            rest,
            false,
            start.checked_add(ms(300)).expect("in range")
        ),
        PollRate::Slow
    );

    let (read, _) = wait_for_read(&activity, start, 300, 800, &[moved]);
    let rate = activity.rate(
        PollRate::Fast,
        moved,
        false,
        start.checked_add(ms(read)).expect("in range"),
    );

    assert!(read - 300 <= 100, "read {read} ms");
    assert_eq!(rate, PollRate::Fast);
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
