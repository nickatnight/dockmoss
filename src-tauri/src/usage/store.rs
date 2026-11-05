use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use jiff::Timestamp;

use super::UsageError;
use super::log::{DecodeError, Decoded, Dropped, LaunchLog, decode, encode};

pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
const FILE_MODE: u32 = 0o600;

#[derive(Debug, thiserror::Error)]
enum Unreadable {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("the file is larger than {MAX_FILE_BYTES} bytes")]
    TooLarge,
    #[error(transparent)]
    Decode(#[from] DecodeError),
}

pub fn load_or_set_aside(path: &Path, now: Timestamp) -> LaunchLog {
    let text = match read_capped(path) {
        Ok(None) => {
            tracing::info!(path = %path.display(), "no launch log yet; starting empty");
            return LaunchLog::default();
        }
        Ok(Some(text)) => text,
        Err(error) => {
            set_aside(path, &error);
            return LaunchLog::default();
        }
    };
    match decode(&text, now) {
        Ok(Decoded { log, dropped }) => {
            if !dropped.is_empty() {
                dropped.into_iter().for_each(warn_dropped);
                keep_as_read(path, text.as_bytes());
            }
            log
        }
        Err(error) => {
            set_aside(path, &error.into());
            LaunchLog::default()
        }
    }
}

fn keep_as_read(path: &Path, bytes: &[u8]) {
    let copy = sibling(path, ".dropped");
    match write_private(&copy, bytes) {
        Ok(()) => tracing::warn!(copy = %copy.display(), "launch log as read kept"),
        Err(error) => tracing::warn!(%error, "launch log as read not kept"),
    }
}

fn warn_dropped(entry: Dropped) {
    match entry {
        Dropped::Entry { bundle_id, reason } => {
            tracing::warn!(bundle_id, %reason, "launch log entry dropped");
        }
        Dropped::Baseline { reason } => tracing::warn!(%reason, "launch log baseline dropped"),
    }
}

fn read_capped(path: &Path) -> Result<Option<String>, Unreadable> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut text = String::new();
    file.take(MAX_FILE_BYTES + 1).read_to_string(&mut text)?;
    if u64::try_from(text.len()).map_or(true, |len| len > MAX_FILE_BYTES) {
        return Err(Unreadable::TooLarge);
    }
    Ok(Some(text))
}

fn set_aside(path: &Path, error: &Unreadable) {
    let corrupt = sibling(path, ".corrupt");
    match fs::rename(path, &corrupt) {
        Ok(()) => tracing::warn!(
            path = %path.display(),
            moved_to = %corrupt.display(),
            %error,
            "launch log unreadable; moved aside, starting empty"
        ),
        Err(move_error) => tracing::warn!(
            path = %path.display(),
            %error,
            %move_error,
            "launch log unreadable and not moved aside; starting empty"
        ),
    }
}

pub fn save(path: &Path, log: &LaunchLog) -> Result<(), UsageError> {
    write_atomically(path, encode(log).as_bytes())
}

/// Replaces the file at `path` with `bytes`, mode 0600: a private temp file
/// beside it, then `rename`. A crash leaves the old file or the new one.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), UsageError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error("create the directory", parent))?;
    }
    let tmp = sibling(path, ".tmp");
    write_private(&tmp, bytes)?;
    fs::rename(&tmp, path).map_err(io_error("rename the temp file over", path))
}

/// Writes `bytes` to a new file at `path` with `FILE_MODE`. A file left
/// there (by a crash, or an earlier run) is removed, not reopened, so
/// `create_new` follows no existing link and keeps no old mode.
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), UsageError> {
    remove_stale(path)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(FILE_MODE)
        .open(path)
        .map_err(io_error("create", path))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(io_error("write", path))
}

fn remove_stale(path: &Path) -> Result<(), UsageError> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            Err(io_error("remove the stale file", path)(error))
        }
        _ => Ok(()),
    }
}

fn io_error(action: &'static str, path: &Path) -> impl FnOnce(io::Error) -> UsageError {
    let path = path.to_path_buf();
    move |source| UsageError::Io {
        action,
        path,
        source,
    }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests;
