use jiff::Timestamp;

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
