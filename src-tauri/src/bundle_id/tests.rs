use super::BundleId;

#[test]
fn a_non_empty_string_is_a_bundle_id() {
    let id = BundleId::new("com.apple.Safari").expect("non-empty bundle id");

    assert_eq!(id.as_str(), "com.apple.Safari");
}

#[test]
fn an_empty_string_is_not_a_bundle_id() {
    assert_eq!(BundleId::new(""), None);
}

#[test]
fn serializes_as_the_plain_string() {
    let id = BundleId::new("com.apple.Safari").expect("non-empty bundle id");

    let json = serde_json::to_value(&id).expect("BundleId serializes to JSON");

    assert_eq!(json, serde_json::json!("com.apple.Safari"));
}
