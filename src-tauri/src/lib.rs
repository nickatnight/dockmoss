mod commands;
mod error;
mod growth;
mod macos;
mod tray;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the Accessibility watcher calls it from the next commit"
    )
)]
mod trust;
mod usage;

pub use error::Error;

use tauri::{AppHandle, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_log::log::LevelFilter;

const OVERLAY_LABEL: &str = "overlay";

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
            tray::build(app.handle())?;
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
