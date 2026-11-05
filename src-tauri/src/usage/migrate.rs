use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::store::private_dir;

const OLD_IDENTIFIER: &str = "com.dockmoss.app";

pub const LAUNCH_LOG_FILE: &str = "launch_log.json";
pub const SETTINGS_FILE: &str = "settings.json";

/// `lib.rs` asks for the first two by these same constants, so a renamed
/// file cannot stop being recognised as left behind.
pub const MOVED_FILES: [&str; 4] = [
    LAUNCH_LOG_FILE,
    SETTINGS_FILE,
    "launch_log.json.corrupt",
    "launch_log.json.dropped",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    Missing,
    File,
    NotARegularFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Listing(BTreeMap<&'static str, Entry>);

impl Listing {
    pub fn from_entries(entries: impl IntoIterator<Item = (&'static str, Entry)>) -> Self {
        Self(entries.into_iter().collect())
    }

    fn read(dir: &Path) -> Self {
        Self::from_entries(MOVED_FILES.map(|name| (name, entry_at(&dir.join(name)))))
    }

    fn get(&self, name: &str) -> Entry {
        self.0.get(name).copied().unwrap_or(Entry::Missing)
    }
}

fn entry_at(path: &Path) -> Entry {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Entry::File,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Entry::Missing,
        Ok(_) | Err(_) => Entry::NotARegularFile,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Move(&'static str),
    KeepNew(&'static str),
    LeaveNotARegularFile(&'static str),
}

pub fn plan_move(old: &Listing, new: &Listing) -> Vec<Step> {
    MOVED_FILES
        .into_iter()
        .filter_map(|name| match (old.get(name), new.get(name)) {
            (Entry::Missing, _) => None,
            (Entry::NotARegularFile, _) => Some(Step::LeaveNotARegularFile(name)),
            (Entry::File, Entry::Missing) => Some(Step::Move(name)),
            (Entry::File, Entry::File | Entry::NotARegularFile) => Some(Step::KeepNew(name)),
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OldFolder {
    #[default]
    Absent,
    Removed,
    Left,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MoveReport {
    pub moved: usize,
    pub kept: usize,
    pub skipped: usize,
    /// Regular files that stayed in the old folder because their move failed.
    pub failed: Vec<&'static str>,
    pub old_folder: OldFolder,
}

impl MoveReport {
    /// The file is still in the old folder and a later start may move it, so
    /// nothing may write a new one in its place this session: the new one
    /// would be kept, and the old one never read again.
    fn left_behind(&self, name: &str) -> bool {
        self.failed.contains(&name)
    }

    /// `dir/name`, or `None` when the move left `name` in the old folder:
    /// writing a new one in `dir` would strand the old one for good, so the
    /// session goes without it and the next start moves it.
    pub fn file_in(&self, dir: &Path, name: &str) -> Option<PathBuf> {
        (!self.left_behind(name)).then(|| dir.join(name))
    }

    /// `info` when this start moved, found or removed something; a start
    /// with nothing left to do logs only at `debug`.
    pub fn worth_reporting(&self) -> bool {
        self.moved + self.kept + self.skipped + self.failed.len() > 0
            || self.old_folder == OldFolder::Removed
    }

    pub fn log(&self) {
        if self.worth_reporting() {
            tracing::info!(
                moved = self.moved,
                kept = self.kept,
                skipped = self.skipped,
                failed = self.failed.len(),
                old_folder = ?self.old_folder,
                "data from the old app folder moved"
            );
        } else {
            tracing::debug!(old_folder = ?self.old_folder, "no data in the old app folder to move");
        }
    }
}

/// The old folder is the new one's sibling. `None` when they are the same.
pub fn old_data_dir(new_dir: &Path) -> Option<PathBuf> {
    let old = new_dir.parent()?.join(OLD_IDENTIFIER);
    (old != new_dir).then_some(old)
}

/// Creates `new_dir` (0700) and moves each of `MOVED_FILES` that is a
/// regular file in `old_dir` and absent from `new_dir`. Then removes
/// `old_dir` if it is empty. Logs a `warn` naming each file left behind,
/// never a path. Running it again finds nothing left to do.
pub fn move_old_data(old_dir: &Path, new_dir: &Path) -> MoveReport {
    let new_dir_ready = private_dir(new_dir)
        .inspect_err(|error| {
            tracing::warn!(kind = %error.kind(), "app data folder not made private");
        })
        .is_ok();
    match fs::symlink_metadata(old_dir) {
        Ok(metadata) if metadata.file_type().is_dir() => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return MoveReport::default(),
        Ok(_) | Err(_) => {
            tracing::warn!("the old app data folder is not a folder; nothing moved");
            return MoveReport {
                old_folder: OldFolder::Left,
                ..MoveReport::default()
            };
        }
    }
    let mut report = MoveReport::default();
    for step in plan_move(&Listing::read(old_dir), &Listing::read(new_dir)) {
        match step {
            Step::Move(name) if !new_dir_ready => {
                report.failed.push(name);
                tracing::warn!(
                    file = name,
                    "old data file not moved: the app data folder is not usable"
                );
            }
            Step::Move(name) => match move_file(&old_dir.join(name), &new_dir.join(name)) {
                Moved::Yes => report.moved += 1,
                Moved::OldCopyLeft(kind) => {
                    report.moved += 1;
                    tracing::warn!(file = name, %kind, "old data file moved; its old copy was not removed");
                }
                Moved::NameTaken => {
                    report.kept += 1;
                    tracing::info!(
                        file = name,
                        "the new app folder took this name first; the old file is left"
                    );
                }
                Moved::No(kind) => {
                    report.failed.push(name);
                    tracing::warn!(file = name, %kind, "old data file not moved; left in the old folder");
                }
            },
            Step::KeepNew(name) => {
                report.kept += 1;
                tracing::info!(
                    file = name,
                    "the new app folder already has an entry with this name; the old file is left"
                );
            }
            Step::LeaveNotARegularFile(name) => {
                report.skipped += 1;
                tracing::warn!(
                    file = name,
                    "old data entry is not a regular file; left in the old folder"
                );
            }
        }
    }
    report.old_folder = match fs::remove_dir(old_dir) {
        Ok(()) => OldFolder::Removed,
        Err(error) => {
            if error.kind() != io::ErrorKind::DirectoryNotEmpty {
                tracing::warn!(kind = %error.kind(), "old app data folder not removed");
            }
            OldFolder::Left
        }
    };
    report
}

enum Moved {
    Yes,
    /// The new name holds the data; the old name could not be removed, so the
    /// next start keeps the new file and leaves the old one.
    OldCopyLeft(io::ErrorKind),
    /// Something took the new name after the plan was made.
    NameTaken,
    No(io::ErrorKind),
}

/// `hard_link` fails when the new name exists, so nothing is ever
/// overwritten, as `rename` would.
fn move_file(old: &Path, new: &Path) -> Moved {
    match fs::hard_link(old, new) {
        Ok(()) => match fs::remove_file(old) {
            Ok(()) => Moved::Yes,
            Err(error) => Moved::OldCopyLeft(error.kind()),
        },
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Moved::NameTaken,
        Err(error) => Moved::No(error.kind()),
    }
}

#[cfg(test)]
mod tests;
