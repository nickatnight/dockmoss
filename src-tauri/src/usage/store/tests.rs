use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use jiff::Timestamp;

use super::{MAX_FILE_BYTES, load_or_set_aside, save};
use crate::bundle_id::BundleId;
use crate::usage::LaunchLog;

fn at(text: &str) -> Timestamp {
    text.parse().expect("test timestamps are valid RFC 3339")
}

fn id(text: &str) -> BundleId {
    BundleId::new(text).expect("non-empty bundle id")
}

const NOW: &str = "2026-11-02T12:00:00Z";

fn log_path(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("launch_log.json")
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn two_apps() -> LaunchLog {
    let mut log = LaunchLog::default();
    log.record(id("com.apple.Safari"), at("2026-11-02T09:14:03Z"));
    log.record(id("com.apple.TextEdit"), at("2026-11-01T18:00:00Z"));
    log
}

#[test]
fn a_missing_file_loads_as_an_empty_log() {
    let dir = tempfile::tempdir().expect("temp dir");

    let log = load_or_set_aside(&log_path(&dir), at(NOW));

    assert_eq!(log, LaunchLog::default());
    assert!(!log_path(&dir).exists(), "loading creates nothing");
}

#[test]
fn save_then_load_round_trips() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = log_path(&dir);

    save(&path, &two_apps()).expect("save succeeds");

    assert_eq!(load_or_set_aside(&path, at(NOW)), two_apps());
}

#[test]
fn a_corrupt_file_loads_empty_and_is_kept_as_corrupt() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = log_path(&dir);
    fs::write(&path, "not json").expect("write corrupt file");
    fs::write(sibling(&path, ".corrupt"), "an older corrupt file").expect("write old corrupt");

    let log = load_or_set_aside(&path, at(NOW));

    assert_eq!(log, LaunchLog::default());
    assert_eq!(
        fs::read(sibling(&path, ".corrupt")).expect("corrupt copy exists"),
        b"not json",
        "the newest corrupt file replaces an older one"
    );
    assert!(!path.exists(), "the corrupt file is moved, not copied");
}

#[test]
fn a_file_with_some_bad_entries_loads_the_rest_in_place() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = log_path(&dir);
    let text = r#"{"version": 1, "baseline_at": "2027-01-01T00:00:00Z", "last_launched": {
        "com.apple.Safari": "2026-11-02T09:14:03Z",
        "com.apple.Notes": "not a time"
    }}"#;
    fs::write(&path, text).expect("write file");

    let log = load_or_set_aside(&path, at(NOW));

    assert_eq!(log.len(), 1);
    assert_eq!(
        log.last_launched(&id("com.apple.Safari")),
        Some(at("2026-11-02T09:14:03Z"))
    );
    assert_eq!(log.baseline_at(), None, "a future baseline is dropped");
    assert!(path.exists(), "a readable file stays where it is");
    assert!(!sibling(&path, ".corrupt").exists());
}

fn padded_log(len: usize) -> Vec<u8> {
    let mut bytes = super::encode(&two_apps()).into_bytes();
    bytes.resize(len, b' ');
    bytes
}

#[test]
fn a_file_over_1_mib_is_treated_as_corrupt() {
    let cases = [
        ("exactly 1 MiB", MAX_FILE_BYTES, true),
        ("1 MiB + 1 byte", MAX_FILE_BYTES + 1, false),
    ];
    for (case, len, loads) in cases {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = log_path(&dir);
        let bytes = padded_log(usize::try_from(len).expect("1 MiB fits in usize"));
        fs::write(&path, &bytes).expect("write padded file");

        let log = load_or_set_aside(&path, at(NOW));

        if loads {
            assert_eq!(log, two_apps(), "{case}");
            assert!(!sibling(&path, ".corrupt").exists(), "{case}");
        } else {
            assert_eq!(log, LaunchLog::default(), "{case}");
            assert_eq!(
                fs::read(sibling(&path, ".corrupt")).expect("corrupt copy exists"),
                bytes,
                "{case}"
            );
        }
    }
}

#[test]
fn a_stale_temp_file_is_replaced_and_the_saved_file_is_private() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = log_path(&dir);
    let tmp = sibling(&path, ".tmp");
    fs::write(&tmp, "half a log from a crash").expect("write stale tmp");
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o644)).expect("chmod stale tmp");

    save(&path, &two_apps()).expect("save succeeds");

    assert_eq!(load_or_set_aside(&path, at(NOW)), two_apps());
    assert!(!tmp.exists(), "the temp file is renamed into place");
    let mode = fs::metadata(&path)
        .expect("saved file")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "mode {mode:o}");
}

#[test]
fn a_new_saved_file_has_mode_0600() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = log_path(&dir);

    save(&path, &two_apps()).expect("save succeeds");

    let mode = fs::metadata(&path)
        .expect("saved file")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "mode {mode:o}");
}

#[test]
fn a_failed_save_leaves_the_previous_file_unchanged() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = log_path(&dir);
    save(&path, &two_apps()).expect("first save succeeds");
    let before = fs::read(&path).expect("saved file");
    fs::create_dir(sibling(&path, ".tmp")).expect("block the temp path with a directory");
    let mut changed = two_apps();
    changed.record(id("com.apple.Notes"), at("2026-11-02T11:00:00Z"));

    let result = save(&path, &changed);

    assert!(result.is_err());
    assert_eq!(fs::read(&path).expect("previous file"), before);
}

#[test]
fn save_creates_a_missing_parent_directory() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("com.dockmoss.app/nested/launch_log.json");

    save(&path, &two_apps()).expect("save succeeds");

    assert_eq!(load_or_set_aside(&path, at(NOW)), two_apps());
}
