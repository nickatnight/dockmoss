use std::io;
use std::path::PathBuf;

mod log;
mod running;
mod schedule;
mod store;

pub use log::{LaunchLog, Recorded};
pub use running::{AppEvent, AppEventKind, RunningApp, tracked_bundle_id};
pub use schedule::SaveSchedule;
pub use store::{load, save};

#[derive(Debug, thiserror::Error)]
pub enum UsageError {
    #[error("could not {action} {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}
