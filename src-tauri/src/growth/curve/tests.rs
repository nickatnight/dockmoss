use jiff::{SignedDuration, Timestamp};

use crate::growth::{FastForward, Growth, Stage, Thresholds, growth};

const TOLERANCE: f32 = 1e-6;
const ZERO: SignedDuration = SignedDuration::ZERO;
const PLUS_DAY: SignedDuration = SignedDuration::from_hours(24);
const MINUS_DAY: SignedDuration = SignedDuration::from_hours(-24);

fn now() -> Timestamp {
    "2026-11-02T12:00:00Z".parse().expect("valid RFC 3339")
}

fn hours(n: i64) -> SignedDuration {
    SignedDuration::from_hours(n)
}

fn days(n: i64) -> SignedDuration {
    SignedDuration::from_hours(n * 24)
}

fn assert_growth(name: &str, actual: Growth, overgrowth: f32, stage: Stage) {
    let got = actual.overgrowth().get();
    assert!(
        (got - overgrowth).abs() < TOLERANCE,
        "{name}: overgrowth {got}, expected {overgrowth}"
    );
    assert_eq!(actual.stage(), stage, "{name}");
}

#[test]
fn unknown_last_used_is_clean() {
    let result = growth(
        now(),
        None,
        PLUS_DAY,
        &Thresholds::default(),
        FastForward::X1000,
    );

    assert_growth("unknown", result, 0.0, Stage::Clean);
}

#[test]
fn just_used_is_clean_whatever_the_jitter() {
    let thresholds = Thresholds::default();
    let cases = [
        ("elapsed 0, jitter +1 d", ZERO, PLUS_DAY),
        // Real jitter never exceeds one day, but growth() takes any offset.
        // This row is the one that fails if the elapsed-zero rule is dropped.
        ("elapsed 0, jitter +30 d", ZERO, days(30)),
        ("last used 1 h after now, jitter +1 d", hours(-1), PLUS_DAY),
        ("last used 1 h after now, jitter +30 d", hours(-1), days(30)),
    ];

    for (name, elapsed, jitter) in cases {
        let result = growth(
            now(),
            Some(now() - elapsed),
            jitter,
            &thresholds,
            FastForward::X1000,
        );
        assert_growth(name, result, 0.0, Stage::Clean);
    }
}

#[test]
fn default_curve_holds_clean_to_5_days_then_rises_to_half_at_10_and_full_at_30() {
    let cases = [
        ("4 d 23 h", hours(4 * 24 + 23), 0.0, Stage::Clean),
        ("5 d", days(5), 0.0, Stage::Fuzz),
        ("7.5 d", hours(180), 0.25, Stage::Fuzz),
        ("10 d", days(10), 0.5, Stage::Patchy),
        ("20 d", days(20), 0.75, Stage::Patchy),
        ("30 d", days(30), 1.0, Stage::Overgrown),
        ("400 d", days(400), 1.0, Stage::Overgrown),
    ];

    for (name, elapsed, overgrowth, stage) in cases {
        let result = growth(
            now(),
            Some(now() - elapsed),
            ZERO,
            &Thresholds::default(),
            FastForward::X1,
        );
        assert_growth(name, result, overgrowth, stage);
    }
}

#[test]
fn jitter_shifts_the_stage_boundaries_by_up_to_a_day() {
    let cases = [
        ("4 d with +1 d", days(4), PLUS_DAY, 0.0, Stage::Fuzz),
        ("5.5 d with -1 d", hours(132), MINUS_DAY, 0.0, Stage::Clean),
        ("1 h with -1 d", hours(1), MINUS_DAY, 0.0, Stage::Clean),
    ];

    for (name, elapsed, jitter, overgrowth, stage) in cases {
        let result = growth(
            now(),
            Some(now() - elapsed),
            jitter,
            &Thresholds::default(),
            FastForward::X1,
        );
        assert_growth(name, result, overgrowth, stage);
    }
}

