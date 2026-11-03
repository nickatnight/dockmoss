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
    let decoded = match read_capped(path) {
        Ok(None) => {
            tracing::info!(path = %path.display(), "no launch log yet; starting empty");
            return LaunchLog::default();
        }
        Ok(Some(text)) => decode(&text, now).map_err(Unreadable::from),
        Err(error) => Err(error),
    };
    match decoded {
        Ok(Decoded { log, dropped }) => {
            for entry in dropped {
                match entry {
                    Dropped::Entry { bundle_id, reason } => {
                        tracing::warn!(bundle_id, %reason, "launch log entry dropped");
                    }
                    Dropped::Baseline { reason } => {
                        tracing::warn!(%reason, "launch log baseline dropped");
                    }
                }
            }
            log
        }
        Err(error) => {
            set_aside(path, &error);
            LaunchLog::default()
        }
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
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error("create the directory", parent))?;
    }
    let tmp = sibling(path, ".tmp");
    remove_stale_temp(&tmp)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(FILE_MODE)
        .open(&tmp)
        .map_err(io_error("create", &tmp))?;
    file.write_all(encode(log).as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(io_error("write", &tmp))?;
    fs::rename(&tmp, path).map_err(io_error("rename the temp file over", path))
}

/// A temp file left by a crash is removed, not reopened, so `create_new`
/// makes the new one with `FILE_MODE` and follows no existing link.
fn remove_stale_temp(tmp: &Path) -> Result<(), UsageError> {
    match fs::remove_file(tmp) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            Err(io_error("remove the stale temp file", tmp)(error))
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
