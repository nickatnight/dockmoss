mod bundle_id;
mod commands;
mod dock;
mod error;
mod geometry;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        unused_imports,
        reason = "growth and its re-exports are wired to the tracker in slice 8"
    )
)]
mod growth;
mod macos;
mod tracker;
mod tray;
mod trust;
mod usage;

pub use error::Error;

use std::sync::mpsc;
use std::time::Duration;

use jiff::Timestamp;
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_log::log::LevelFilter;

use commands::{TrackerInbox, TrackerMessage};
use geometry::Display;
use launch_log_writer::{LaunchLogInbox, WriterMessage};
use tracker::Visibility;
use trust::TrustState;

const OVERLAY_LABEL: &str = "overlay";
const LAUNCH_LOG_FILE: &str = "launch_log.json";
const FLUSH_TIMEOUT: Duration = Duration::from_millis(500);
const TRUST_POLL_INTERVAL: Duration = Duration::from_secs(5);
const APP_LOG_LEVEL: LevelFilter = if cfg!(debug_assertions) {
    LevelFilter::Debug
} else {
    LevelFilter::Info
};

/// Builds and runs the Tauri application.
///
/// # Panics
///
/// Panics if the Tauri runtime fails to build.
pub fn run() {
    #[expect(
        clippy::expect_used,
        reason = "startup invariant: the runtime builds with the context built into the binary"
    )]
    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(LevelFilter::Info)
                .level_for("dockmoss_lib", APP_LOG_LEVEL)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![commands::watch_dock])
        .setup(|app| {
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let trust = macos::ax::request_trust();
            tracing::info!(trusted = trust.is_trusted(), "accessibility trust checked");
            let tray = tray::build(app.handle(), trust)?;
            app.manage(TrustState::new(trust));
            spawn_trust_watcher(tray)?;
            let display = primary_display(app.handle())?;
            let (inbox, messages) = mpsc::channel();
            let screen_changes = inbox.clone();
            app.manage(TrackerInbox(inbox));
            let overlay = build_overlay(app.handle(), display)?;
            #[expect(
                clippy::expect_used,
                reason = "startup invariant: tauri runs setup() on the main thread"
            )]
            let mtm =
                macos::MainThreadMarker::new().expect("tauri runs setup() on the main thread");
            start_launch_log(app.handle(), mtm)?;
            let levels = macos::overlay::pin_above_dock(&overlay, mtm).map_err(Error::from)?;
            tracing::info!(
                dock_level = levels.dock,
                overlay_level = levels.overlay,
                "overlay pinned above the dock"
            );
            overlay.show()?;
            let shown = if overlay.is_visible()? {
                Visibility::Shown
            } else {
                Visibility::Hidden
            };
            dock_tracker::spawn(app.handle().clone(), messages, display, shown)?;
            macos::screen::on_screen_parameters_change(
                move || {
                    if screen_changes
                        .send(TrackerMessage::ScreenParametersChanged)
                        .is_err()
                    {
                        tracing::warn!("screen change not passed on: the dock tracker stopped");
                    }
                },
                mtm,
            );
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "dockmoss started");
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("tauri runtime builds with the bundled context");
    app.run(|app, event| {
        if let RunEvent::Exit = event {
            flush_launch_log(app);
        }
    });
}

fn start_launch_log(app: &AppHandle, mtm: macos::MainThreadMarker) -> tauri::Result<()> {
    let path = app.path().app_data_dir()?.join(LAUNCH_LOG_FILE);
    let log = usage::load(&path, Timestamp::now());
    let entries = log.len();
    let inbox = launch_log_writer::spawn(path.clone(), log)?;
    let now = Timestamp::now();
    let mut running = 0_usize;
    for bundle_id in macos::workspace::running_apps(mtm)
        .into_iter()
        .filter_map(usage::tracked_bundle_id)
    {
        tracing::debug!(bundle_id = bundle_id.as_str(), "running at startup");
        if inbox
            .send(WriterMessage::Used { bundle_id, at: now })
            .is_err()
        {
            tracing::warn!("running apps not recorded: the launch log writer stopped");
            break;
        }
        running += 1;
    }
    let events = inbox.clone();
    macos::workspace::on_app_events(move |event| record_app_event(&events, event), mtm);
    app.manage(LaunchLogInbox(inbox));
    tracing::info!(
        path = %path.display(),
        entries,
        running_apps = running,
        "launch log started"
    );
    Ok(())
}

