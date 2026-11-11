use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use super::store::private_dir;

const OLD_IDENTIFIER: &str = "com.dockmoss.app";
const PRIVATE_FILE_MODE: u32 = 0o600;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftBehind {
    UsageHistory,
    Settings,
    UsageHistoryAndSettings,
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
    /// would be kept, and the old one never read again. Only the launch log
    /// and the settings are asked about. A `.corrupt` or `.dropped` copy left
    /// behind is not guarded on purpose: a newer copy may be written in the
    /// new folder, and the old one then stays in the old folder.
    fn left_behind(&self, name: &str) -> bool {
        self.failed.contains(&name)
    }

    /// `dir/name`, or `None` when the move left `name` in the old folder:
    /// writing a new one in `dir` would strand the old one for good, so the
    /// session goes without it and the next start moves it.
    pub fn file_in(&self, dir: &Path, name: &str) -> Option<PathBuf> {
        (!self.left_behind(name)).then(|| dir.join(name))
    }

    pub fn left_behind_data(&self) -> Option<LeftBehind> {
        match (
            self.left_behind(LAUNCH_LOG_FILE),
            self.left_behind(SETTINGS_FILE),
        ) {
            (true, true) => Some(LeftBehind::UsageHistoryAndSettings),
            (true, false) => Some(LeftBehind::UsageHistory),
            (false, true) => Some(LeftBehind::Settings),
            (false, false) => None,
        }
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
                Moved::Copied => {
                    report.moved += 1;
                    tracing::info!(
                        file = name,
                        "old data file moved by copy: this volume makes no hard links"
                    );
                }
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

#[derive(Debug, PartialEq, Eq)]
enum Moved {
    Yes,
    Copied,
    /// The new name holds the data; the old name could not be removed, so the
    /// next start keeps the new file and leaves the old one.
    OldCopyLeft(io::ErrorKind),
    /// Something took the new name after the plan was made.
    NameTaken,
    No(io::ErrorKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkError {
    NameTaken,
    CannotLink,
    Other(io::ErrorKind),
}

/// std reports macOS's `ENOTSUP` as `Uncategorized`, not `Unsupported`
/// (rustc 1.99), so the raw code is checked too.
fn link_error(error: &io::Error) -> LinkError {
    if error.kind() == io::ErrorKind::AlreadyExists {
        LinkError::NameTaken
    } else if error.kind() == io::ErrorKind::Unsupported
        || matches!(error.raw_os_error(), Some(libc::ENOTSUP | libc::EOPNOTSUPP))
    {
        LinkError::CannotLink
    } else {
        LinkError::Other(error.kind())
    }
}

fn move_file(old: &Path, new: &Path) -> Moved {
    move_with(old, new, |old, new| fs::hard_link(old, new))
}

fn move_with(old: &Path, new: &Path, link: impl FnOnce(&Path, &Path) -> io::Result<()>) -> Moved {
    match link(old, new) {
        Ok(()) => match fs::remove_file(old) {
            Ok(()) => Moved::Yes,
            Err(error) => Moved::OldCopyLeft(error.kind()),
        },
        Err(error) => match link_error(&error) {
            LinkError::NameTaken => Moved::NameTaken,
            LinkError::Other(kind) => Moved::No(kind),
            LinkError::CannotLink => match copy_into_new_file(old, new) {
                Ok(()) => match fs::remove_file(old) {
                    Ok(()) => Moved::Copied,
                    Err(error) => Moved::OldCopyLeft(error.kind()),
                },
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Moved::NameTaken,
                Err(error) => Moved::No(error.kind()),
            },
        },
    }
}

/// `create_new` refuses a name that exists, so the copy never replaces one,
/// even a name made after the plan (macOS's exclusive rename,
/// `RENAME_EXCL`, is refused on exFAT too). A copy that fails removes the
/// file it made, so the next start retries the move.
fn copy_into_new_file(old: &Path, new: &Path) -> io::Result<()> {
    let mut source = fs::File::open(old)?;
    let mut target = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(PRIVATE_FILE_MODE)
        .open(new)?;
    let copied = io::copy(&mut source, &mut target).and_then(|_| target.sync_all());
    if copied.is_err()
        && let Err(error) = fs::remove_file(new)
    {
        tracing::warn!(kind = %error.kind(), "partial copy of an old data file not removed");
    }
    copied
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::Path;

    use std::io;

    use super::{
        Entry, LeftBehind, LinkError, Listing, MOVED_FILES, MoveReport, Moved, OldFolder, Step,
        link_error, move_old_data, move_with, plan_move,
    };

    fn listing(entries: &[(&'static str, Entry)]) -> Listing {
        Listing::from_entries(entries.iter().copied())
    }

    #[test]
    fn the_moved_names_are_the_four_user_data_files() {
        assert_eq!(
            MOVED_FILES,
            [
                "launch_log.json",
                "settings.json",
                "launch_log.json.corrupt",
                "launch_log.json.dropped",
            ]
        );
    }

    #[test]
    fn plan_move_moves_a_regular_file_only_into_an_empty_name() {
        let cases: [(&str, Listing, Listing, Vec<Step>); 9] = [
            (
                "nothing in either folder",
                listing(&[]),
                listing(&[]),
                vec![],
            ),
            (
                "the launch log alone",
                listing(&[("launch_log.json", Entry::File)]),
                listing(&[]),
                vec![Step::Move("launch_log.json")],
            ),
            (
                "settings alone",
                listing(&[("settings.json", Entry::File)]),
                listing(&[]),
                vec![Step::Move("settings.json")],
            ),
            (
                "the corrupt copy alone",
                listing(&[("launch_log.json.corrupt", Entry::File)]),
                listing(&[]),
                vec![Step::Move("launch_log.json.corrupt")],
            ),
            (
                "the dropped copy alone",
                listing(&[("launch_log.json.dropped", Entry::File)]),
                listing(&[]),
                vec![Step::Move("launch_log.json.dropped")],
            ),
            (
                "a file the new folder already has",
                listing(&[("launch_log.json", Entry::File)]),
                listing(&[("launch_log.json", Entry::File)]),
                vec![Step::KeepNew("launch_log.json")],
            ),
            (
                "a name the new folder holds as a link",
                listing(&[("settings.json", Entry::File)]),
                listing(&[("settings.json", Entry::NotARegularFile)]),
                vec![Step::KeepNew("settings.json")],
            ),
            (
                "a link or folder in the old folder",
                listing(&[("launch_log.json", Entry::NotARegularFile)]),
                listing(&[]),
                vec![Step::LeaveNotARegularFile("launch_log.json")],
            ),
            (
                "all four, in the listed order",
                listing(&[
                    ("launch_log.json.dropped", Entry::File),
                    ("settings.json", Entry::File),
                    ("launch_log.json", Entry::File),
                    ("launch_log.json.corrupt", Entry::File),
                ]),
                listing(&[]),
                vec![
                    Step::Move("launch_log.json"),
                    Step::Move("settings.json"),
                    Step::Move("launch_log.json.corrupt"),
                    Step::Move("launch_log.json.dropped"),
                ],
            ),
        ];
        for (case, old, new, expected) in cases {
            assert_eq!(plan_move(&old, &new), expected, "{case}");
        }
    }

    struct Folders {
        _root: tempfile::TempDir,
        old: std::path::PathBuf,
        new: std::path::PathBuf,
    }

    fn folders() -> Folders {
        let root = tempfile::tempdir().expect("temp dir");
        let old = root.path().join("com.dockmoss.app");
        let new = root.path().join("app.dockmoss.DockMoss");
        fs::create_dir(&old).expect("old folder");
        Folders {
            _root: root,
            old,
            new,
        }
    }

    fn write(dir: &Path, name: &str, text: &str) {
        fs::write(dir.join(name), text).expect("write a fixture");
    }

    fn read(dir: &Path, name: &str) -> String {
        fs::read_to_string(dir.join(name)).expect("read a fixture")
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).expect("metadata").permissions().mode() & 0o777
    }

    #[test]
    fn with_no_old_folder_nothing_moves_and_the_new_folder_is_created_private() {
        let root = tempfile::tempdir().expect("temp dir");
        let old = root.path().join("com.dockmoss.app");
        let new = root.path().join("app.dockmoss.DockMoss");

        let report = move_old_data(&old, &new);

        assert_eq!(report, MoveReport::default());
        assert_eq!(report.old_folder, OldFolder::Absent);
        assert_eq!(mode(&new), 0o700);
        assert!(!old.exists());
    }

    #[test]
    fn each_file_alone_moves_with_its_contents_and_the_empty_old_folder_goes() {
        for name in MOVED_FILES {
            let folders = folders();
            write(&folders.old, name, "made-up contents");

            let report = move_old_data(&folders.old, &folders.new);

            assert_eq!(
                report,
                MoveReport {
                    moved: 1,
                    old_folder: OldFolder::Removed,
                    ..MoveReport::default()
                },
                "{name}"
            );
            assert_eq!(read(&folders.new, name), "made-up contents", "{name}");
            assert!(!folders.old.exists(), "{name}");
        }
    }

    #[test]
    fn every_listed_file_moves_and_keeps_its_mode() {
        let folders = folders();
        for name in MOVED_FILES {
            write(&folders.old, name, name);
            fs::set_permissions(folders.old.join(name), fs::Permissions::from_mode(0o600))
                .expect("chmod");
        }

        let report = move_old_data(&folders.old, &folders.new);

        assert_eq!(report.moved, 4);
        assert_eq!(report.old_folder, OldFolder::Removed);
        for name in MOVED_FILES {
            assert_eq!(read(&folders.new, name), name);
            assert_eq!(mode(&folders.new.join(name)), 0o600, "{name}");
        }
    }

    #[test]
    fn a_file_already_in_the_new_folder_is_kept_and_the_old_one_left() {
        let folders = folders();
        write(&folders.old, "launch_log.json", "old");
        fs::create_dir(&folders.new).expect("new folder");
        write(&folders.new, "launch_log.json", "new");

        let report = move_old_data(&folders.old, &folders.new);

        assert_eq!(
            report,
            MoveReport {
                kept: 1,
                old_folder: OldFolder::Left,
                ..MoveReport::default()
            }
        );
        assert_eq!(read(&folders.new, "launch_log.json"), "new");
        assert_eq!(read(&folders.old, "launch_log.json"), "old");
    }

    #[test]
    fn a_symlink_is_left_where_it_is() {
        let folders = folders();
        let target = folders.old.join("elsewhere");
        fs::write(&target, "linked").expect("link target");
        symlink(&target, folders.old.join("settings.json")).expect("symlink");

        let report = move_old_data(&folders.old, &folders.new);

        assert_eq!(
            report,
            MoveReport {
                skipped: 1,
                old_folder: OldFolder::Left,
                ..MoveReport::default()
            }
        );
        let left = fs::symlink_metadata(folders.old.join("settings.json")).expect("still there");
        assert!(left.file_type().is_symlink());
        assert!(fs::symlink_metadata(folders.new.join("settings.json")).is_err());
    }

    #[test]
    fn a_temp_file_or_another_name_is_left_and_keeps_the_old_folder() {
        let folders = folders();
        write(&folders.old, "launch_log.json", "log");
        write(&folders.old, "launch_log.json.tmp", "half-written");
        write(&folders.old, "notes.txt", "someone else's");

        let report = move_old_data(&folders.old, &folders.new);

        assert_eq!(
            report,
            MoveReport {
                moved: 1,
                old_folder: OldFolder::Left,
                ..MoveReport::default()
            }
        );
        assert_eq!(read(&folders.new, "launch_log.json"), "log");
        assert_eq!(read(&folders.old, "launch_log.json.tmp"), "half-written");
        assert_eq!(read(&folders.old, "notes.txt"), "someone else's");
        assert!(!folders.new.join("launch_log.json.tmp").exists());
    }

    #[test]
    fn a_read_only_old_folder_still_gives_the_new_folder_the_file() {
        let folders = folders();
        write(&folders.old, "launch_log.json", "log");
        fs::set_permissions(&folders.old, fs::Permissions::from_mode(0o500)).expect("chmod old");

        let report = move_old_data(&folders.old, &folders.new);

        fs::set_permissions(&folders.old, fs::Permissions::from_mode(0o700)).expect("chmod back");
        assert_eq!(
            report,
            MoveReport {
                moved: 1,
                old_folder: OldFolder::Left,
                ..MoveReport::default()
            }
        );
        assert!(!report.left_behind("launch_log.json"));
        assert_eq!(read(&folders.new, "launch_log.json"), "log");
        assert_eq!(read(&folders.old, "launch_log.json"), "log");
    }

    /// Clears the immutable flag on its paths when dropped, even when a test
    /// fails, so `TempDir` can delete them. Declared after the folders, so it
    /// drops first.
    struct ClearsImmutable(Vec<std::path::PathBuf>);

    impl Drop for ClearsImmutable {
        fn drop(&mut self) {
            for path in &self.0 {
                if fs::symlink_metadata(path).is_ok() {
                    let cleared = std::process::Command::new("chflags")
                        .arg("nouchg")
                        .arg(path)
                        .status();
                    if !cleared.is_ok_and(|status| status.success()) {
                        eprintln!("could not clear the immutable flag on {}", path.display());
                    }
                }
            }
        }
    }

    /// `chflags uchg` makes a file immutable: macOS refuses to link or rename
    /// it, which is how a move fails here.
    fn set_immutable(path: &Path, immutable: bool) {
        let flag = if immutable { "uchg" } else { "nouchg" };
        let status = std::process::Command::new("chflags")
            .arg(flag)
            .arg(path)
            .status()
            .expect("chflags runs");
        assert!(status.success(), "chflags {flag}");
    }

    #[test]
    fn a_failed_move_leaves_the_file_behind_and_a_later_start_moves_it() {
        let folders = folders();
        write(&folders.old, "launch_log.json", "log");
        write(&folders.old, "settings.json", "settings");
        let _clears = ClearsImmutable(vec![
            folders.old.join("launch_log.json"),
            folders.new.join("launch_log.json"),
        ]);
        set_immutable(&folders.old.join("launch_log.json"), true);

        let first = move_old_data(&folders.old, &folders.new);

        set_immutable(&folders.old.join("launch_log.json"), false);
        assert_eq!(
            first,
            MoveReport {
                moved: 1,
                failed: vec!["launch_log.json"],
                old_folder: OldFolder::Left,
                ..MoveReport::default()
            }
        );
        assert!(first.left_behind("launch_log.json"));
        assert!(!first.left_behind("settings.json"));
        assert!(!folders.new.join("launch_log.json").exists());

        let retry = move_old_data(&folders.old, &folders.new);

        assert_eq!(
            retry,
            MoveReport {
                moved: 1,
                old_folder: OldFolder::Removed,
                ..MoveReport::default()
            }
        );
        assert!(!retry.left_behind("launch_log.json"));
        assert_eq!(read(&folders.new, "launch_log.json"), "log");
    }

    #[test]
    fn only_a_failed_move_leaves_a_file_behind() {
        let cases = [
            ("nothing happened", MoveReport::default(), false),
            (
                "moved",
                MoveReport {
                    moved: 1,
                    old_folder: OldFolder::Removed,
                    ..MoveReport::default()
                },
                false,
            ),
            (
                "kept: the new folder's file is the live one",
                MoveReport {
                    kept: 1,
                    old_folder: OldFolder::Left,
                    ..MoveReport::default()
                },
                false,
            ),
            (
                "skipped: a link never moves, so a retry cannot help",
                MoveReport {
                    skipped: 1,
                    old_folder: OldFolder::Left,
                    ..MoveReport::default()
                },
                false,
            ),
            (
                "another file failed",
                MoveReport {
                    failed: vec!["settings.json"],
                    old_folder: OldFolder::Left,
                    ..MoveReport::default()
                },
                false,
            ),
            (
                "the launch log failed",
                MoveReport {
                    failed: vec!["launch_log.json"],
                    old_folder: OldFolder::Left,
                    ..MoveReport::default()
                },
                true,
            ),
        ];
        for (case, report, expected) in cases {
            assert_eq!(report.left_behind("launch_log.json"), expected, "{case}");
        }
    }

    #[test]
    fn a_second_run_moves_nothing() {
        let folders = folders();
        write(&folders.old, "launch_log.json", "log");
        write(&folders.old, "settings.json", "settings");
        let first = move_old_data(&folders.old, &folders.new);

        let second = move_old_data(&folders.old, &folders.new);

        assert_eq!(first.moved, 2);
        assert_eq!(second, MoveReport::default());
        assert_eq!(read(&folders.new, "launch_log.json"), "log");
        assert_eq!(read(&folders.new, "settings.json"), "settings");
    }

    #[test]
    fn a_second_run_after_a_kept_file_changes_nothing_on_disk() {
        let folders = folders();
        write(&folders.old, "settings.json", "old");
        fs::create_dir(&folders.new).expect("new folder");
        write(&folders.new, "settings.json", "new");
        let first = move_old_data(&folders.old, &folders.new);

        let second = move_old_data(&folders.old, &folders.new);

        assert_eq!(first, second);
        assert_eq!(read(&folders.old, "settings.json"), "old");
        assert_eq!(read(&folders.new, "settings.json"), "new");
    }

    #[test]
    fn an_old_folder_that_is_a_link_is_not_followed() {
        let root = tempfile::tempdir().expect("temp dir");
        let real = root.path().join("real");
        fs::create_dir(&real).expect("real folder");
        write(&real, "launch_log.json", "log");
        let old = root.path().join("com.dockmoss.app");
        symlink(&real, &old).expect("symlink");
        let new = root.path().join("app.dockmoss.DockMoss");

        let report = move_old_data(&old, &new);

        assert_eq!(
            report,
            MoveReport {
                old_folder: OldFolder::Left,
                ..MoveReport::default()
            }
        );
        assert_eq!(read(&real, "launch_log.json"), "log");
        assert!(!new.join("launch_log.json").exists());
    }

    #[test]
    fn the_old_folder_is_the_sibling_named_by_the_old_identifier() {
        let new = Path::new("/Users/someone/Library/Application Support/app.dockmoss.DockMoss");

        assert_eq!(
            super::old_data_dir(new).as_deref(),
            Some(Path::new(
                "/Users/someone/Library/Application Support/com.dockmoss.app"
            ))
        );
        assert_eq!(
            super::old_data_dir(Path::new("/x/com.dockmoss.app")),
            None,
            "a new folder with the old name has nothing to move from"
        );
    }

    #[test]
    fn a_new_folder_that_is_a_link_gets_nothing_and_the_old_data_stays() {
        let folders = folders();
        write(&folders.old, "launch_log.json", "log");
        let elsewhere = folders.old.parent().expect("root").join("elsewhere");
        fs::create_dir(&elsewhere).expect("link target");
        symlink(&elsewhere, &folders.new).expect("symlink");

        let report = move_old_data(&folders.old, &folders.new);

        assert_eq!(
            report,
            MoveReport {
                failed: vec!["launch_log.json"],
                old_folder: OldFolder::Left,
                ..MoveReport::default()
            }
        );
        assert!(report.left_behind("launch_log.json"));
        assert_eq!(read(&folders.old, "launch_log.json"), "log");
        assert!(!elsewhere.join("launch_log.json").exists());
    }

    #[test]
    fn only_a_start_that_did_or_found_something_reports_at_info() {
        let cases = [
            ("no old folder", MoveReport::default(), false),
            (
                "a second start after a .tmp was left",
                MoveReport {
                    old_folder: OldFolder::Left,
                    ..MoveReport::default()
                },
                false,
            ),
            (
                "the empty old folder removed",
                MoveReport {
                    old_folder: OldFolder::Removed,
                    ..MoveReport::default()
                },
                true,
            ),
            (
                "moved",
                MoveReport {
                    moved: 1,
                    old_folder: OldFolder::Removed,
                    ..MoveReport::default()
                },
                true,
            ),
            (
                "moved, with a .tmp keeping the old folder",
                MoveReport {
                    moved: 1,
                    old_folder: OldFolder::Left,
                    ..MoveReport::default()
                },
                true,
            ),
            (
                "kept",
                MoveReport {
                    kept: 1,
                    old_folder: OldFolder::Left,
                    ..MoveReport::default()
                },
                true,
            ),
            (
                "failed",
                MoveReport {
                    failed: vec!["launch_log.json"],
                    old_folder: OldFolder::Left,
                    ..MoveReport::default()
                },
                true,
            ),
            (
                "skipped",
                MoveReport {
                    skipped: 1,
                    old_folder: OldFolder::Left,
                    ..MoveReport::default()
                },
                true,
            ),
        ];
        for (case, report, expected) in cases {
            assert_eq!(report.worth_reporting(), expected, "{case}");
        }
    }

    #[test]
    fn a_name_taken_after_the_plan_is_never_overwritten() {
        let folders = folders();
        fs::create_dir(&folders.new).expect("new folder");
        write(&folders.old, "launch_log.json", "old");
        write(&folders.new, "launch_log.json", "taken");

        let moved = super::move_file(
            &folders.old.join("launch_log.json"),
            &folders.new.join("launch_log.json"),
        );

        assert!(matches!(moved, super::Moved::NameTaken));
        assert_eq!(read(&folders.new, "launch_log.json"), "taken");
        assert_eq!(read(&folders.old, "launch_log.json"), "old");
    }

    #[test]
    fn a_file_left_behind_gets_no_path_in_the_new_folder() {
        let dir = Path::new("/data/app.dockmoss.DockMoss");
        let nothing_failed = MoveReport::default();
        let log_failed = MoveReport {
            failed: vec!["launch_log.json"],
            old_folder: OldFolder::Left,
            ..MoveReport::default()
        };
        let settings_failed = MoveReport {
            failed: vec!["settings.json"],
            old_folder: OldFolder::Left,
            ..MoveReport::default()
        };
        let cases = [
            (
                "nothing failed: the launch log",
                &nothing_failed,
                "launch_log.json",
                Some(dir.join("launch_log.json")),
            ),
            (
                "the launch log failed: the launch log",
                &log_failed,
                "launch_log.json",
                None,
            ),
            (
                "the launch log failed: settings",
                &log_failed,
                "settings.json",
                Some(dir.join("settings.json")),
            ),
            (
                "settings failed: settings",
                &settings_failed,
                "settings.json",
                None,
            ),
        ];
        for (case, report, name, expected) in cases {
            assert_eq!(report.file_in(dir, name), expected, "{case}");
        }
    }

    #[test]
    fn a_link_error_says_whether_the_name_is_taken_or_the_volume_cannot_link() {
        let cases = [
            (
                "the name exists",
                io::Error::from_raw_os_error(libc::EEXIST),
                LinkError::NameTaken,
            ),
            (
                "exFAT and other volumes without hard links: std calls ENOTSUP Uncategorized",
                io::Error::from_raw_os_error(libc::ENOTSUP),
                LinkError::CannotLink,
            ),
            (
                "the socket-style spelling of the same answer",
                io::Error::from_raw_os_error(libc::EOPNOTSUPP),
                LinkError::CannotLink,
            ),
            (
                "an error std already calls Unsupported",
                io::Error::from(io::ErrorKind::Unsupported),
                LinkError::CannotLink,
            ),
            (
                "an immutable (uchg) file",
                io::Error::from_raw_os_error(libc::EPERM),
                LinkError::Other(io::ErrorKind::PermissionDenied),
            ),
            (
                "another volume",
                io::Error::from_raw_os_error(libc::EXDEV),
                LinkError::Other(io::ErrorKind::CrossesDevices),
            ),
        ];
        for (case, error, expected) in cases {
            assert_eq!(link_error(&error), expected, "{case}");
        }
    }

    fn cannot_link(_old: &Path, _new: &Path) -> io::Result<()> {
        Err(io::Error::from_raw_os_error(libc::ENOTSUP))
    }

    #[test]
    fn on_a_volume_that_cannot_link_the_file_is_copied_into_an_empty_name_and_kept_private() {
        let folders = folders();
        fs::create_dir(&folders.new).expect("new folder");
        write(&folders.old, "launch_log.json", "history");

        let moved = move_with(
            &folders.old.join("launch_log.json"),
            &folders.new.join("launch_log.json"),
            cannot_link,
        );

        assert_eq!(moved, Moved::Copied);
        assert_eq!(read(&folders.new, "launch_log.json"), "history");
        assert_eq!(mode(&folders.new.join("launch_log.json")), 0o600);
        assert!(!folders.old.join("launch_log.json").exists());
    }

    /// Another process creates the new name while the move runs. The move
    /// may keep the old file or move it, but never replaces a file the
    /// racer made. The race window is microseconds wide, so the test runs
    /// the move many times; correct code can never fail it.
    #[test]
    fn on_a_volume_that_cannot_link_a_name_taken_during_the_move_is_never_replaced() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let mut replaced = 0;
        for _ in 0..RACE_RUNS {
            let folders = folders();
            fs::create_dir(&folders.new).expect("new folder");
            write(&folders.old, "launch_log.json", "old");
            let target = folders.new.join("launch_log.json");
            let stop = Arc::new(AtomicBool::new(false));
            let racer = {
                let (stop, target) = (Arc::clone(&stop), target.clone());
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        if let Ok(mut file) = fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&target)
                        {
                            std::io::Write::write_all(&mut file, b"racer").expect("racer writes");
                            return true;
                        }
                    }
                    false
                })
            };

            let moved = move_with(&folders.old.join("launch_log.json"), &target, cannot_link);
            stop.store(true, Ordering::Relaxed);
            let racer_made_it = racer.join().expect("racer thread");

            if racer_made_it
                && moved != Moved::NameTaken
                && read(&folders.new, "launch_log.json") == "old"
            {
                replaced += 1;
            }
        }
        assert_eq!(
            replaced, 0,
            "the move replaced the racer's file {replaced} times in {RACE_RUNS}"
        );
    }

    const RACE_RUNS: usize = 500;

    #[test]
    fn on_a_volume_that_cannot_link_a_taken_name_is_kept_and_never_overwritten() {
        let folders = folders();
        fs::create_dir(&folders.new).expect("new folder");
        write(&folders.old, "launch_log.json", "old");
        write(&folders.new, "launch_log.json", "taken");

        let moved = move_with(
            &folders.old.join("launch_log.json"),
            &folders.new.join("launch_log.json"),
            cannot_link,
        );

        assert_eq!(moved, Moved::NameTaken);
        assert_eq!(read(&folders.new, "launch_log.json"), "taken");
        assert_eq!(read(&folders.old, "launch_log.json"), "old");
    }

    #[test]
    fn the_user_hears_about_the_launch_log_or_settings_left_behind_only() {
        let report = |failed: Vec<&'static str>| MoveReport {
            failed,
            old_folder: OldFolder::Left,
            ..MoveReport::default()
        };
        let cases = [
            ("nothing left behind", MoveReport::default(), None),
            (
                "the launch log",
                report(vec!["launch_log.json"]),
                Some(LeftBehind::UsageHistory),
            ),
            (
                "the settings",
                report(vec!["settings.json"]),
                Some(LeftBehind::Settings),
            ),
            (
                "both",
                report(vec!["launch_log.json", "settings.json"]),
                Some(LeftBehind::UsageHistoryAndSettings),
            ),
            (
                "only copies that are never read back",
                report(vec!["launch_log.json.corrupt", "launch_log.json.dropped"]),
                None,
            ),
        ];
        for (case, report, expected) in cases {
            assert_eq!(report.left_behind_data(), expected, "{case}");
        }
    }
}
