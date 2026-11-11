use std::path::Path;

use serde::Serialize;

use crate::bounded::AloneRead;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginItemStatus {
    NotRegistered,
    Enabled,
    RequiresApproval,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LaunchAtLogin {
    On,
    Off,
    NeedsApproval,
}

impl LoginItemStatus {
    pub fn launch_at_login(self) -> LaunchAtLogin {
        match self {
            Self::Enabled => LaunchAtLogin::On,
            Self::RequiresApproval => LaunchAtLogin::NeedsApproval,
            Self::NotRegistered | Self::NotFound => LaunchAtLogin::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("launch at login was not changed")]
pub struct NotChanged;

/// The status after a change, judged by the status read after it, not by
/// whether the call reported an error: unregistering an item that was never
/// registered fails, yet ends where it was asked to.
pub fn outcome(wanted: bool, after: LoginItemStatus) -> Result<LaunchAtLogin, NotChanged> {
    match (wanted, after.launch_at_login()) {
        (true, LaunchAtLogin::On) => Ok(LaunchAtLogin::On),
        (false, LaunchAtLogin::Off) => Ok(LaunchAtLogin::Off),
        // Either way it is done: turned on, macOS registered it and only the
        // user can approve it; turned off, it does not open at login.
        (_, LaunchAtLogin::NeedsApproval) => Ok(LaunchAtLogin::NeedsApproval),
        (true, LaunchAtLogin::Off) | (false, LaunchAtLogin::On) => Err(NotChanged),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeFailed {
    NotChanged {
        after: LoginItemStatus,
    },
    /// The change ran, but the status read after it did not answer.
    StatusUnknown,
}

pub fn after_change(
    wanted: bool,
    read: AloneRead<LoginItemStatus>,
) -> Result<LaunchAtLogin, ChangeFailed> {
    match read {
        AloneRead::Answered(after) => {
            outcome(wanted, after).map_err(|NotChanged| ChangeFailed::NotChanged { after })
        }
        AloneRead::NoAnswer | AloneRead::Busy => Err(ChangeFailed::StatusUnknown),
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

    use serde_json::json;

    use super::{
        ChangeFailed, LaunchAtLogin, LoginItemStatus, NotChanged, after_change, is_app_bundle,
        outcome,
    };
    use crate::bounded::AloneRead;

    #[test]
    fn each_status_shows_as_on_off_or_needs_approval() {
        let cases = [
            (LoginItemStatus::NotRegistered, LaunchAtLogin::Off),
            (LoginItemStatus::Enabled, LaunchAtLogin::On),
            (
                LoginItemStatus::RequiresApproval,
                LaunchAtLogin::NeedsApproval,
            ),
            (LoginItemStatus::NotFound, LaunchAtLogin::Off),
        ];
        for (status, expected) in cases {
            assert_eq!(status.launch_at_login(), expected, "{status:?}");
        }
    }

    #[test]
    fn the_webview_gets_the_status_as_a_camel_case_string() {
        let cases = [
            (LaunchAtLogin::On, json!("on")),
            (LaunchAtLogin::Off, json!("off")),
            (LaunchAtLogin::NeedsApproval, json!("needsApproval")),
        ];
        for (status, expected) in cases {
            assert_eq!(
                serde_json::to_value(status).expect("LaunchAtLogin serializes"),
                expected,
                "{status:?}"
            );
        }
    }

    #[test]
    fn a_change_succeeds_when_macos_ends_as_asked_or_waits_for_approval() {
        let cases = [
            (
                "on, now enabled",
                true,
                LoginItemStatus::Enabled,
                Ok(LaunchAtLogin::On),
            ),
            (
                "on, still off",
                true,
                LoginItemStatus::NotRegistered,
                Err(NotChanged),
            ),
            (
                "on, waiting for the user's approval",
                true,
                LoginItemStatus::RequiresApproval,
                Ok(LaunchAtLogin::NeedsApproval),
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
                Ok(LaunchAtLogin::Off),
            ),
            (
                "off, never registered",
                false,
                LoginItemStatus::NotFound,
                Ok(LaunchAtLogin::Off),
            ),
            (
                "off, still enabled",
                false,
                LoginItemStatus::Enabled,
                Err(NotChanged),
            ),
            (
                "off, still waiting for approval",
                false,
                LoginItemStatus::RequiresApproval,
                Ok(LaunchAtLogin::NeedsApproval),
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

    #[test]
    fn a_change_is_judged_by_the_read_after_it_and_a_missed_read_is_unknown() {
        let cases = [
            (
                "on, read as enabled",
                true,
                AloneRead::Answered(LoginItemStatus::Enabled),
                Ok(LaunchAtLogin::On),
            ),
            (
                "on, read as still off",
                true,
                AloneRead::Answered(LoginItemStatus::NotRegistered),
                Err(ChangeFailed::NotChanged {
                    after: LoginItemStatus::NotRegistered,
                }),
            ),
            (
                "off, read as off",
                false,
                AloneRead::Answered(LoginItemStatus::NotRegistered),
                Ok(LaunchAtLogin::Off),
            ),
            (
                "the read did not answer in time",
                true,
                AloneRead::NoAnswer,
                Err(ChangeFailed::StatusUnknown),
            ),
            (
                "an earlier read is still stalled",
                false,
                AloneRead::Busy,
                Err(ChangeFailed::StatusUnknown),
            ),
        ];
        for (case, wanted, read, expected) in cases {
            assert_eq!(after_change(wanted, read), expected, "{case}");
        }
    }
}
