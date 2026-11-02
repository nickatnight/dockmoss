use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Wry};

use crate::macos;
use crate::trust::TrayChange;

const QUIT_ID: &str = "quit";
const GRANT_ID: &str = "grant-accessibility";
const ACCESSIBILITY_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";
const TRUSTED_TOOLTIP: &str = "DockMoss";
const UNTRUSTED_TOOLTIP: &str = "DockMoss needs Accessibility access";

/// Builds the tray in its trusted layout: Quit alone.
pub fn build(app: &AppHandle) -> Result<TrayIcon, crate::Error> {
    let tray = TrayIconBuilder::with_id("main")
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .tooltip(TRUSTED_TOOLTIP)
        .menu(&menu(app, false)?)
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

/// Shows or removes the grant item and sets the matching tooltip.
pub fn apply(tray: &TrayIcon, change: TrayChange) -> Result<(), crate::Error> {
    let (grant_item, tooltip) = match change {
        TrayChange::ShowGrantItem => (true, UNTRUSTED_TOOLTIP),
        TrayChange::HideGrantItem => (false, TRUSTED_TOOLTIP),
    };
    tray.set_menu(Some(menu(tray.app_handle(), grant_item)?))?;
    tray.set_tooltip(Some(tooltip))?;
    Ok(())
}

fn menu(app: &AppHandle, grant_item: bool) -> tauri::Result<Menu<Wry>> {
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit DockMoss", true, Some("CmdOrCtrl+Q"))?;
    if !grant_item {
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
    if macos::open_url(ACCESSIBILITY_SETTINGS_URL, mtm) {
        tracing::info!("accessibility settings opened from tray");
    } else {
        tracing::warn!(
            url = ACCESSIBILITY_SETTINGS_URL,
            "macOS did not open the accessibility settings"
        );
    }
}
