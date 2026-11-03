use serde::Serialize;

/// The smallest Fuzz threshold, in days. It is larger than the largest
/// jitter (1 day), so jitter alone never puts moss on a just-used app.
const MIN_FUZZ_DAYS: u32 = 2;
/// The largest Overgrown threshold, in days: ten years.
const MAX_OVERGROWN_DAYS: u32 = 3650;

/// How overgrown an icon is, from 0 (clean) to 1 (fully overgrown).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Overgrowth(f32);

impl Overgrowth {
    /// Clamps `value` to `0.0..=1.0`. NaN becomes 0.
    pub fn new(value: f32) -> Self {
        if value.is_nan() {
            Self(0.0)
        } else {
            Self(value.clamp(0.0, 1.0))
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

/// The whole days at which each stage begins. Every value of this type
/// satisfies `2 <= fuzz < patchy < overgrown <= 3650`.
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

/// How much faster than real time moss grows. The settings offer exactly
/// these four speeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FastForward {
    #[default]
    X1,
    X10,
    X100,
    X1000,
}

impl FastForward {
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
         they must satisfy 2 <= fuzz < patchy < overgrown <= 3650 days"
    )]
    InvalidThresholds {
        fuzz: u32,
        patchy: u32,
        overgrown: u32,
    },
}

#[cfg(test)]
mod tests;
