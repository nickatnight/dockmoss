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
pub enum SaneSeeds {
    Kept(Vec<(BundleId, SpotlightSeed)>),
    /// The dates formed a migration batch, so none is kept.
    MigrationBatch,
}

pub fn sane_seeds(readings: Vec<(BundleId, Option<Timestamp>)>, now: Timestamp) -> SaneSeeds {
    let past = latest_past_date_per_app(readings, now);
    let dates: Vec<Timestamp> = past.iter().map(|(_, date)| *date).collect();
    if is_migration_batch(&dates) {
        return SaneSeeds::MigrationBatch;
    }
    SaneSeeds::Kept(
        past.into_iter()
            .map(|(bundle_id, date)| (bundle_id, SpotlightSeed(date)))
            .collect(),
    )
}

fn latest_past_date_per_app(
    readings: Vec<(BundleId, Option<Timestamp>)>,
    now: Timestamp,
) -> Vec<(BundleId, Timestamp)> {
    let mut past: Vec<(BundleId, Timestamp)> = Vec::new();
    for (bundle_id, date) in readings {
        let Some(past_date) = date.filter(|date| *date <= now) else {
            continue;
        };
        match past.iter_mut().find(|(seen, _)| *seen == bundle_id) {
            Some((_, kept)) => *kept = (*kept).max(past_date),
            None => past.push((bundle_id, past_date)),
        }
    }
    past
}

#[cfg(test)]
mod tests;
