use super::{RunningApp, tracked_bundle_id};
use crate::bundle_id::BundleId;

#[test]
fn only_regular_apps_with_a_bundle_id_are_tracked() {
    let cases = [
        (
            "regular with an ID",
            Some("com.apple.TextEdit"),
            true,
            BundleId::new("com.apple.TextEdit"),
        ),
        ("regular without an ID", None, true, None),
        ("regular with an empty ID", Some(""), true, None),
        (
            "an agent or helper with an ID",
            Some("com.apple.Spotlight"),
            false,
            None,
        ),
    ];
    for (case, bundle_id, regular, expected) in cases {
        let app = RunningApp {
            bundle_id: bundle_id.map(str::to_owned),
            regular,
        };

        assert_eq!(tracked_bundle_id(app), expected, "{case}");
    }
}
