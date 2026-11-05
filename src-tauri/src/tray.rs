use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Wry};

use crate::macos;
use crate::trust::Trust;

pub const TRAY_ID: &str = "main";

const QUIT_ID: &str = "quit";
const GRANT_ID: &str = "grant-accessibility";
const SETTINGS_ID: &str = "settings";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuEntry {
    GrantAccessibility,
    Settings,
    Separator,
    Quit,
}

pub fn build(app: &AppHandle, trust: Trust) -> Result<(), crate::Error> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .tooltip(tooltip(trust))
        .menu(&menu(app, trust)?)
        .on_menu_event(|app, event| match event.id().as_ref() {
            QUIT_ID => {
                tracing::info!("quit requested from tray");
                app.exit(0);
            }
            GRANT_ID => show_accessibility_pane(),
            SETTINGS_ID => crate::show_settings_window(app),
            _ => {}
        })
        .build(app)?;
    Ok(())
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

fn menu_entries(trust: Trust) -> &'static [MenuEntry] {
    match trust {
        Trust::Trusted => &[MenuEntry::Settings, MenuEntry::Separator, MenuEntry::Quit],
        Trust::Untrusted => &[
            MenuEntry::GrantAccessibility,
            MenuEntry::Settings,
            MenuEntry::Separator,
            MenuEntry::Quit,
        ],
    }
}

fn menu(app: &AppHandle, trust: Trust) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    for entry in menu_entries(trust) {
        match entry {
            MenuEntry::GrantAccessibility => menu.append(&MenuItem::with_id(
                app,
                GRANT_ID,
                "Grant Accessibility Access\u{2026}",
                true,
                None::<&str>,
            )?)?,
            MenuEntry::Settings => menu.append(&MenuItem::with_id(
                app,
                SETTINGS_ID,
                "Settings\u{2026}",
                true,
                Some("CmdOrCtrl+,"),
            )?)?,
            MenuEntry::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
            MenuEntry::Quit => menu.append(&MenuItem::with_id(
                app,
                QUIT_ID,
                "Quit DockMoss",
                true,
                Some("CmdOrCtrl+Q"),
            )?)?,
        }
    }
    Ok(menu)
}

fn show_accessibility_pane() {
    let Some(mtm) = macos::MainThreadMarker::new() else {
        tracing::warn!("tray menu event arrived off the main thread; settings not opened");
        return;
    };
    match macos::workspace::open_accessibility_settings(mtm) {
        Ok(()) => tracing::info!("accessibility settings opened from tray"),
        Err(error) => tracing::warn!(%error, "accessibility settings did not open"),
    }
}

#[cfg(test)]
mod tests {
    use super::{MenuEntry, menu_entries, tooltip};
    use crate::trust::Trust;

    #[test]
    fn tooltip_says_when_accessibility_access_is_missing() {
        assert_eq!(tooltip(Trust::Trusted), "DockMoss");
        assert_eq!(
            tooltip(Trust::Untrusted),
            "DockMoss needs Accessibility access"
        );
    }

    #[test]
    fn menu_offers_settings_with_and_without_accessibility_access() {
        let cases = [
            (
                Trust::Trusted,
                vec![MenuEntry::Settings, MenuEntry::Separator, MenuEntry::Quit],
            ),
            (
                Trust::Untrusted,
                vec![
                    MenuEntry::GrantAccessibility,
                    MenuEntry::Settings,
                    MenuEntry::Separator,
                    MenuEntry::Quit,
                ],
            ),
        ];

        for (trust, entries) in cases {
            assert_eq!(menu_entries(trust), entries.as_slice(), "{trust:?}");
        }
    }
}