fn record_app_event(inbox: &mpsc::Sender<WriterMessage>, event: usage::AppEvent) {
    let kind = event.kind;
    let Some(bundle_id) = usage::tracked_bundle_id(event.app) else {
        return;
    };
    tracing::debug!(bundle_id = bundle_id.as_str(), ?kind, "app used");
    let used = WriterMessage::Used {
        bundle_id,
        at: Timestamp::now(),
    };
    if inbox.send(used).is_err() {
        tracing::warn!(?kind, "app use not recorded: the launch log writer stopped");
    }
}

fn flush_launch_log(app: &AppHandle) {
    let Some(inbox) = app.try_state::<LaunchLogInbox>() else {
        return;
    };
    let (ack, acked) = mpsc::channel();
    if inbox.0.send(WriterMessage::Flush(ack)).is_err() {
        tracing::warn!("launch log not flushed on quit: the writer stopped");
        return;
    }
    match acked.recv_timeout(FLUSH_TIMEOUT) {
        Ok(()) => tracing::debug!("launch log flushed on quit"),
        Err(error) => tracing::warn!(%error, "launch log flush on quit not confirmed"),
    }
}

mod launch_log_writer {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
    use std::time::Instant;

    use jiff::Timestamp;

    use crate::bundle_id::BundleId;
    use crate::usage::{self, LaunchLog, SaveSchedule};

    pub enum WriterMessage {
        Used { bundle_id: BundleId, at: Timestamp },
        Flush(Sender<()>),
    }

    pub struct LaunchLogInbox(pub Sender<WriterMessage>);

    pub fn spawn(path: PathBuf, log: LaunchLog) -> std::io::Result<Sender<WriterMessage>> {
        let (inbox, messages) = mpsc::channel();
        std::thread::Builder::new()
            .name("launch-log".into())
            .spawn(move || write_launch_log(&path, log, &messages))?;
        Ok(inbox)
    }

    fn write_launch_log(path: &Path, mut log: LaunchLog, messages: &Receiver<WriterMessage>) {
        let mut schedule = SaveSchedule::default();
        loop {
            let message = match schedule.deadline() {
                None => messages.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(deadline) => {
                    messages.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                }
            };
            match message {
                Ok(WriterMessage::Used { bundle_id, at }) => {
                    if log.record(bundle_id, at) {
                        schedule.changed(Instant::now());
                    }
                }
                Ok(WriterMessage::Flush(ack)) => {
                    if schedule.deadline().is_some() {
                        save(path, &log, &mut schedule, "flush");
                    }
                    if ack.send(()).is_err() {
                        tracing::debug!("launch log flushed after the quit stopped waiting");
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    if schedule.deadline().is_some() {
                        save(path, &log, &mut schedule, "disconnected");
                    }
                    tracing::debug!("launch log writer stopped: no senders left");
                    return;
                }
            }
            if schedule.is_due(Instant::now()) {
                save(path, &log, &mut schedule, "deadline");
            }
        }
    }

    fn save(path: &Path, log: &LaunchLog, schedule: &mut SaveSchedule, trigger: &'static str) {
        match usage::save(path, log) {
            Ok(()) => {
                schedule.saved();
                tracing::debug!(entries = log.len(), trigger, "launch log saved");
            }
            Err(error) => {
                schedule.save_failed(Instant::now());
                tracing::warn!(%error, trigger, "launch log not saved; retrying in 5 s");
            }
        }
    }
}

fn spawn_trust_watcher(tray: TrayIcon) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("ax-trust".into())
        .spawn(move || {
            loop {
                std::thread::sleep(TRUST_POLL_INTERVAL);
                let state = tray.app_handle().state::<TrustState>();
                if let Some(trust) = state.replace_if_changed(macos::ax::check_trust()) {
                    tracing::info!(trusted = trust.is_trusted(), "accessibility trust changed");
                    refresh_tray_on_main_thread(&tray);
                }
            }
        })?;
    Ok(())
}

fn refresh_tray_on_main_thread(tray: &TrayIcon) {
    let main_tray = tray.clone();
    let scheduled = tray.app_handle().run_on_main_thread(move || {
        let trust = main_tray.app_handle().state::<TrustState>().current();
        if let Err(error) = tray::show_trust(&main_tray, trust) {
            tracing::warn!(%error, "tray update after a trust change failed");
        }
    });
    if let Err(error) = scheduled {
        tracing::warn!(%error, "could not schedule the tray update");
    }
}

