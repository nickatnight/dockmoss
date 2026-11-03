mod bundle_id;
mod commands;
mod dock;
mod error;
mod geometry;
mod growth;
mod macos;
mod tray;
mod trust;
mod usage;

pub use error::Error;

use std::time::Duration;

use tauri::tray::TrayIcon;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_log::log::LevelFilter;

use commands::OverlayDisplay;
use geometry::Display;
use trust::TrustState;

const OVERLAY_LABEL: &str = "overlay";
const TRUST_POLL_INTERVAL: Duration = Duration::from_secs(5);

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
                .build(),
        )
        .invoke_handler(tauri::generate_handler![commands::dock_items])
        .setup(|app| {
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let trust = macos::ax::request_trust();
            tracing::info!(trusted = trust.is_trusted(), "accessibility trust checked");
            let tray = tray::build(app.handle(), trust)?;
            app.manage(TrustState::new(trust));
            spawn_trust_watcher(tray)?;
            let display = primary_display(app.handle())?;
            app.manage(OverlayDisplay(display));
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
                if let Some(trust) = state.update(macos::ax::check_trust()) {
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

fn primary_display(app: &AppHandle) -> Result<Display, Error> {
    let monitor = app.primary_monitor()?.ok_or(Error::NoMainDisplay)?;
    Ok(Display {
        physical_x: monitor.position().x,
        physical_y: monitor.position().y,
        physical_width: monitor.size().width,
        physical_height: monitor.size().height,
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
