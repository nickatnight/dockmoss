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
