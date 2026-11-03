use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use super::{last_used_date, stdout_within};

#[test]
fn a_missing_or_relative_path_reads_as_none() {
    assert_eq!(
        last_used_date(Path::new("/nonexistent/DockMoss Test.app")),
        None
    );
    assert_eq!(
        last_used_date(Path::new("System/Applications/TextEdit.app")),
        None
    );
}

#[test]
fn an_app_that_exists_reads_as_some_output() {
    let stdout = last_used_date(Path::new("/System/Applications/TextEdit.app"));

    assert!(stdout.is_some_and(|text| !text.is_empty()));
}

#[test]
fn a_child_past_the_timeout_is_killed_and_reads_as_none() {
    let mut sleep = Command::new("/bin/sleep");
    sleep.arg("10");
    let started = Instant::now();

    let stdout = stdout_within(sleep, Duration::from_millis(200));

    assert_eq!(stdout, None);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "returned after {:?}",
        started.elapsed()
    );
}

#[test]
fn a_child_that_exits_in_time_returns_its_stdout() {
    let mut echo = Command::new("/bin/echo");
    echo.arg("2026-11-03 06:45:44 +0000");

    let stdout = stdout_within(echo, Duration::from_secs(2));

    assert_eq!(stdout.as_deref(), Some("2026-11-03 06:45:44 +0000\n"));
}

#[test]
fn a_child_that_exits_non_zero_reads_as_none() {
    let stdout = stdout_within(Command::new("/usr/bin/false"), Duration::from_secs(2));

    assert_eq!(stdout, None);
}
