use serde::ser::{Serialize, SerializeStruct, Serializer};

/// Crate error returned by commands. Serializes as `{ kind, message }`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
    #[error("macOS reports no main display")]
    NoMainDisplay,
    #[error(transparent)]
    Overlay(#[from] crate::macos::overlay::OverlayError),
    #[error(transparent)]
    Dock(#[from] crate::dock::DockError),
    #[error("the Dock tracker is not running")]
    TrackerStopped,
    #[error("usage tracking is not running")]
    UsageStopped,
    #[error(transparent)]
    InvalidSettings(#[from] crate::settings::SettingsError),
    #[error("settings could not be saved")]
    SettingsNotSaved(#[from] crate::settings::NotSaved),
    #[error(transparent)]
    LoginItemNotChanged(#[from] crate::login_item::NotChanged),
    #[error("macOS did not report launch at login in time")]
    LoginStatusUnknown,
}

impl From<crate::login_item::ChangeFailed> for Error {
    fn from(failed: crate::login_item::ChangeFailed) -> Self {
        match failed {
            crate::login_item::ChangeFailed::NotChanged { .. } => {
                Self::LoginItemNotChanged(crate::login_item::NotChanged)
            }
            crate::login_item::ChangeFailed::StatusUnknown => Self::LoginStatusUnknown,
        }
    }
}

impl Error {
    fn kind(&self) -> &'static str {
        match self {
            Self::Tauri(_) => "tauri",
            Self::NoMainDisplay => "no_main_display",
            Self::Overlay(_) => "overlay",
            Self::Dock(_) => "dock",
            Self::TrackerStopped => "tracker_stopped",
            Self::UsageStopped => "usage_stopped",
            Self::InvalidSettings(_) => "invalid_settings",
            Self::SettingsNotSaved(_) => "settings_not_saved",
            Self::LoginItemNotChanged(_) => "login_item_not_changed",
            Self::LoginStatusUnknown => "login_status_unknown",
        }
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn serializes_as_kind_and_message() {
        let error = Error::Tauri(tauri::Error::WindowNotFound);

        let json = serde_json::to_value(&error).expect("Error serializes to JSON");

        assert_eq!(
            json,
            serde_json::json!({ "kind": "tauri", "message": error.to_string() })
        );
    }

    #[test]
    fn missing_main_display_has_its_own_kind() {
        let json = serde_json::to_value(Error::NoMainDisplay).expect("Error serializes to JSON");

        assert_eq!(
            json,
            serde_json::json!({ "kind": "no_main_display", "message": "macOS reports no main display" })
        );
    }

    #[test]
    fn overlay_failures_have_the_overlay_kind_and_message() {
        let json =
            serde_json::to_value(Error::from(crate::macos::overlay::OverlayError::NullWindow))
                .expect("Error serializes to JSON");

        assert_eq!(
            json,
            serde_json::json!({ "kind": "overlay", "message": "overlay's native window pointer is null" })
        );
    }

    #[test]
    fn dock_failures_have_the_dock_kind_and_message() {
        let json = serde_json::to_value(Error::from(crate::dock::DockError::ApiDisabled))
            .expect("Error serializes to JSON");

        assert_eq!(
            json,
            serde_json::json!({
                "kind": "dock",
                "message": "Accessibility access is not granted, so the Dock cannot be read"
            })
        );
    }

    #[test]
    fn dock_ax_failures_name_the_attribute_and_code() {
        let error = Error::from(crate::dock::DockError::Ax {
            attribute: "AXSubrole",
            code: -25202,
        });

        let json = serde_json::to_value(error).expect("Error serializes to JSON");

        assert_eq!(
            json,
            serde_json::json!({
                "kind": "dock",
                "message": "reading AXSubrole from the Dock failed with AX error -25202"
            })
        );
    }

    #[test]
    fn stopped_tracker_has_its_own_kind() {
        let json = serde_json::to_value(Error::TrackerStopped).expect("Error serializes to JSON");

        assert_eq!(
            json,
            serde_json::json!({ "kind": "tracker_stopped", "message": "the Dock tracker is not running" })
        );
    }

    #[test]
    fn stopped_usage_thread_has_its_own_kind() {
        let json = serde_json::to_value(Error::UsageStopped).expect("Error serializes to JSON");

        assert_eq!(
            json,
            serde_json::json!({
                "kind": "usage_stopped",
                "message": "usage tracking is not running"
            })
        );
    }

    #[test]
    fn invalid_settings_carry_the_field_check_as_their_message() {
        let error = Error::from(crate::settings::SettingsError::Intensity);

        let json = serde_json::to_value(error).expect("Error serializes to JSON");

        assert_eq!(
            json,
            serde_json::json!({
                "kind": "invalid_settings",
                "message": "intensity must be a whole percent from 0 to 100"
            })
        );
    }

    #[test]
    fn unsaved_settings_have_their_own_kind_and_no_path_in_the_message() {
        let cause = std::io::Error::other("could not create /Users/someone/settings.json.tmp");
        let json = serde_json::to_value(Error::from(crate::settings::NotSaved::new(cause)))
            .expect("Error serializes to JSON");

        assert_eq!(
            json,
            serde_json::json!({
                "kind": "settings_not_saved",
                "message": "settings could not be saved"
            })
        );
    }

    #[test]
    fn an_unchanged_login_item_has_its_own_kind() {
        let json = serde_json::to_value(Error::from(crate::login_item::NotChanged))
            .expect("Error serializes to JSON");

        assert_eq!(
            json,
            serde_json::json!({
                "kind": "login_item_not_changed",
                "message": "launch at login was not changed"
            })
        );
    }

    #[test]
    fn a_login_status_read_that_misses_its_bound_has_its_own_kind() {
        let json =
            serde_json::to_value(Error::LoginStatusUnknown).expect("Error serializes to JSON");

        assert_eq!(
            json,
            serde_json::json!({
                "kind": "login_status_unknown",
                "message": "macOS did not report launch at login in time"
            })
        );
    }

    #[test]
    fn error_kind_in_ipc_ts_lists_every_error_kind() {
        let errors = [
            Error::Tauri(tauri::Error::WindowNotFound),
            Error::NoMainDisplay,
            Error::Overlay(crate::macos::overlay::OverlayError::NullWindow),
            Error::Dock(crate::dock::DockError::ApiDisabled),
            Error::TrackerStopped,
            Error::UsageStopped,
            Error::InvalidSettings(crate::settings::SettingsError::Intensity),
            Error::SettingsNotSaved(crate::settings::NotSaved::new(std::io::Error::other("x"))),
            Error::LoginItemNotChanged(crate::login_item::NotChanged),
            Error::LoginStatusUnknown,
        ];
        for error in &errors {
            // A new variant must be added here and to `errors`; the `declared`
            // check below fails if `errors` misses the variant's kind.
            match error {
                Error::Tauri(_)
                | Error::NoMainDisplay
                | Error::Overlay(_)
                | Error::Dock(_)
                | Error::TrackerStopped
                | Error::UsageStopped
                | Error::InvalidSettings(_)
                | Error::SettingsNotSaved(_)
                | Error::LoginItemNotChanged(_)
                | Error::LoginStatusUnknown => {}
            }
        }
        let rust: std::collections::BTreeSet<&str> = errors.iter().map(Error::kind).collect();

        let ipc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../src/ipc.ts"));
        let union = ipc
            .split("export type ErrorKind =")
            .nth(1)
            .and_then(|rest| rest.split(';').next())
            .expect("src/ipc.ts declares ErrorKind");
        let typescript: std::collections::BTreeSet<&str> =
            union.split('"').skip(1).step_by(2).collect();

        let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/error.rs"));
        let kind_body = source
            .split("fn kind(&self)")
            .nth(1)
            .and_then(|rest| rest.split("\n    }\n").next())
            .expect("src/error.rs defines Error::kind");
        let declared: std::collections::BTreeSet<&str> = kind_body
            .split("=> \"")
            .skip(1)
            .filter_map(|arm| arm.split('"').next())
            .collect();

        assert_eq!(
            rust, declared,
            "`errors` misses a kind that Error::kind() returns"
        );
        assert_eq!(typescript, declared, "ErrorKind in src/ipc.ts");
    }
}
