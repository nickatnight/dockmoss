mod bundle_id;
mod commands;
mod dock;
mod error;
mod geometry;
mod growth;
mod macos;
mod tracker;
mod tray;
mod trust;
mod usage;

pub use error::Error;

use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use tauri::ipc::Channel;
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_log::log::LevelFilter;

use commands::{TrackerInbox, TrackerMessage};
use dock::DockItem;
use geometry::Display;
use tracker::{PollRate, Tick, TickLog, Tracker};
use trust::TrustState;

const OVERLAY_LABEL: &str = "overlay";
const TRUST_POLL_INTERVAL: Duration = Duration::from_secs(5);
/// Debug builds log every Dock read and send; release builds stay at info.
const APP_LOG_LEVEL: LevelFilter = if cfg!(debug_assertions) {
    LevelFilter::Debug
} else {
    LevelFilter::Info
};

/// Builds and runs the Tauri application.
///
/// # Panics
///
/// Panics if the Tauri runtime fails to start.
pub fn run() {
    tauri::Builder::default()
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
            app.manage(TrackerInbox(inbox));
            let overlay = build_overlay(app.handle(), display)?;
            let mtm =
                macos::MainThreadMarker::new().expect("tauri runs setup() on the main thread");
            let levels = macos::overlay::pin_above_dock(&overlay, mtm).map_err(Error::from)?;
            tracing::info!(
                dock_level = levels.dock,
                overlay_level = levels.overlay,
                "overlay pinned above the dock"
            );
            overlay.show()?;
            spawn_dock_tracker(app.handle().clone(), messages, display)?;
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "dockmoss started");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("tauri runtime starts with the bundled context");
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

fn spawn_dock_tracker(
    app: AppHandle,
    messages: Receiver<TrackerMessage>,
    display: Display,
) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("dock-tracker".into())
        .spawn(move || track_dock(&app, &messages, display))?;
    Ok(())
}

/// The thread body: read, let `Tracker` decide, do what it says, wait. It
/// must not panic (release builds abort), so it holds no `expect`, `unwrap`
/// or indexing.
fn track_dock(app: &AppHandle, messages: &Receiver<TrackerMessage>, display: Display) {
    let Some(trust) = app.try_state::<TrustState>() else {
        tracing::warn!("dock tracker started before the trust state; not tracking");
        return;
    };
    let mut reader = macos::dock::DockReader::new();
    let mut tracker = Tracker::new(display);
    let mut watcher = None;
    let mut rate = PollRate::Slow;
    loop {
        let started = Instant::now();
        let cursor = macos::cursor::location();
        let read = dock::read(trust.current(), || reader.read());
        tracing::debug!(elapsed_us = micros(started.elapsed()), ?rate, "dock read");
        let tick = tracker.tick(read, cursor);
        if tick.rate != rate {
            tracing::debug!(rate = ?tick.rate, "dock poll rate changed");
            rate = tick.rate;
        }
        perform(tick, &mut watcher);
        let delay = tracker::next_delay(rate, started.elapsed());
        match messages.recv_timeout(delay) {
            Ok(message) => receive(message, &mut tracker, &mut watcher),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => std::thread::sleep(delay),
        }
        while let Ok(message) = messages.try_recv() {
            receive(message, &mut tracker, &mut watcher);
        }
    }
}

fn receive(
    message: TrackerMessage,
    tracker: &mut Tracker,
    watcher: &mut Option<Channel<Vec<DockItem>>>,
) {
    match message {
        TrackerMessage::Watch(channel) => {
            *watcher = Some(channel);
            tracker.resubscribe();
        }
    }
}

fn perform(tick: Tick, watcher: &mut Option<Channel<Vec<DockItem>>>) {
    match tick.log {
        Some(TickLog::ReadFailed(error)) => tracing::warn!(%error, "dock read failed"),
        Some(TickLog::Recovered) => tracing::info!("dock read recovered"),
        None => {}
    }
    if let Some(visibility) = tick.overlay {
        tracing::debug!(?visibility, "overlay visibility changed");
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

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
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
