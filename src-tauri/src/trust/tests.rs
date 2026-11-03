use super::{Trust, TrustState};

#[test]
fn state_starts_at_the_first_reading() {
    for first in [Trust::Trusted, Trust::Untrusted] {
        assert_eq!(TrustState::new(first).current(), first);
    }
}

#[test]
fn replace_if_changed_returns_the_reading_only_when_it_changed() {
    let cases = [
        (
            "grant revoked",
            Trust::Trusted,
            Trust::Untrusted,
            Some(Trust::Untrusted),
        ),
        (
            "grant given",
            Trust::Untrusted,
            Trust::Trusted,
            Some(Trust::Trusted),
        ),
        ("still trusted", Trust::Trusted, Trust::Trusted, None),
        ("still untrusted", Trust::Untrusted, Trust::Untrusted, None),
    ];

    for (case, previous, reading, changed) in cases {
        let state = TrustState::new(previous);

        assert_eq!(state.replace_if_changed(reading), changed, "{case}: change");
        assert_eq!(state.current(), reading, "{case}: current");
    }
}

#[test]
fn only_trusted_reports_trusted() {
    assert!(Trust::Trusted.is_trusted());
    assert!(!Trust::Untrusted.is_trusted());
}
