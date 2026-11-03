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

    pub fn replace_if_changed(&self, reading: Trust) -> Option<Trust> {
        let previous = self.trusted.swap(reading.is_trusted(), Ordering::Relaxed);
        (previous != reading.is_trusted()).then_some(reading)
    }
}

#[cfg(test)]
mod tests;
