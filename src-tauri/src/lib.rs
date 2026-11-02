mod commands;
mod error;
mod growth;
mod macos;
mod tray;
mod trust;
mod usage;

pub use error::Error;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::tray::TrayIcon;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_log::log::LevelFilter;

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
        .setup(|app| {
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            // The only check that may prompt: macOS shows its dialog when
            // untrusted, and every later check passes `false`.
            let trusted = macos::ax::is_trusted(true);
            tracing::info!(trusted, "accessibility trust checked");
            let tray = tray::build(app.handle())?;
            if let Some(change) = trust::tray_change(None, trusted) {
                tray::apply(&tray, change)?;
            }
            app.manage(TrustState(AtomicBool::new(trusted)));
            spawn_trust_watcher(tray)?;
            let overlay = build_overlay(app.handle())?;
            let mtm =
                macos::MainThreadMarker::new().expect("tauri runs setup() on the main thread");
            let levels = macos::pin_above_dock(&overlay, mtm).map_err(Error::from)?;
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

/// Re-checks trust every few seconds for the life of the app, in both
/// directions, and updates the tray on the main thread when it changes.
fn spawn_trust_watcher(tray: TrayIcon) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("ax-trust".into())
        .spawn(move || {
            loop {
                std::thread::sleep(TRUST_POLL_INTERVAL);
                let state = tray.app_handle().state::<TrustState>();
                let previous = state.0.load(Ordering::Relaxed);
                let trusted = macos::ax::is_trusted(false);
                let Some(change) = trust::tray_change(Some(previous), trusted) else {
                    continue;
                };
                state.0.store(trusted, Ordering::Relaxed);
                tracing::info!(trusted, "accessibility trust changed");
                let main_tray = tray.clone();
                let scheduled = tray.app_handle().run_on_main_thread(move || {
                    if let Err(error) = tray::apply(&main_tray, change) {
                        tracing::warn!(%error, "tray update after a trust change failed");
                    }
                });
                if let Err(error) = scheduled {
                    tracing::warn!(%error, "could not schedule the tray update");
                }
            }
        })?;
    Ok(())
}

fn build_overlay(app: &AppHandle) -> Result<WebviewWindow, Error> {
    let display = app.primary_monitor()?.ok_or(Error::NoMainDisplay)?;
    let scale = display.scale_factor();
    let origin = display.position().to_logical::<f64>(scale);
    let size = display.size().to_logical::<f64>(scale);

    let window =
        WebviewWindowBuilder::new(app, OVERLAY_LABEL, WebviewUrl::App("overlay.html".into()))
            .title("DockMoss Overlay")
            .position(origin.x, origin.y)
            .inner_size(size.width, size.height)
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
        x = origin.x,
        y = origin.y,
        width = size.width,
        height = size.height,
        scale,
        "overlay window created"
    );
    Ok(window)
}
