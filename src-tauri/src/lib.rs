mod commands;
mod error;
mod growth;
mod macos;
mod usage;

pub use error::Error;

/// Builds and runs the Tauri application.
///
/// # Panics
///
/// Panics if the Tauri runtime fails to start.
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("tauri runtime starts with the bundled context");
}
