use jiff::{SignedDuration, Timestamp};

use crate::bundle_id::BundleId;

const MDLS_DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S %z";
const MDLS_NULL: &str = "(null)";
const MIGRATION_BATCH_SIZE: usize = 5;
const MIGRATION_WINDOW: SignedDuration = SignedDuration::from_secs(5);

/// A Spotlight `kMDItemLastUsedDate` judged sane at first run, or read back
/// from the launch log that stored it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpotlightSeed(Timestamp);

impl SpotlightSeed {
    pub(super) fn assume_sane(at: Timestamp) -> Self {
        Self(at)
    }

    pub fn at(self) -> Timestamp {
        self.0
    }
}

pub fn parse_mdls(stdout: &str) -> Option<Timestamp> {
    let text = stdout.trim();
    if text == MDLS_NULL {
        return None;
    }
    Timestamp::strptime(MDLS_DATE_FORMAT, text).ok()
}

pub fn is_migration_batch(dates: &[Timestamp]) -> bool {
    let mut sorted = dates.to_vec();
    sorted.sort_unstable();
    sorted.windows(MIGRATION_BATCH_SIZE).any(|window| {
        let (Some(first), Some(last)) = (window.first(), window.last()) else {
            return false;
        };
        last.duration_since(*first) <= MIGRATION_WINDOW
    })
}

#[derive(Debug)]
pub struct SaneSeeds {
    pub seeds: Vec<(BundleId, SpotlightSeed)>,
    pub migration_batch: bool,
}

pub fn sane_seeds(readings: Vec<(BundleId, Option<Timestamp>)>, now: Timestamp) -> SaneSeeds {
    let past: Vec<(BundleId, Timestamp)> = readings
        .into_iter()
        .filter_map(|(bundle_id, date)| Some((bundle_id, date.filter(|date| *date <= now)?)))
        .collect();
    let dates: Vec<Timestamp> = past.iter().map(|(_, date)| *date).collect();
    if is_migration_batch(&dates) {
        return SaneSeeds {
            seeds: Vec::new(),
            migration_batch: true,
        };
    }
    SaneSeeds {
        seeds: past
            .into_iter()
            .map(|(bundle_id, date)| (bundle_id, SpotlightSeed(date)))
            .collect(),
        migration_batch: false,
    }
}

#[cfg(test)]
mod tests;
