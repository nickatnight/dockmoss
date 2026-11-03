use jiff::Timestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastUsedSources {
    pub launch_log: Option<Timestamp>,
    pub spotlight: Option<Timestamp>,
    pub baseline: Option<Timestamp>,
    pub running: bool,
}

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
