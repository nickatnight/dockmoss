mod commands;
mod error;
mod growth;
mod macos;
mod tray;
mod usage;

pub use error::Error;

use tauri_plugin_log::log::LevelFilter;

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
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "dock moss started");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("tauri runtime starts with the bundled context");
}
