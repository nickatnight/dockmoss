use super::BundleId;

#[test]
fn a_non_empty_string_is_a_bundle_id() {
    assert!(BundleId::new("com.apple.Safari").is_some());
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

#[test]
fn a_bundle_id_is_at_most_255_bytes() {
    let cases = [
        ("255 ASCII bytes", "a".repeat(255), true),
        ("256 ASCII bytes", "a".repeat(256), false),
        ("127 two-byte characters (254 bytes)", "é".repeat(127), true),
        (
            "128 two-byte characters (256 bytes)",
            "é".repeat(128),
            false,
        ),
    ];
    for (case, id, valid) in cases {
        assert_eq!(BundleId::new(id).is_some(), valid, "{case}");
    }
}
