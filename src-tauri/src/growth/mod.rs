use serde::Serialize;

mod curve;
mod jitter;
mod last_used;

pub use curve::growth;
pub use jitter::jitter;
pub use last_used::{LastUsedSources, effective_last_used};

pub const MIN_FUZZ_DAYS: u32 = jitter::MAX_JITTER_DAYS + 1;
pub const MAX_OVERGROWN_DAYS: u32 = 3650;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Overgrowth(f32);

impl Overgrowth {
    /// Clamps to 0..=1. NaN and every value at or below zero, `-0.0`
    /// included, become `+0.0`.
    pub fn new(value: f32) -> Self {
        if value.is_nan() || value <= 0.0 {
            Self(0.0)
        } else {
            Self(value.min(1.0))
        }
    }

    pub fn get(self) -> f32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Clean,
    Fuzz,
    Patchy,
    Overgrown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    fuzz: u32,
    patchy: u32,
    overgrown: u32,
}

impl Thresholds {
    pub fn new(fuzz: u32, patchy: u32, overgrown: u32) -> Result<Self, GrowthError> {
        if MIN_FUZZ_DAYS <= fuzz
            && fuzz < patchy
            && patchy < overgrown
            && overgrown <= MAX_OVERGROWN_DAYS
        {
            Ok(Self {
                fuzz,
                patchy,
                overgrown,
            })
        } else {
            Err(GrowthError::InvalidThresholds {
                fuzz,
                patchy,
                overgrown,
            })
        }
    }

    pub fn fuzz(self) -> u32 {
        self.fuzz
    }

    pub fn patchy(self) -> u32 {
        self.patchy
    }

    pub fn overgrown(self) -> u32 {
        self.overgrown
    }
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            fuzz: 5,
            patchy: 10,
            overgrown: 30,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FastForward {
    #[default]
    X1,
    X10,
    X100,
    X1000,
}

impl FastForward {
    pub fn from_factor(factor: u32) -> Option<Self> {
        match factor {
            1 => Some(Self::X1),
            10 => Some(Self::X10),
            100 => Some(Self::X100),
            1000 => Some(Self::X1000),
            _ => None,
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        text.trim().parse().ok().and_then(Self::from_factor)
    }

    pub fn factor(self) -> u32 {
        match self {
            Self::X1 => 1,
            Self::X10 => 10,
            Self::X100 => 100,
            Self::X1000 => 1000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GrowthError {
    #[error(
        "invalid stage thresholds fuzz={fuzz} patchy={patchy} overgrown={overgrown}: \
         they must satisfy {min} <= fuzz < patchy < overgrown <= {max} days",
        min = MIN_FUZZ_DAYS,
        max = MAX_OVERGROWN_DAYS
    )]
    InvalidThresholds {
        fuzz: u32,
        patchy: u32,
        overgrown: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Growth {
    overgrowth: Overgrowth,
    stage: Stage,
}

impl Growth {
    pub fn overgrowth(self) -> Overgrowth {
        self.overgrowth
    }

    pub fn stage(self) -> Stage {
        self.stage
    }
}

#[cfg(test)]
mod tests {
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
    fn overgrowth_of_negative_zero_is_positive_zero() {
        let zero = Overgrowth::new(-0.0);

        assert!(zero.get().is_sign_positive(), "{:?}", zero.get());
        assert_eq!(
            serde_json::to_string(&zero).expect("Overgrowth serializes to JSON"),
            "0.0"
        );
    }

    #[test]
    fn overgrowth_serializes_as_a_bare_number() {
        let json =
            serde_json::to_value(Overgrowth::new(0.5)).expect("Overgrowth serializes to JSON");

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

    #[test]
    fn fast_forward_from_factor_accepts_only_the_four_speeds() {
        let cases = [
            (1, Some(FastForward::X1)),
            (10, Some(FastForward::X10)),
            (100, Some(FastForward::X100)),
            (1000, Some(FastForward::X1000)),
            (0, None),
            (2, None),
            (50, None),
            (10_000, None),
        ];
        for (factor, expected) in cases {
            assert_eq!(FastForward::from_factor(factor), expected, "{factor}");
        }
    }

    #[test]
    fn fast_forward_parses_a_factor_written_as_text() {
        let cases = [
            ("1000", Some(FastForward::X1000)),
            (" 10\n", Some(FastForward::X10)),
            ("1", Some(FastForward::X1)),
            ("7", None),
            ("-10", None),
            ("10x", None),
            ("", None),
        ];
        for (text, expected) in cases {
            assert_eq!(FastForward::parse(text), expected, "{text:?}");
        }
    }
}
