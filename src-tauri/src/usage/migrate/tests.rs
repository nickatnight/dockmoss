use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

use super::{Entry, Listing, MOVED_FILES, MoveReport, OldFolder, Step, move_old_data, plan_move};

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
