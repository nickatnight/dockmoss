use super::{Trust, TrustState};

#[test]
fn state_starts_at_the_first_reading() {
    for first in [Trust::Trusted, Trust::Untrusted] {
        assert_eq!(TrustState::new(first).current(), first);
    }
}

#[test]
fn replace_returns_the_previous_reading_and_keeps_the_new_one() {
    let cases = [
        ("grant revoked", Trust::Trusted, Trust::Untrusted),
        ("grant given", Trust::Untrusted, Trust::Trusted),
        ("still trusted", Trust::Trusted, Trust::Trusted),
        ("still untrusted", Trust::Untrusted, Trust::Untrusted),
    ];

    for (case, previous, reading) in cases {
        let state = TrustState::new(previous);

        assert_eq!(state.replace(reading), previous, "{case}: previous");
        assert_eq!(state.current(), reading, "{case}: current");
    }
}

#[test]
fn only_trusted_reports_trusted() {
    assert!(Trust::Trusted.is_trusted());
    assert!(!Trust::Untrusted.is_trusted());
}
