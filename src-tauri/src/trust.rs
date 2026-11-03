use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    Trusted,
    Untrusted,
}

impl Trust {
    pub fn is_trusted(self) -> bool {
        self == Self::Trusted
    }

    pub fn from_trusted(trusted: bool) -> Self {
        if trusted {
            Self::Trusted
        } else {
            Self::Untrusted
        }
    }
}

/// Read by the tray now, and by the Dock tracker from job 004.
pub struct TrustState {
    trusted: AtomicBool,
}

impl TrustState {
    pub fn new(trust: Trust) -> Self {
        Self {
            trusted: AtomicBool::new(trust.is_trusted()),
        }
    }

    pub fn current(&self) -> Trust {
        Trust::from_trusted(self.trusted.load(Ordering::Relaxed))
    }

    /// Stores `reading` and returns the reading it replaced. Only the
    /// `ax-trust` watcher calls it.
    pub fn replace(&self, reading: Trust) -> Trust {
        Trust::from_trusted(self.trusted.swap(reading.is_trusted(), Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests;
