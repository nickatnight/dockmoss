use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Wry};

use crate::macos;
use crate::trust::Trust;

const QUIT_ID: &str = "quit";
const GRANT_ID: &str = "grant-accessibility";

pub fn build(app: &AppHandle, trust: Trust) -> Result<TrayIcon, crate::Error> {
    let tray = TrayIconBuilder::with_id("main")
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .tooltip(tooltip(trust))
        .menu(&menu(app, trust)?)
        .on_menu_event(|app, event| match event.id().as_ref() {
            QUIT_ID => {
                tracing::info!("quit requested from tray");
                app.exit(0);
            }
            GRANT_ID => open_accessibility_settings(),
            _ => {}
        })
        .build(app)?;
    Ok(tray)
}

pub fn show_trust(tray: &TrayIcon, trust: Trust) -> Result<(), crate::Error> {
    tray.set_menu(Some(menu(tray.app_handle(), trust)?))?;
    tray.set_tooltip(Some(tooltip(trust)))?;
    Ok(())
}

fn tooltip(trust: Trust) -> &'static str {
    match trust {
        Trust::Trusted => "DockMoss",
        Trust::Untrusted => "DockMoss needs Accessibility access",
    }
}

fn menu(app: &AppHandle, trust: Trust) -> tauri::Result<Menu<Wry>> {
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit DockMoss", true, Some("CmdOrCtrl+Q"))?;
    if trust == Trust::Trusted {
        return Menu::with_items(app, &[&quit]);
    }
    let grant = MenuItem::with_id(
        app,
        GRANT_ID,
        "Grant Accessibility Access\u{2026}",
        true,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    Menu::with_items(app, &[&grant, &separator, &quit])
}

fn open_accessibility_settings() {
    let mtm =
        macos::MainThreadMarker::new().expect("tauri delivers tray menu events on the main thread");
    match macos::workspace::open_accessibility_settings(mtm) {
        Ok(()) => tracing::info!("accessibility settings opened from tray"),
        Err(error) => tracing::warn!(%error, "accessibility settings did not open"),
    }
}
