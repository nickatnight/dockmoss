use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MDLS: &str = "/usr/bin/mdls";
const TIMEOUT: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_STDOUT_BYTES: u64 = 4096;

/// Runs `mdls -raw -name kMDItemLastUsedDate <path>` and returns its stdout:
/// a date line or `(null)`. `None` for a relative path, a spawn error, a
/// non-zero exit (a missing path), or no exit within 2 s.
///
/// One process per app: with several paths `mdls` leaves no slot for a
/// missing one and mixes its errors into stdout.
pub fn last_used_date(path: &Path) -> Option<String> {
    // An absolute path starts with `/`, so `mdls` cannot read it as an option.
    if !path.is_absolute() {
        tracing::debug!(path = %path.display(), "spotlight read skipped: not an absolute path");
        return None;
    }
    let mut mdls = Command::new(MDLS);
    mdls.args(["-raw", "-name", "kMDItemLastUsedDate"])
        .arg(path);
    stdout_within(mdls, TIMEOUT)
}

fn stdout_within(mut command: Command, timeout: Duration) -> Option<String> {
    let mut child = match command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            tracing::warn!(%error, "spotlight read not started");
            return None;
        }
    };
    let deadline = Instant::now().checked_add(timeout);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if deadline.is_some_and(|deadline| Instant::now() < deadline) => {
                std::thread::sleep(POLL_INTERVAL);
            }
            Ok(None) => {
                tracing::warn!(?timeout, "spotlight read timed out; killing it");
                kill_and_reap(&mut child);
                return None;
            }
            Err(error) => {
                tracing::warn!(%error, "spotlight read not awaited; killing it");
                kill_and_reap(&mut child);
                return None;
            }
        }
    };
    if !status.success() {
        tracing::debug!(%status, "spotlight read exited with an error");
        return None;
    }
    let mut stdout = String::new();
    let read = child
        .stdout
        .take()?
        .take(MAX_STDOUT_BYTES)
        .read_to_string(&mut stdout);
    match read {
        Ok(_) => Some(stdout),
        Err(error) => {
            tracing::debug!(%error, "spotlight read output not readable");
            None
        }
    }
}

fn kill_and_reap(child: &mut std::process::Child) {
    if let Err(error) = child.kill() {
        tracing::warn!(%error, "spotlight read not killed");
    }
    if let Err(error) = child.wait() {
        tracing::warn!(%error, "spotlight read not reaped");
    }
}

#[cfg(test)]
mod tests {
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
}
