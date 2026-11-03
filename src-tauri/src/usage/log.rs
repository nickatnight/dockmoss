//! The app's own launch log: the last time each app was launched, made
//! frontmost or quit, keyed by bundle ID (SUMMARY.md "Last-used data", rule 1).

use std::collections::BTreeMap;
use std::fmt;

use jiff::{SignedDuration, Timestamp};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::bundle_id::BundleId;

const VERSION: u64 = 1;

/// How far past `now` a stored timestamp may lie and still load. Anything
/// later came from a clock skewed forward or a damaged file, and would
/// otherwise read as "just used" until that date passed.
pub const FUTURE_TOLERANCE: SignedDuration = SignedDuration::from_mins(5);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchLog {
    last_launched: BTreeMap<BundleId, Timestamp>,
    baseline_at: Option<Timestamp>,
}

impl LaunchLog {
    pub fn last_launched(&self, bundle_id: &BundleId) -> Option<Timestamp> {
        self.last_launched.get(bundle_id).copied()
    }

    pub fn baseline_at(&self) -> Option<Timestamp> {
        self.baseline_at
    }

    pub fn set_baseline_at(&mut self, at: Timestamp) {
        self.baseline_at = Some(at);
    }

    pub fn len(&self) -> usize {
        self.last_launched.len()
    }

    /// Sets the app's entry to `at`, earlier or later than the one stored:
    /// the latest event wins, so an entry written under a clock that ran
    /// ahead heals on the next real event. Returns whether anything changed.
    pub fn record(&mut self, bundle_id: BundleId, at: Timestamp) -> bool {
        self.last_launched.insert(bundle_id, at) != Some(at)
    }
}

/// A file entry that `decode` left out, so the rest of the log could load.
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
    EmptyBundleId,
    UnparseableTimestamp,
    InFuture,
}

impl fmt::Display for DropReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyBundleId => "the bundle ID is empty",
            Self::UnparseableTimestamp => "the timestamp is not RFC 3339",
            Self::InFuture => "the timestamp is more than 5 minutes in the future",
        })
    }
}

#[derive(Debug)]
pub struct Decoded {
    pub log: LaunchLog,
    pub dropped: Vec<Dropped>,
}

/// The whole file is unreadable; nothing in it can be trusted.
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

/// Version 1 as stored. Timestamps stay strings until `decode` parses each
/// one on its own, so a bad entry is dropped instead of failing the file.
#[derive(Deserialize)]
struct FileV1 {
    baseline_at: Option<String>,
    last_launched: BTreeMap<String, String>,
}

/// Writes `log` as version 1 JSON, entries ordered by bundle ID.
pub fn encode(log: &LaunchLog) -> String {
    let entries: Map<String, Value> = log
        .last_launched
        .iter()
        .map(|(bundle_id, at)| (bundle_id.as_str().to_owned(), Value::String(at.to_string())))
        .collect();
    let file = json!({
        "version": VERSION,
        "baseline_at": log.baseline_at.map(|at| at.to_string()),
        "last_launched": entries,
    });
    format!("{file:#}\n")
}

/// Reads a launch log written by `encode`. Entries that cannot be used are
/// left out and listed in `Decoded::dropped`; `now` bounds how far in the
/// future a timestamp may be (`FUTURE_TOLERANCE`).
pub fn decode(text: &str, now: Timestamp) -> Result<Decoded, DecodeError> {
    let FileVersion { version } = serde_json::from_str(text)?;
    if version != VERSION {
        return Err(DecodeError::Version(version));
    }
    let file: FileV1 = serde_json::from_str(text)?;
    let latest = now.checked_add(FUTURE_TOLERANCE).unwrap_or(Timestamp::MAX);
    let mut dropped = Vec::new();
    let mut log = LaunchLog::default();
    for (key, value) in file.last_launched {
        let entry = BundleId::new(key.as_str())
            .ok_or(DropReason::EmptyBundleId)
            .and_then(|bundle_id| Ok((bundle_id, timestamp(&value, latest)?)));
        match entry {
            Ok((bundle_id, at)) => {
                log.last_launched.insert(bundle_id, at);
            }
            Err(reason) => dropped.push(Dropped::Entry {
                bundle_id: key,
                reason,
            }),
        }
    }
    if let Some(value) = file.baseline_at {
        match timestamp(&value, latest) {
            Ok(at) => log.baseline_at = Some(at),
            Err(reason) => dropped.push(Dropped::Baseline { reason }),
        }
    }
    Ok(Decoded { log, dropped })
}

fn timestamp(text: &str, latest: Timestamp) -> Result<Timestamp, DropReason> {
    let at: Timestamp = text.parse().map_err(|_| DropReason::UnparseableTimestamp)?;
    if at > latest {
        return Err(DropReason::InFuture);
    }
    Ok(at)
}

#[cfg(test)]
mod tests;
