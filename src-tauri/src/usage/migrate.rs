use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::store::private_dir;

const OLD_IDENTIFIER: &str = "com.dockmoss.app";

pub const MOVED_FILES: [&str; 4] = [
    "launch_log.json",
    "settings.json",
    "launch_log.json.corrupt",
    "launch_log.json.dropped",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    Missing,
    File,
    Other,
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
        Ok(_) | Err(_) => Entry::Other,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Move(&'static str),
    KeepNew(&'static str),
    LeaveNotAFile(&'static str),
}

pub fn plan_move(old: &Listing, new: &Listing) -> Vec<Step> {
    MOVED_FILES
        .into_iter()
        .filter_map(|name| match (old.get(name), new.get(name)) {
            (Entry::Missing, _) => None,
            (Entry::Other, _) => Some(Step::LeaveNotAFile(name)),
            (Entry::File, Entry::Missing) => Some(Step::Move(name)),
            (Entry::File, Entry::File | Entry::Other) => Some(Step::KeepNew(name)),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MoveReport {
    pub moved: usize,
    pub kept: usize,
    pub skipped: usize,
    pub failed: usize,
    pub old_folder: OldFolder,
}

impl MoveReport {
    pub fn log(&self) {
        let Self {
            moved,
            kept,
            skipped,
            failed,
            old_folder,
        } = *self;
        if moved + kept + skipped + failed > 0 || old_folder == OldFolder::Removed {
            tracing::info!(
                moved,
                kept,
                skipped,
                failed,
                ?old_folder,
                "data from the old app folder moved"
            );
        } else {
            tracing::debug!(?old_folder, "no data in the old app folder to move");
        }
    }
}

/// The old folder is the new one's sibling. `None` when they are the same.
pub fn old_data_dir(new_dir: &Path) -> Option<PathBuf> {
    let old = new_dir.parent()?.join(OLD_IDENTIFIER);
    (old != new_dir).then_some(old)
}

/// Creates `new_dir` (0700) and moves each of `MOVED_FILES` that is a
/// regular file in `old_dir` and absent from `new_dir`, with `rename`. Then
/// removes `old_dir` if it is empty. Logs a `warn` naming each file left
/// behind, never a path. Running it again finds nothing left to do.
pub fn move_old_data(old_dir: &Path, new_dir: &Path) -> MoveReport {
    if let Err(error) = private_dir(new_dir) {
        tracing::warn!(kind = %error.kind(), "app data folder not created; old data not moved");
        return MoveReport::default();
    }
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
            Step::Move(name) => match fs::rename(old_dir.join(name), new_dir.join(name)) {
                Ok(()) => report.moved += 1,
                Err(error) => {
                    report.failed += 1;
                    tracing::warn!(file = name, kind = %error.kind(), "old data file not moved; left in the old folder");
                }
            },
            Step::KeepNew(name) => {
                report.kept += 1;
                tracing::warn!(
                    file = name,
                    "the new app folder already has this file; the old one is left"
                );
            }
            Step::LeaveNotAFile(name) => {
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

#[cfg(test)]
mod tests;