mod dock_tracker {
    use std::sync::mpsc::{Receiver, RecvTimeoutError};
    use std::time::{Duration, Instant};

    use tauri::ipc::Channel;
    use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager};

    use crate::commands::{TrackerInbox, TrackerMessage};
    use crate::dock::{self, DockItem};
    use crate::geometry::Display;
    use crate::tracker::{self, PollRate, Tick, TickLog, Tracker, Visibility};
    use crate::trust::TrustState;
    use crate::{OVERLAY_LABEL, macos, primary_display};

    pub(super) fn spawn(
        app: AppHandle,
        messages: Receiver<TrackerMessage>,
        display: Display,
        overlay: Visibility,
    ) -> std::io::Result<()> {
        std::thread::Builder::new()
            .name("dock-tracker".into())
            .spawn(move || track_dock(&app, &messages, display, overlay))?;
        Ok(())
    }

    fn track_dock(
        app: &AppHandle,
        messages: &Receiver<TrackerMessage>,
        display: Display,
        overlay: Visibility,
    ) {
        let Some(trust) = app.try_state::<TrustState>() else {
            tracing::warn!("dock tracker started before the trust state; not tracking");
            return;
        };
        let mut reader = macos::dock::DockReader::new();
        let mut gate = dock::ReadGate::default();
        let mut tracker = Tracker::new(display, overlay);
        let mut watcher = None;
        let mut rate = PollRate::Slow;
        let mut screen_changed = false;
        loop {
            let started = Instant::now();
            if std::mem::take(&mut screen_changed) {
                follow_display(app, &mut tracker);
            }
            let cursor = macos::cursor::location();
            let read = gate.read(trust.current(), macos::ax::check_trust, || reader.read());
            tracing::debug!(elapsed_us = micros(started.elapsed()), ?rate, "dock read");
            let tick = tracker.tick(read, cursor);
            if tick.rate != rate {
                tracing::debug!(rate = ?tick.rate, "dock poll rate changed");
                rate = tick.rate;
            }
            perform(app, tick, &mut watcher);
            let delay = tracker::next_delay(rate, started.elapsed());
            drain_inbox_for(
                delay,
                messages,
                &mut tracker,
                &mut watcher,
                &mut screen_changed,
            );
        }
    }

    /// Applies each message that arrives within `delay`, and returns only once
    /// `delay` has passed. A burst of screen notifications (one per frame while
    /// the Dock slides in) therefore adds no Dock reads; it sets `screen_changed`
    /// for the next tick.
    fn drain_inbox_for(
        delay: Duration,
        messages: &Receiver<TrackerMessage>,
        tracker: &mut Tracker,
        watcher: &mut Option<Channel<Vec<DockItem>>>,
        screen_changed: &mut bool,
    ) {
        let deadline = Instant::now().checked_add(delay);
        loop {
            let left = deadline.map_or(Duration::ZERO, |deadline| {
                deadline.saturating_duration_since(Instant::now())
            });
            match messages.recv_timeout(left) {
                Ok(TrackerMessage::Watch(channel)) => {
                    *watcher = Some(channel);
                    tracker.resend_on_next_tick();
                }
                Ok(TrackerMessage::ScreenParametersChanged) => *screen_changed = true,
                Ok(TrackerMessage::OverlayOrderFailed) => tracker.order_failed(),
                Err(RecvTimeoutError::Timeout) => return,
                Err(RecvTimeoutError::Disconnected) => {
                    std::thread::sleep(left);
                    return;
                }
            }
        }
    }

    fn follow_display(app: &AppHandle, tracker: &mut Tracker) {
        let display = match primary_display(app) {
            Ok(display) => display,
            Err(error) => {
                tracing::warn!(%error, "display change not followed");
                return;
            }
        };
        if !tracker.set_display(display) {
            tracing::debug!("screen parameters changed; primary display unchanged");
            return;
        }
        let frame = display.frame();
        let scale = display.scale_factor;
        let Some(window) = app.get_webview_window(OVERLAY_LABEL) else {
            tracing::warn!("display changed but the overlay window is gone");
            return;
        };
        let moved = window
            .set_position(LogicalPosition::new(frame.x, frame.y))
            .and_then(|()| window.set_size(LogicalSize::new(frame.width, frame.height)));
        match moved {
            Ok(()) => tracing::info!(
                x = frame.x,
                y = frame.y,
                width = frame.width,
                height = frame.height,
                scale,
                "overlay moved to the new display frame"
            ),
            Err(error) => tracing::warn!(%error, "overlay not moved to the new display frame"),
        }
    }

    fn perform(app: &AppHandle, tick: Tick, watcher: &mut Option<Channel<Vec<DockItem>>>) {
        match tick.log {
            Some(TickLog::ReadFailed(error)) => tracing::warn!(%error, "dock read failed"),
            Some(TickLog::Recovered) => tracing::info!("dock read recovered"),
            None => {}
        }
        if let Some(visibility) = tick.overlay {
            order_overlay(app, visibility);
        }
        let Some(frame) = tick.send else {
            return;
        };
        let count = frame.len();
        let Some(channel) = watcher.as_ref() else {
            tracing::debug!(count, "dock items changed with no watcher");
            return;
        };
        match channel.send(frame) {
            Ok(()) => tracing::debug!(count, "dock items sent"),
            Err(error) => {
                tracing::debug!(%error, "dock watcher dropped after a failed send");
                *watcher = None;
            }
        }
    }

    fn order_overlay(app: &AppHandle, visibility: Visibility) {
        let main = app.clone();
        let scheduled = app.run_on_main_thread(move || {
            let Some(mtm) = macos::MainThreadMarker::new() else {
                tracing::warn!("overlay ordering ran off the main thread");
                return;
            };
            let Some(window) = main.get_webview_window(OVERLAY_LABEL) else {
                tracing::warn!("overlay window is gone");
                return;
            };
            match macos::overlay::set_visible(&window, visibility == Visibility::Shown, mtm) {
                Ok(state) => tracing::debug!(
                    ?visibility,
                    visible = state.visible,
                    level = state.level,
                    ignores_mouse_events = state.ignores_mouse_events,
                    collection_behavior = state.collection_behavior,
                    "overlay ordered"
                ),
                Err(error) => {
                    tracing::warn!(%error, ?visibility, "overlay not ordered");
                    report_order_failed(&main);
                }
            }
        });
        if let Err(error) = scheduled {
            tracing::warn!(%error, "could not schedule the overlay ordering");
            report_order_failed(app);
        }
    }

    fn report_order_failed(app: &AppHandle) {
        let Some(inbox) = app.try_state::<TrackerInbox>() else {
            tracing::warn!("overlay ordering not retried: no tracker inbox");
            return;
        };
        if inbox.0.send(TrackerMessage::OverlayOrderFailed).is_err() {
            tracing::warn!("overlay ordering not retried: the dock tracker stopped");
        }
    }

    fn micros(duration: Duration) -> u64 {
        u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
    }

    #[cfg(test)]
    mod tests;
}

