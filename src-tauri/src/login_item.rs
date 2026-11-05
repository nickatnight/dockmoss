use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginItemStatus {
    NotRegistered,
    Enabled,
    RequiresApproval,
    NotFound,
}

impl LoginItemStatus {
    pub fn opens_at_login(self) -> bool {
        self == Self::Enabled
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("launch at login was not changed")]
pub struct NotChanged;

/// Whether the app now opens at login, judged by the status read after the
/// change, not by whether the call reported an error: unregistering an item
/// that was never registered fails, yet ends where it was asked to.
pub fn outcome(wanted: bool, after: LoginItemStatus) -> Result<bool, NotChanged> {
    if after.opens_at_login() == wanted {
        Ok(wanted)
    } else {
        Err(NotChanged)
    }
}

/// `tauri dev` runs a bare binary, which must never become a login item:
/// only a path ending in `.app` is an app bundle.
pub fn is_app_bundle(bundle_path: &Path) -> bool {
    bundle_path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{LoginItemStatus, NotChanged, is_app_bundle, outcome};

    #[test]
    fn only_an_enabled_item_opens_at_login() {
        let cases = [
            (LoginItemStatus::NotRegistered, false),
            (LoginItemStatus::Enabled, true),
            (LoginItemStatus::RequiresApproval, false),
            (LoginItemStatus::NotFound, false),
        ];
        for (status, expected) in cases {
            assert_eq!(status.opens_at_login(), expected, "{status:?}");
        }
    }

    #[test]
    fn a_change_succeeds_only_when_macos_ends_in_the_state_asked_for() {
        let cases = [
            ("on, now enabled", true, LoginItemStatus::Enabled, Ok(true)),
            (
                "on, still off",
                true,
                LoginItemStatus::NotRegistered,
                Err(NotChanged),
            ),
            (
                "on, waiting for approval",
                true,
                LoginItemStatus::RequiresApproval,
                Err(NotChanged),
            ),
            (
                "on, not an app bundle",
                true,
                LoginItemStatus::NotFound,
                Err(NotChanged),
            ),
            (
                "off, now off",
                false,
                LoginItemStatus::NotRegistered,
                Ok(false),
            ),
            (
                "off, never registered",
                false,
                LoginItemStatus::NotFound,
                Ok(false),
            ),
            (
                "off, still enabled",
                false,
                LoginItemStatus::Enabled,
                Err(NotChanged),
            ),
            (
                "off, waiting for approval",
                false,
                LoginItemStatus::RequiresApproval,
                Ok(false),
            ),
        ];
        for (case, wanted, after, expected) in cases {
            assert_eq!(outcome(wanted, after), expected, "{case}");
        }
    }

    #[test]
    fn only_a_path_ending_in_app_is_an_app_bundle() {
        let cases = [
            ("/Applications/DockMoss.app", true),
            ("/Applications/DockMoss.APP", true),
            ("/Users/someone/repo/src-tauri/target/debug", false),
            ("/Applications/DockMoss.app/Contents/MacOS", false),
            ("/Users/someone/notes.app.txt", false),
            ("", false),
        ];
        for (path, expected) in cases {
            assert_eq!(is_app_bundle(Path::new(path)), expected, "{path}");
        }
    }
}
