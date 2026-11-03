//! When the launch-log writer saves. Pure: every instant is passed in.

use std::time::{Duration, Instant};

/// Changes within this window of the first one are written together. It
/// leaves room for the write itself inside the 1 s bound for a change to
/// reach the disk.
const SAVE_DELAY: Duration = Duration::from_millis(500);
/// How long a failed save (a full disk, a permissions error) waits before it
/// is tried again, so the writer does not spin.
const RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
pub struct SaveSchedule {
    deadline: Option<Instant>,
}

impl SaveSchedule {
    /// Schedules a save `SAVE_DELAY` after the first unsaved change. Later
    /// changes keep that deadline, so a burst of app switches cannot hold the
    /// save off.
    pub fn changed(&mut self, now: Instant) {
        if self.deadline.is_none() {
            self.deadline = Some(after(now, SAVE_DELAY));
        }
    }

    /// When the pending save is due, or `None` when nothing is unsaved.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub fn is_due(&self, now: Instant) -> bool {
        self.deadline.is_some_and(|deadline| deadline <= now)
    }

    pub fn saved(&mut self) {
        self.deadline = None;
    }

    /// Keeps the change pending and retries `RETRY_DELAY` after `now`.
    pub fn save_failed(&mut self, now: Instant) {
        self.deadline = Some(after(now, RETRY_DELAY));
    }
}

/// `now + delay`, or `now` if that overflows, so a save is never lost to an
/// unrepresentable deadline.
fn after(now: Instant, delay: Duration) -> Instant {
    now.checked_add(delay).unwrap_or(now)
}

#[cfg(test)]
mod tests;
