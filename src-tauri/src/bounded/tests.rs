use std::time::{Duration, Instant};

use super::read_within;

const BOUND: Duration = Duration::from_millis(100);

#[test]
fn a_read_that_answers_in_time_gives_its_answer() {
    assert_eq!(read_within(BOUND, || 42), Some(42));
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
