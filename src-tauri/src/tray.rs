use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Wry};

use crate::macos;
use crate::trust::Trust;
use crate::usage::LeftBehind;

pub const TRAY_ID: &str = "main";

const QUIT_ID: &str = "quit";
const GRANT_ID: &str = "grant-accessibility";
const SETTINGS_ID: &str = "settings";
const LEFT_BEHIND_USAGE_ID: &str = "left-behind-usage";
const LEFT_BEHIND_SETTINGS_ID: &str = "left-behind-settings";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuEntry {
    LeftBehind(LeftBehind),
    GrantAccessibility,
    Settings,
    Separator,
    Quit,
}

pub fn build(
    app: &AppHandle,
    trust: Trust,
    left_behind: &[LeftBehind],
) -> Result<(), crate::Error> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .tooltip(tooltip(trust))
        .menu(&menu(app, trust, left_behind)?)
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

pub fn show_trust(
    tray: &TrayIcon,
    trust: Trust,
    left_behind: &[LeftBehind],
) -> Result<(), crate::Error> {
    tray.set_menu(Some(menu(tray.app_handle(), trust, left_behind)?))?;
    tray.set_tooltip(Some(tooltip(trust)))?;
    Ok(())
}

fn tooltip(trust: Trust) -> &'static str {
    match trust {
        Trust::Trusted => "DockMoss",
        Trust::Untrusted => "DockMoss needs Accessibility access",
    }
}

fn left_behind_text(left_behind: LeftBehind) -> &'static str {
    match left_behind {
        LeftBehind::UsageHistory => "Usage history wasn\u{2019}t moved, so no moss is shown",
        LeftBehind::Settings => "Saved settings weren\u{2019}t moved, so changes won\u{2019}t save",
    }
}

fn left_behind_id(left_behind: LeftBehind) -> &'static str {
    match left_behind {
        LeftBehind::UsageHistory => LEFT_BEHIND_USAGE_ID,
        LeftBehind::Settings => LEFT_BEHIND_SETTINGS_ID,
    }
}

fn menu_entries(trust: Trust, left_behind: &[LeftBehind]) -> Vec<MenuEntry> {
    let mut entries: Vec<MenuEntry> = left_behind
        .iter()
        .copied()
        .map(MenuEntry::LeftBehind)
        .collect();
    if !entries.is_empty() {
        entries.push(MenuEntry::Separator);
    }
    if trust == Trust::Untrusted {
        entries.push(MenuEntry::GrantAccessibility);
    }
    entries.extend([MenuEntry::Settings, MenuEntry::Separator, MenuEntry::Quit]);
    entries
}

fn menu(app: &AppHandle, trust: Trust, left_behind: &[LeftBehind]) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    for entry in menu_entries(trust, left_behind) {
        match entry {
            MenuEntry::LeftBehind(left) => menu.append(&MenuItem::with_id(
                app,
                left_behind_id(left),
                left_behind_text(left),
                false,
                None::<&str>,
            )?)?,
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
    use super::{MenuEntry, left_behind_text, menu_entries, tooltip};
    use crate::trust::Trust;
    use crate::usage::LeftBehind;

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
            assert_eq!(menu_entries(trust, &[]), entries, "{trust:?}");
        }
    }

    #[test]
    fn data_left_in_the_old_folder_heads_the_menu_with_and_without_accessibility_access() {
        let left = LeftBehind::UsageHistory;
        let cases = [
            (
                Trust::Trusted,
                vec![
                    MenuEntry::LeftBehind(left),
                    MenuEntry::Separator,
                    MenuEntry::Settings,
                    MenuEntry::Separator,
                    MenuEntry::Quit,
                ],
            ),
            (
                Trust::Untrusted,
                vec![
                    MenuEntry::LeftBehind(left),
                    MenuEntry::Separator,
                    MenuEntry::GrantAccessibility,
                    MenuEntry::Settings,
                    MenuEntry::Separator,
                    MenuEntry::Quit,
                ],
            ),
        ];

        for (trust, entries) in cases {
            assert_eq!(menu_entries(trust, &[left]), entries, "{trust:?}");
        }
    }

    #[test]
    fn with_both_left_behind_each_gets_its_own_line_before_one_separator() {
        let both = [LeftBehind::UsageHistory, LeftBehind::Settings];

        assert_eq!(
            menu_entries(Trust::Trusted, &both),
            vec![
                MenuEntry::LeftBehind(LeftBehind::UsageHistory),
                MenuEntry::LeftBehind(LeftBehind::Settings),
                MenuEntry::Separator,
                MenuEntry::Settings,
                MenuEntry::Separator,
                MenuEntry::Quit,
            ]
        );
    }

    #[test]
    fn the_notice_names_what_is_left_and_no_path() {
        let cases = [
            (
                LeftBehind::UsageHistory,
                "Usage history wasn\u{2019}t moved, so no moss is shown",
            ),
            (
                LeftBehind::Settings,
                "Saved settings weren\u{2019}t moved, so changes won\u{2019}t save",
            ),
        ];
        for (left, expected) in cases {
            assert_eq!(left_behind_text(left), expected, "{left:?}");
        }
    }

    #[test]
    fn each_left_behind_line_has_its_own_id() {
        use super::left_behind_id;
        assert_ne!(
            left_behind_id(LeftBehind::UsageHistory),
            left_behind_id(LeftBehind::Settings)
        );
    }
}
