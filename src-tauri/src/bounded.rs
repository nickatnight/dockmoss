use std::sync::atomic::{AtomicBool, Ordering};
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

/// `read_within`, one read per `slot` at a time. While an earlier read
/// still runs, even one its caller stopped waiting for, this gives `None` at
/// once and starts no read, so a read that never returns leaves one thread
/// behind, not one per call.
pub fn read_alone<T: Send + 'static>(
    slot: &'static AtomicBool,
    timeout: Duration,
    read: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let Some(claim) = Claim::take(slot) else {
        tracing::debug!("a bounded read is still running; not starting another");
        return None;
    };
    read_within(timeout, move || {
        let _held = claim;
        read()
    })
}

/// Holds a slot until dropped: when its read returns, or when the read's
/// thread does not start and the read is dropped unrun.
struct Claim(&'static AtomicBool);

impl Claim {
    fn take(slot: &'static AtomicBool) -> Option<Self> {
        slot.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
            .then_some(Self(slot))
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use super::{read_alone, read_within};

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

    #[test]
    fn reads_that_answer_in_time_each_run_in_turn() {
        static SLOT: AtomicBool = AtomicBool::new(false);

        assert_eq!(read_alone(&SLOT, BOUND, || 1), Some(1));
        assert_eq!(read_alone(&SLOT, BOUND, || 2), Some(2));
    }

    #[test]
    fn while_a_late_read_still_runs_another_call_gives_none_at_once_and_starts_no_read() {
        static SLOT: AtomicBool = AtomicBool::new(false);
        static SECOND_RAN: AtomicBool = AtomicBool::new(false);
        assert_eq!(
            read_alone(&SLOT, BOUND, || std::thread::sleep(BOUND * 3)),
            None
        );

        let started = Instant::now();
        let second = read_alone(&SLOT, BOUND, || SECOND_RAN.store(true, Ordering::SeqCst));

        assert_eq!(second, None);
        assert!(started.elapsed() < BOUND / 2, "{:?}", started.elapsed());
        std::thread::sleep(BOUND * 3);
        assert!(!SECOND_RAN.load(Ordering::SeqCst), "the second read ran");
    }

    #[test]
    fn once_the_late_read_finishes_the_next_call_reads_again() {
        static SLOT: AtomicBool = AtomicBool::new(false);
        assert_eq!(
            read_alone(&SLOT, BOUND, || std::thread::sleep(BOUND * 2)),
            None
        );

        std::thread::sleep(BOUND * 2);

        assert_eq!(read_alone(&SLOT, BOUND, || 3), Some(3));
    }
}
