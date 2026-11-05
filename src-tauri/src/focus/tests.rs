use super::{ActiveApp, OnSettingsClosed, Pid, is_return_target, on_settings_closed};

const OWN_PID: Pid = Pid(100);
const OWN_ID: &str = "com.dockmoss.app";

#[test]
fn only_apps_other_than_dockmoss_are_return_targets() {
    let cases = [
        ("another app", 200, Some("com.apple.finder"), true),
        ("another app without a bundle ID", 201, None, true),
        ("this process", 100, Some(OWN_ID), false),
        (
            "this process without a bundle ID (tauri dev)",
            100,
            None,
            false,
        ),
        ("a second launch about to exit", 300, Some(OWN_ID), false),
    ];
    for (case, pid, bundle_id, expected) in cases {
        let app = ActiveApp {
            pid: Pid(pid),
            bundle_id: bundle_id.map(str::to_owned),
        };

        assert_eq!(is_return_target(&app, OWN_PID, OWN_ID), expected, "{case}");
    }
}

#[test]
fn closing_settings_returns_focus_only_while_dockmoss_is_in_front() {
    let finder = Pid(200);
    let cases = [
        (
            "in front, target known",
            true,
            Some(finder),
            OnSettingsClosed::ReturnTo(finder),
        ),
        ("in front, no target", true, None, OnSettingsClosed::Hide),
        (
            "behind another app, target known",
            false,
            Some(finder),
            OnSettingsClosed::Leave,
        ),
        (
            "behind another app, no target",
            false,
            None,
            OnSettingsClosed::Leave,
        ),
    ];
    for (case, dockmoss_active, return_to, expected) in cases {
        assert_eq!(
            on_settings_closed(dockmoss_active, return_to),
            expected,
            "{case}"
        );
    }
}
