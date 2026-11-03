use std::io;
use std::path::PathBuf;

mod log;
mod running;
mod schedule;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the usage thread seeds the first run once slice 8 wires it"
    )
)]
mod spotlight;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the usage thread holds a UsageState once slice 8 wires it"
    )
)]
mod state;
mod store;

pub use log::{LaunchLog, Recorded};
pub use running::{AppEvent, AppEventKind, RunningApp, tracked_bundle_id};
pub use schedule::SaveSchedule;
pub use store::{load_or_set_aside, save};

#[derive(Debug, thiserror::Error)]
pub enum UsageError {
    #[error("could not {action} {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}
