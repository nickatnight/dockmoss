use super::{RunningApp, tracked_bundle_id};
use crate::bundle_id::BundleId;

const OWN: &str = "app.dockmoss.DockMoss";

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
            "an ID that extends DockMoss's",
            Some("app.dockmoss.DockMoss.helper"),
            true,
            BundleId::new("app.dockmoss.DockMoss.helper"),
        ),
        (
            "an ID that DockMoss's extends",
            Some("app.dockmoss"),
            true,
            BundleId::new("app.dockmoss"),
        ),
        (
            "the identifier DockMoss had before slice 13",
            Some("com.dockmoss.app"),
            true,
            BundleId::new("com.dockmoss.app"),
        ),
        (
            "an agent or helper with an ID",
            Some("com.apple.Spotlight"),
            false,
            None,
        ),
    ];
    for (case, bundle_id, can_have_dock_icon, expected) in cases {
        let app = RunningApp {
            bundle_id: bundle_id.map(str::to_owned),
            can_have_dock_icon,
        };

        assert_eq!(tracked_bundle_id(app, OWN), expected, "{case}");
    }
}

#[test]
fn another_dockmoss_process_is_not_tracked() {
    let second_launch = RunningApp {
        bundle_id: Some(OWN.to_owned()),
        can_have_dock_icon: true,
    };

    assert_eq!(tracked_bundle_id(second_launch, OWN), None);
}