#[test]
fn fast_forward_scales_elapsed_time_but_not_jitter() {
    let cases = [
        (
            "1 h at 10x",
            hours(1),
            ZERO,
            FastForward::X10,
            0.0,
            Stage::Clean,
        ),
        (
            "1 d at 10x",
            days(1),
            ZERO,
            FastForward::X10,
            0.5,
            Stage::Patchy,
        ),
        (
            "1 h at 100x",
            hours(1),
            ZERO,
            FastForward::X100,
            0.0,
            Stage::Clean,
        ),
        (
            "1 d at 100x",
            days(1),
            ZERO,
            FastForward::X100,
            1.0,
            Stage::Overgrown,
        ),
        (
            "1 h at 1000x",
            hours(1),
            ZERO,
            FastForward::X1000,
            1.0,
            Stage::Overgrown,
        ),
        // 432 s x 1000 = 5 d, plus 1 d of jitter = 6 d. Scaling the jitter
        // too would give 1,005 d and a fully overgrown icon.
        (
            "432 s at 1000x with +1 d",
            SignedDuration::from_secs(432),
            PLUS_DAY,
            FastForward::X1000,
            0.1,
            Stage::Fuzz,
        ),
    ];

    for (name, elapsed, jitter, fast_forward, overgrowth, stage) in cases {
        let result = growth(
            now(),
            Some(now() - elapsed),
            jitter,
            &Thresholds::default(),
            fast_forward,
        );
        assert_growth(name, result, overgrowth, stage);
    }
}

#[test]
fn custom_thresholds_move_the_knots() {
    let thresholds = Thresholds::new(30, 90, 180).expect("30/90/180 is valid");
    let cases = [
        ("60 d", days(60), 0.25, Stage::Fuzz),
        ("90 d", days(90), 0.5, Stage::Patchy),
    ];

    for (name, elapsed, overgrowth, stage) in cases {
        let result = growth(
            now(),
            Some(now() - elapsed),
            ZERO,
            &thresholds,
            FastForward::X1,
        );
        assert_growth(name, result, overgrowth, stage);
    }
}

#[test]
fn growth_never_decreases_and_stays_in_range_as_time_passes() {
    let thresholds = Thresholds::default();
    let last_used = now();
    let speeds = [
        FastForward::X1,
        FastForward::X10,
        FastForward::X100,
        FastForward::X1000,
    ];

    for fast_forward in speeds {
        for jitter in [MINUS_DAY, ZERO, PLUS_DAY] {
            let mut previous = growth(
                last_used,
                Some(last_used),
                jitter,
                &thresholds,
                fast_forward,
            );
            for hour in 1..=400 * 24 {
                let current = growth(
                    last_used + hours(hour),
                    Some(last_used),
                    jitter,
                    &thresholds,
                    fast_forward,
                );
                let value = current.overgrowth().get();
                let at = format!("{fast_forward:?}, jitter {jitter:?}, hour {hour}");
                assert!((0.0..=1.0).contains(&value), "{at}: overgrowth {value}");
                assert!(
                    value >= previous.overgrowth().get(),
                    "{at}: overgrowth fell"
                );
                assert!(current.stage() >= previous.stage(), "{at}: stage fell");
                previous = current;
            }
        }
    }
}

#[test]
fn extreme_timestamps_do_not_panic() {
    let thresholds = Thresholds::default();

    let oldest = growth(
        Timestamp::MAX,
        Some(Timestamp::MIN),
        PLUS_DAY,
        &thresholds,
        FastForward::X1000,
    );
    let reversed = growth(
        Timestamp::MIN,
        Some(Timestamp::MAX),
        PLUS_DAY,
        &thresholds,
        FastForward::X1000,
    );

    assert_growth("MIN used, MAX now", oldest, 1.0, Stage::Overgrown);
    assert_growth("MAX used, MIN now", reversed, 0.0, Stage::Clean);
}
