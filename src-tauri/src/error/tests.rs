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
