use jiff::{SignedDuration, Timestamp};

use super::{FastForward, Growth, Overgrowth, Stage, Thresholds};

const SECONDS_PER_DAY: f64 = 86_400.0;
const AT_PATCHY: f64 = 0.5;

const CLEAN: Growth = Growth {
    overgrowth: Overgrowth(0.0),
    stage: Stage::Clean,
};

pub fn growth(
    now: Timestamp,
    last_used: Option<Timestamp>,
    jitter: SignedDuration,
    thresholds: &Thresholds,
    fast_forward: FastForward,
) -> Growth {
    let Some(last_used) = last_used else {
        return CLEAN;
    };
    let elapsed = now.duration_since(last_used);
    if elapsed <= SignedDuration::ZERO {
        return CLEAN;
    }
    let seconds = elapsed.as_secs_f64() * f64::from(fast_forward.factor()) + jitter.as_secs_f64();
    at_day(seconds / SECONDS_PER_DAY, thresholds)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "overgrowth is in 0..=1, so f32 loses only precision"
)]
fn at_day(days: f64, thresholds: &Thresholds) -> Growth {
    let fuzz = f64::from(thresholds.fuzz);
    let patchy = f64::from(thresholds.patchy);
    let overgrown = f64::from(thresholds.overgrown);
    let (stage, overgrowth) = if days < fuzz {
        (Stage::Clean, 0.0)
    } else if days < patchy {
        (Stage::Fuzz, AT_PATCHY * (days - fuzz) / (patchy - fuzz))
    } else if days < overgrown {
        let rise = (1.0 - AT_PATCHY) * (days - patchy) / (overgrown - patchy);
        (Stage::Patchy, AT_PATCHY + rise)
    } else {
        (Stage::Overgrown, 1.0)
    };
    Growth {
        overgrowth: Overgrowth::new(overgrowth as f32),
        stage,
    }
}

#[cfg(test)]
mod tests;
