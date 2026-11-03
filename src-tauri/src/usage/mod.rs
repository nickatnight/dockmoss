use std::io;
use std::path::PathBuf;

mod log;
mod running;
mod schedule;
mod store;

pub use log::LaunchLog;

#[derive(Debug, thiserror::Error)]
pub enum UsageError {
    #[error("could not {action} {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}
