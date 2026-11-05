use super::{ActiveApp, OnSettingsClosed, Pid, hides, is_return_target, on_settings_closed};

const OWN_PID: Pid = Pid(100);
const OWN_ID: &str = "app.dockmoss.DockMoss";

#[test]
fn only_regular_apps_other_than_dockmoss_are_return_targets() {
    let cases = [
        ("another app", 200, Some("com.apple.finder"), true, true),
        ("another app without a bundle ID", 201, None, true, true),
        (
            "an agent such as a launcher",
            202,
            Some("com.apple.Spotlight"),
            false,
            false,
        ),
        ("this process", 100, Some(OWN_ID), false, false),
        (
            "this process without a bundle ID (tauri dev)",
            100,
            None,
            true,
            false,
        ),
        (
            "a second launch about to exit",
            300,
            Some(OWN_ID),
            true,
            false,
        ),
    ];
    for (case, pid, bundle_id, regular, expected) in cases {
        let app = ActiveApp {
            pid: Pid(pid),
            bundle_id: bundle_id.map(str::to_owned),
            regular,
        };

        assert_eq!(is_return_target(&app, OWN_PID, OWN_ID), expected, "{case}");
    }
}

fn app(pid: i32, bundle_id: &str) -> ActiveApp {
    ActiveApp {
        pid: Pid(pid),
        bundle_id: Some(bundle_id.to_owned()),
        regular: true,
    }
}

#[test]
fn closing_settings_returns_focus_only_while_dockmoss_is_in_front() {
    let finder = app(200, "com.apple.finder");
    let mail_on_finders_old_pid = app(200, "com.apple.mail");
    let cases = [
        (
            "in front, target still running",
            true,
            Some(&finder),
            Some(&finder),
            OnSettingsClosed::ReturnTo(Pid(200)),
        ),
        (
            "in front, target quit",
            true,
            Some(&finder),
            None,
            OnSettingsClosed::Hide,
        ),
        (
            "in front, target's pid now another app",
            true,
            Some(&finder),
            Some(&mail_on_finders_old_pid),
            OnSettingsClosed::Hide,
        ),
        (
            "in front, no target",
            true,
            None,
            None,
            OnSettingsClosed::Hide,
        ),
        (
            "behind another app, target still running",
            false,
            Some(&finder),
            Some(&finder),
            OnSettingsClosed::Leave,
        ),
        (
            "behind another app, no target",
            false,
            None,
            None,
            OnSettingsClosed::Leave,
        ),
    ];
    for (case, dockmoss_active, target, at_its_pid, expected) in cases {
        assert_eq!(
            on_settings_closed(dockmoss_active, target, at_its_pid),
            expected,
            "{case}"
        );
    }
}

#[test]
fn dockmoss_hides_when_no_app_took_the_focus_back() {
    let finder = Pid(200);
    let cases = [
        ("closed from behind", OnSettingsClosed::Leave, false, false),
        (
            "returned to the app",
            OnSettingsClosed::ReturnTo(finder),
            true,
            false,
        ),
        (
            "the app did not activate",
            OnSettingsClosed::ReturnTo(finder),
            false,
            true,
        ),
        ("no app to return to", OnSettingsClosed::Hide, false, true),
    ];
    for (case, decision, activated, expected) in cases {
        assert_eq!(hides(decision, activated), expected, "{case}");
    }
}
