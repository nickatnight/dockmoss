use jiff::Timestamp;

/// What is known about when an app was last used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastUsedSources {
    /// The latest launch in our own launch log.
    pub launch_log: Option<Timestamp>,
    /// The `kMDItemLastUsedDate` seed, already judged sane by the caller.
    pub spotlight: Option<Timestamp>,
    /// The first-run grace baseline.
    pub baseline: Option<Timestamp>,
    /// The app is running now.
    pub running: bool,
}

/// When the app counts as last used: `now` while it runs, otherwise the
/// latest source present, never later than `now`. `None` means nothing is
/// known, and the app stays clean.
pub fn effective_last_used(now: Timestamp, sources: &LastUsedSources) -> Option<Timestamp> {
    if sources.running {
        return Some(now);
    }
    [sources.launch_log, sources.spotlight, sources.baseline]
        .into_iter()
        .flatten()
        .max()
        .map(|latest| latest.min(now))
}

#[cfg(test)]
mod tests;
