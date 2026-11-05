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
    let json = serde_json::to_value(Error::from(crate::macos::overlay::OverlayError::NullWindow))
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
            | Error::LoginItemNotChanged(_) => {}
        }
    }
    let rust: std::collections::BTreeSet<&str> = errors.iter().map(Error::kind).collect();

    let ipc = include_str!("../../../src/ipc.ts");
    let union = ipc
        .split("export type ErrorKind =")
        .nth(1)
        .and_then(|rest| rest.split(';').next())
        .expect("src/ipc.ts declares ErrorKind");
    let typescript: std::collections::BTreeSet<&str> =
        union.split('"').skip(1).step_by(2).collect();

    let source = include_str!("../error.rs");
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
