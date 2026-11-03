use std::collections::BTreeMap;
use std::fmt;

use jiff::{SignedDuration, Timestamp};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::spotlight::SpotlightSeed;
use crate::bundle_id::BundleId;

const VERSION: u64 = 1;

pub const FUTURE_TOLERANCE: SignedDuration = SignedDuration::from_mins(5);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchLog {
    last_launched: BTreeMap<BundleId, Timestamp>,
    first_seen: BTreeMap<BundleId, Timestamp>,
    spotlight_seed: BTreeMap<BundleId, SpotlightSeed>,
    baseline_at: Option<Timestamp>,
}

impl LaunchLog {
    pub fn last_launched(&self, bundle_id: &BundleId) -> Option<Timestamp> {
        self.last_launched.get(bundle_id).copied()
    }

    pub fn first_seen(&self, bundle_id: &BundleId) -> Option<Timestamp> {
        self.first_seen.get(bundle_id).copied()
    }

    pub fn spotlight_seed(&self, bundle_id: &BundleId) -> Option<SpotlightSeed> {
        self.spotlight_seed.get(bundle_id).copied()
    }

    pub fn baseline_at(&self) -> Option<Timestamp> {
        self.baseline_at
    }

    pub fn set_baseline_at(&mut self, at: Timestamp) {
        self.baseline_at = Some(at);
    }

    pub fn set_first_seen(&mut self, bundle_id: BundleId, at: Timestamp) {
        self.first_seen.insert(bundle_id, at);
    }

    pub fn set_spotlight_seed(&mut self, bundle_id: BundleId, seed: SpotlightSeed) {
        self.spotlight_seed.insert(bundle_id, seed);
    }

    pub fn len(&self) -> usize {
        self.last_launched.len()
    }

    pub fn record(&mut self, bundle_id: BundleId, at: Timestamp) -> Recorded {
        if self.last_launched.insert(bundle_id, at) == Some(at) {
            Recorded::Unchanged
        } else {
            Recorded::Changed
        }
    }
}

/// Whether `LaunchLog::record` changed the log, so it needs saving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recorded {
    Changed,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dropped {
    Entry {
        bundle_id: String,
        reason: DropReason,
    },
    Baseline {
        reason: DropReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    InvalidBundleId,
    NotAString,
    UnparseableTimestamp,
    InFuture,
}

impl fmt::Display for DropReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBundleId => f.write_str("the bundle ID is empty or longer than 255 bytes"),
            Self::NotAString => f.write_str("the value is not a string"),
            Self::UnparseableTimestamp => f.write_str("the timestamp is not RFC 3339"),
            Self::InFuture => write!(
                f,
                "the timestamp is more than {} minutes in the future",
                FUTURE_TOLERANCE.as_mins()
            ),
        }
    }
}

#[derive(Debug)]
pub struct Decoded {
    pub log: LaunchLog,
    pub dropped: Vec<Dropped>,
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("not a launch log: {0}")]
    Shape(#[from] serde_json::Error),
    #[error("launch log version {0} is not supported (expected {VERSION})")]
    Version(u64),
}

#[derive(Deserialize)]
struct FileVersion {
    version: u64,
}

#[derive(Deserialize)]
struct FileV1 {
    #[serde(default)]
    baseline_at: Value,
    last_launched: BTreeMap<String, Value>,
    #[serde(default)]
    first_seen: BTreeMap<String, Value>,
    #[serde(default)]
    spotlight_seed: BTreeMap<String, Value>,
}

pub fn encode(log: &LaunchLog) -> String {
    let file = json!({
        "version": VERSION,
        "baseline_at": log.baseline_at.map(|at| at.to_string()),
        "last_launched": encode_map(&log.last_launched, |at| *at),
        "first_seen": encode_map(&log.first_seen, |at| *at),
        "spotlight_seed": encode_map(&log.spotlight_seed, |seed| seed.at()),
    });
    format!("{file:#}\n")
}

fn encode_map<T>(
    entries: &BTreeMap<BundleId, T>,
    at: impl Fn(&T) -> Timestamp,
) -> Map<String, Value> {
    entries
        .iter()
        .map(|(bundle_id, value)| {
            (
                bundle_id.as_str().to_owned(),
                Value::String(at(value).to_string()),
            )
        })
        .collect()
}

pub fn decode(text: &str, now: Timestamp) -> Result<Decoded, DecodeError> {
    let FileVersion { version } = serde_json::from_str(text)?;
    if version != VERSION {
        return Err(DecodeError::Version(version));
    }
    let file: FileV1 = serde_json::from_str(text)?;
    let latest = now.checked_add(FUTURE_TOLERANCE).unwrap_or(Timestamp::MAX);
    let mut dropped = Vec::new();
    let log = LaunchLog {
        last_launched: decode_map(file.last_launched, latest, &mut dropped),
        first_seen: decode_map(file.first_seen, latest, &mut dropped),
        spotlight_seed: decode_map(file.spotlight_seed, latest, &mut dropped)
            .into_iter()
            .map(|(bundle_id, at)| (bundle_id, SpotlightSeed::assume_sane(at)))
            .collect(),
        baseline_at: decode_baseline(&file.baseline_at, latest, &mut dropped),
    };
    Ok(Decoded { log, dropped })
}

fn decode_baseline(
    value: &Value,
    latest: Timestamp,
    dropped: &mut Vec<Dropped>,
) -> Option<Timestamp> {
    if value.is_null() {
        return None;
    }
    match timestamp(value, latest) {
        Ok(at) => Some(at),
        Err(reason) => {
            dropped.push(Dropped::Baseline { reason });
            None
        }
    }
}

fn decode_map(
    entries: BTreeMap<String, Value>,
    latest: Timestamp,
    dropped: &mut Vec<Dropped>,
) -> BTreeMap<BundleId, Timestamp> {
    let mut decoded = BTreeMap::new();
    for (key, value) in entries {
        match entry(&key, &value, latest) {
            Ok((bundle_id, at)) => {
                decoded.insert(bundle_id, at);
            }
            Err(reason) => dropped.push(Dropped::Entry {
                bundle_id: key,
                reason,
            }),
        }
    }
    decoded
}

fn entry(key: &str, value: &Value, latest: Timestamp) -> Result<(BundleId, Timestamp), DropReason> {
    let bundle_id = BundleId::new(key).ok_or(DropReason::InvalidBundleId)?;
    Ok((bundle_id, timestamp(value, latest)?))
}

fn timestamp(value: &Value, latest: Timestamp) -> Result<Timestamp, DropReason> {
    let text = value.as_str().ok_or(DropReason::NotAString)?;
    let at: Timestamp = text.parse().map_err(|_| DropReason::UnparseableTimestamp)?;
    if at > latest {
        return Err(DropReason::InFuture);
    }
    Ok(at)
}

#[cfg(test)]
mod tests;