fn primary_display(app: &AppHandle) -> Result<Display, Error> {
    let monitor = app.primary_monitor()?.ok_or(Error::NoMainDisplay)?;
    let origin = monitor.position();
    let size = monitor.size();
    Ok(Display {
        physical_x: origin.x,
        physical_y: origin.y,
        physical_width: size.width,
        physical_height: size.height,
        scale_factor: monitor.scale_factor(),
    })
}

fn build_overlay(app: &AppHandle, display: Display) -> Result<WebviewWindow, Error> {
    let frame = display.frame();
    let scale = display.scale_factor;
    let window =
        WebviewWindowBuilder::new(app, OVERLAY_LABEL, WebviewUrl::App("overlay.html".into()))
            .title("DockMoss Overlay")
            .position(frame.x, frame.y)
            .inner_size(frame.width, frame.height)
            .transparent(true)
            .decorations(false)
            .shadow(false)
            .resizable(false)
            .focused(false)
            .focusable(false)
            .skip_taskbar(true)
            .visible_on_all_workspaces(true)
            .visible(false)
            .build()?;
    window.set_ignore_cursor_events(true)?;

    tracing::info!(
        x = frame.x,
        y = frame.y,
        width = frame.width,
        height = frame.height,
        scale,
        "overlay window created"
    );
    Ok(window)
}
