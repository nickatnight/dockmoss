use std::io;
use std::path::PathBuf;

mod log;
mod migrate;
mod running;
mod schedule;
mod spotlight;
mod state;
mod store;

pub use log::LaunchLog;
pub use migrate::{move_old_data, old_data_dir};
pub use running::{AppEvent, AppEventKind, RunningApp, tracked_bundle_id};
pub use schedule::{SaveSchedule, recompute_interval};
pub use spotlight::parse_mdls;
pub use state::{AppGrowth, Change, DistinctDockApps, DockApp, DockUpdate, UsageState};
pub use store::{load_or_set_aside, read_capped, save, write_atomically};

#[derive(Debug, thiserror::Error)]
pub enum UsageError {
    #[error("could not {action} {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}
