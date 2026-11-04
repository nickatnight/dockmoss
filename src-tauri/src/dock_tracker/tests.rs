use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tauri::ipc::Channel;

use super::{Drained, Inbox, drain_inbox_until};
use crate::commands::TrackerMessage;
use crate::dock::RawDockChild;
use crate::geometry::{Display, ScreenRect};
use crate::tracker::{Tracker, Visibility};

const PRIMARY_2X: Display = Display {
    physical_x: 0,
    physical_y: 0,
    physical_width: 3024,
    physical_height: 1964,
    scale_factor: 2.0,
};

fn safari() -> RawDockChild {
    RawDockChild {
        subrole: Some("AXApplicationDockItem".into()),
        rect: Some(ScreenRect {
            x: 700.0,
            y: 920.0,
            width: 56.0,
            height: 56.0,
        }),
        app_path: Some(PathBuf::from("/Applications/Safari.app")),
        bundle_id: Some("com.apple.Safari".into()),
    }
}

#[test]
fn a_burst_of_screen_changes_does_not_cut_the_wait_short() {
    let (inbox, messages) = mpsc::channel();
    for _ in 0..1000 {
        inbox
            .send(TrackerMessage::ScreenParametersChanged)
            .expect("the receiver is alive");
    }
    let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
    let mut watcher = None;
    let mut inbox_state = Inbox::default();
    let delay = Duration::from_millis(50);

    let started = Instant::now();
    let drained = drain_inbox_until(
        started + delay,
        &messages,
        &mut tracker,
        &mut watcher,
        &mut inbox_state,
    );

    assert_eq!(drained, Drained::AtDeadline);
    assert!(
        started.elapsed() >= delay,
        "returned after {:?}",
        started.elapsed()
    );
    assert!(inbox_state.screen_changed);
    assert!(messages.try_recv().is_err(), "every message was taken");
}

#[test]
fn a_new_watcher_gets_the_current_frame_on_the_next_tick() {
    let (inbox, messages) = mpsc::channel();
    let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
    let first = tracker.tick(Ok(vec![safari()]), None);
    let unchanged = tracker.tick(Ok(vec![safari()]), None);
    inbox
        .send(TrackerMessage::Watch(Channel::new(|_| Ok(()))))
        .expect("the receiver is alive");
    let mut watcher = None;
    let mut inbox_state = Inbox::default();

    drain_inbox_until(
        Instant::now() + Duration::from_millis(5),
        &messages,
        &mut tracker,
        &mut watcher,
        &mut inbox_state,
    );
    let after_watch = tracker.tick(Ok(vec![safari()]), None);

    assert!(first.send.is_some() && unchanged.send.is_none(), "baseline");
    assert!(watcher.is_some(), "the watcher is stored");
    assert!(!inbox_state.screen_changed);
    assert_eq!(after_watch.send.map(|frame| frame.len()), Some(1));
}

#[test]
fn a_dock_exit_ends_the_wait_at_once() {
    let (inbox, messages) = mpsc::channel();
    inbox
        .send(TrackerMessage::DockExited)
        .expect("the receiver is alive");
    let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
    let mut watcher = None;
    let mut inbox_state = Inbox::default();

    let started = Instant::now();
    let drained = drain_inbox_until(
        started + Duration::from_millis(500),
        &messages,
        &mut tracker,
        &mut watcher,
        &mut inbox_state,
    );

    assert_eq!(drained, Drained::Early);
    assert!(inbox_state.dock_exited);
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "waited {:?}",
        started.elapsed()
    );
}
