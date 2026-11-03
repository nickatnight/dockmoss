use std::time::{Duration, Instant};

use crate::growth::FastForward;

const SAVE_DELAY: Duration = Duration::from_millis(500);
const RETRY_DELAY: Duration = Duration::from_secs(5);
const RECOMPUTE_EVERY: Duration = Duration::from_mins(5);
const MIN_RECOMPUTE_EVERY: Duration = Duration::from_secs(1);

/// How often growth is recomputed with no usage or Dock change: every 5
/// minutes of curve time, but never more than once a second.
pub fn recompute_interval(fast_forward: FastForward) -> Duration {
    (RECOMPUTE_EVERY / fast_forward.factor()).max(MIN_RECOMPUTE_EVERY)
}

#[derive(Debug, Default)]
pub struct SaveSchedule {
    deadline: Option<Instant>,
}

impl SaveSchedule {
    pub fn changed(&mut self, now: Instant) {
        if self.deadline.is_none() {
            self.deadline = Some(after_or_now(now, SAVE_DELAY));
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub fn is_due(&self, now: Instant) -> bool {
        self.deadline.is_some_and(|deadline| deadline <= now)
    }

    pub fn saved(&mut self) {
        self.deadline = None;
    }

    pub fn save_failed(&mut self, now: Instant) {
        self.deadline = Some(after_or_now(now, RETRY_DELAY));
    }
}

fn after_or_now(now: Instant, delay: Duration) -> Instant {
    now.checked_add(delay).unwrap_or(now)
}

#[cfg(test)]
mod tests;
