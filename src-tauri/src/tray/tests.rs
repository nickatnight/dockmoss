use super::{MenuEntry, menu_entries, tooltip};
use crate::trust::Trust;

#[test]
fn tooltip_says_when_accessibility_access_is_missing() {
    assert_eq!(tooltip(Trust::Trusted), "DockMoss");
    assert_eq!(
        tooltip(Trust::Untrusted),
        "DockMoss needs Accessibility access"
    );
}

#[test]
fn menu_offers_settings_with_and_without_accessibility_access() {
    let cases = [
        (
            Trust::Trusted,
            vec![MenuEntry::Settings, MenuEntry::Separator, MenuEntry::Quit],
        ),
        (
            Trust::Untrusted,
            vec![
                MenuEntry::GrantAccessibility,
                MenuEntry::Settings,
                MenuEntry::Separator,
                MenuEntry::Quit,
            ],
        ),
    ];

    for (trust, entries) in cases {
        assert_eq!(menu_entries(trust), entries.as_slice(), "{trust:?}");
    }
}
