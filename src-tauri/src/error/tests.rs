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
fn overlay_failures_have_the_overlay_kind() {
    let error = Error::from(crate::macos::OverlayError::NullWindow);

    let json = serde_json::to_value(&error).expect("Error serializes to JSON");

    assert_eq!(json["kind"], "overlay");
}
