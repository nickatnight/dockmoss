use super::{LoginItemStatus, NotChanged, outcome};

#[test]
fn only_an_enabled_item_opens_at_login() {
    let cases = [
        (LoginItemStatus::NotRegistered, false),
        (LoginItemStatus::Enabled, true),
        (LoginItemStatus::RequiresApproval, false),
        (LoginItemStatus::NotFound, false),
    ];
    for (status, expected) in cases {
        assert_eq!(status.opens_at_login(), expected, "{status:?}");
    }
}

#[test]
fn a_change_succeeds_only_when_macos_ends_in_the_state_asked_for() {
    let cases = [
        ("on, now enabled", true, LoginItemStatus::Enabled, Ok(true)),
        (
            "on, still off",
            true,
            LoginItemStatus::NotRegistered,
            Err(NotChanged),
        ),
        (
            "on, waiting for approval",
            true,
            LoginItemStatus::RequiresApproval,
            Err(NotChanged),
        ),
        (
            "on, not an app bundle",
            true,
            LoginItemStatus::NotFound,
            Err(NotChanged),
        ),
        (
            "off, now off",
            false,
            LoginItemStatus::NotRegistered,
            Ok(false),
        ),
        (
            "off, never registered",
            false,
            LoginItemStatus::NotFound,
            Ok(false),
        ),
        (
            "off, still enabled",
            false,
            LoginItemStatus::Enabled,
            Err(NotChanged),
        ),
    ];
    for (case, wanted, after, expected) in cases {
        assert_eq!(outcome(wanted, after), expected, "{case}");
    }
}
