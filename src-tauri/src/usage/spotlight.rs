use jiff::{SignedDuration, Timestamp};

use crate::bundle_id::BundleId;

/// `mdls -raw` prints dates as `2026-11-03 06:45:44 +0000`.
const MDLS_DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S %z";
const MDLS_NULL: &str = "(null)";
/// SUMMARY.md rule 4: this many Spotlight dates within `MIGRATION_WINDOW`
/// mark an OS update or migration that re-stamped them.
const MIGRATION_BATCH_SIZE: usize = 5;
const MIGRATION_WINDOW: SignedDuration = SignedDuration::from_secs(5);

/// A Spotlight `kMDItemLastUsedDate` judged sane at first run, or read back
/// from the launch log that stored it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpotlightSeed(Timestamp);

impl SpotlightSeed {
    /// For `log::decode`, which applies the per-entry checks itself.
    pub(super) fn from_log(at: Timestamp) -> Self {
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

/// True when `MIGRATION_BATCH_SIZE` dates fall within `MIGRATION_WINDOW`,
/// both ends included.
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
    /// The surviving dates formed a migration batch, so all were dropped.
    pub migration_batch: bool,
}

/// Drops dates after `now`, then drops every date if the rest form a
/// migration batch (SUMMARY.md rule 4 says "entirely").
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
