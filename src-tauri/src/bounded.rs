use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Runs `read` on a thread of its own and waits at most `timeout` for its
/// answer. `None` when it does not answer in time, or its thread cannot
/// start; a late read finishes on its own and its answer is dropped.
pub fn read_within<T: Send + 'static>(
    timeout: Duration,
    read: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (reply, replied) = mpsc::channel();
    thread::Builder::new()
        .name("bounded-read".into())
        .spawn(move || {
            if reply.send(read()).is_err() {
                tracing::debug!("a bounded read answered after its caller stopped waiting");
            }
        })
        .inspect_err(|error| tracing::warn!(%error, "bounded read not started"))
        .ok()?;
    replied.recv_timeout(timeout).ok()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::read_within;

    const BOUND: Duration = Duration::from_millis(100);

    #[test]
    fn a_read_that_answers_inside_the_bound_gives_its_answer() {
        let read = read_within(BOUND, || {
            std::thread::sleep(BOUND / 4);
            42
        });

        assert_eq!(read, Some(42));
    }

    #[test]
    fn a_read_that_hangs_gives_none_at_the_bound() {
        let started = Instant::now();

        let read = read_within(BOUND, || {
            std::thread::sleep(BOUND * 3);
            42
        });

        assert_eq!(read, None);
        assert!(started.elapsed() < BOUND * 2, "{:?}", started.elapsed());
    }
}
