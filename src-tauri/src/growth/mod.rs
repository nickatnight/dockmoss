use serde::Serialize;

mod curve;
mod jitter;
mod last_used;

pub use curve::growth;
pub use jitter::jitter;
pub use last_used::{LastUsedSources, effective_last_used};

const MIN_FUZZ_DAYS: u32 = jitter::MAX_JITTER_DAYS + 1;
const MAX_OVERGROWN_DAYS: u32 = 3650;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Overgrowth(f32);

impl Overgrowth {
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
mod tests;
