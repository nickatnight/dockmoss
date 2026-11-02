use super::{TrayChange, tray_change};

#[test]
fn tray_follows_each_trust_reading() {
    let cases = [
        (
            "first reading untrusted",
            None,
            false,
            Some(TrayChange::ShowGrantItem),
        ),
        ("first reading trusted", None, true, None),
        (
            "grant revoked",
            Some(true),
            false,
            Some(TrayChange::ShowGrantItem),
        ),
        (
            "grant given",
            Some(false),
            true,
            Some(TrayChange::HideGrantItem),
        ),
        ("still trusted", Some(true), true, None),
        ("still untrusted", Some(false), false, None),
    ];

    for (case, previous, trusted, expected) in cases {
        assert_eq!(tray_change(previous, trusted), expected, "{case}");
    }
}
