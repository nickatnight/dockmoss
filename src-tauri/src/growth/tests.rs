use super::{FastForward, GrowthError, Overgrowth, Stage, Thresholds};

#[test]
fn overgrowth_clamps_to_zero_one_and_maps_nan_to_zero() {
    let cases = [
        (-0.1_f32, 0.0_f32),
        (0.5, 0.5),
        (1.5, 1.0),
        (f32::NAN, 0.0),
        (f32::INFINITY, 1.0),
        (f32::NEG_INFINITY, 0.0),
    ];

    for (input, expected) in cases {
        let got = Overgrowth::new(input).get();
        assert!(
            (got - expected).abs() < 1e-6,
            "Overgrowth::new({input}) = {got}, expected {expected}"
        );
    }
}

#[test]
fn overgrowth_serializes_as_a_bare_number() {
    let json = serde_json::to_value(Overgrowth::new(0.5)).expect("Overgrowth serializes to JSON");

    assert_eq!(json, serde_json::json!(0.5));
}

#[test]
fn stages_order_from_clean_to_overgrown() {
    assert!(Stage::Clean < Stage::Fuzz);
    assert!(Stage::Fuzz < Stage::Patchy);
    assert!(Stage::Patchy < Stage::Overgrown);
}

#[test]
fn stages_serialize_as_lowercase_strings() {
    let cases = [
        (Stage::Clean, "clean"),
        (Stage::Fuzz, "fuzz"),
        (Stage::Patchy, "patchy"),
        (Stage::Overgrown, "overgrown"),
    ];

    for (stage, expected) in cases {
        let json = serde_json::to_value(stage).expect("Stage serializes to JSON");
        assert_eq!(json, serde_json::json!(expected), "{stage:?}");
    }
}

#[test]
fn default_thresholds_are_5_10_30_days() {
    let five_ten_thirty = Thresholds::new(5, 10, 30).expect("5/10/30 is valid");

    assert_eq!(Thresholds::default(), five_ten_thirty);
}

#[test]
fn thresholds_accept_strictly_increasing_days_from_2_to_3650() {
    for (fuzz, patchy, overgrown) in [(5, 10, 30), (2, 3, 4), (30, 90, 180), (2, 3, 3650)] {
        assert!(
            Thresholds::new(fuzz, patchy, overgrown).is_ok(),
            "{fuzz}/{patchy}/{overgrown} should be valid"
        );
    }
}

#[test]
fn thresholds_reject_out_of_range_or_unordered_days() {
    let cases = [
        (1, 10, 30, "fuzz below 2 days"),
        (5, 5, 30, "patchy equal to fuzz"),
        (5, 10, 10, "overgrown equal to patchy"),
        (10, 5, 30, "patchy before fuzz"),
        (5, 10, 3651, "overgrown past ten years"),
    ];

    for (fuzz, patchy, overgrown, why) in cases {
        let err = Thresholds::new(fuzz, patchy, overgrown).expect_err(why);
        assert_eq!(
            err,
            GrowthError::InvalidThresholds {
                fuzz,
                patchy,
                overgrown
            },
            "{why}"
        );
    }
}

#[test]
fn threshold_error_message_names_the_values_and_the_rule() {
    let err = Thresholds::new(5, 10, 3651).expect_err("3651 days is past the limit");
    let message = err.to_string();

    for part in ["fuzz=5", "patchy=10", "overgrown=3651", "3650"] {
        assert!(message.contains(part), "{message:?} lacks {part:?}");
    }
}

#[test]
fn fast_forward_factors_are_1_10_100_1000() {
    let cases = [
        (FastForward::X1, 1),
        (FastForward::X10, 10),
        (FastForward::X100, 100),
        (FastForward::X1000, 1000),
    ];

    for (fast_forward, factor) in cases {
        assert_eq!(fast_forward.factor(), factor, "{fast_forward:?}");
    }
}

#[test]
fn fast_forward_defaults_to_real_time() {
    assert_eq!(FastForward::default(), FastForward::X1);
}
