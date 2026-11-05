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
    assert_eq!(MOVED_FILES[0], crate::LAUNCH_LOG_FILE);
    assert_eq!(MOVED_FILES[1], crate::SETTINGS_FILE);
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
fn a_move_that_fails_leaves_the_file_in_the_old_folder() {
    let folders = folders();
    write(&folders.old, "launch_log.json", "log");
    fs::set_permissions(&folders.old, fs::Permissions::from_mode(0o500)).expect("chmod old");

    let report = move_old_data(&folders.old, &folders.new);

    fs::set_permissions(&folders.old, fs::Permissions::from_mode(0o700)).expect("chmod back");
    assert_eq!(
        report,
        MoveReport {
            failed: 1,
            old_folder: OldFolder::Left,
            ..MoveReport::default()
        }
    );
    assert_eq!(read(&folders.old, "launch_log.json"), "log");
    assert!(!folders.new.join("launch_log.json").exists());
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
