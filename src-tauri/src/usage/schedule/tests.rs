use std::time::{Duration, Instant};

use super::SaveSchedule;

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

#[test]
fn nothing_is_pending_at_first() {
    let schedule = SaveSchedule::default();

    assert_eq!(schedule.deadline(), None);
    assert!(!schedule.is_due(Instant::now()));
}

#[test]
fn the_first_change_saves_500_ms_later() {
    let t0 = Instant::now();
    let mut schedule = SaveSchedule::default();

    schedule.changed(t0);

    assert_eq!(schedule.deadline(), Some(t0 + ms(500)));
}

#[test]
fn a_later_change_does_not_push_the_deadline_back() {
    let t0 = Instant::now();
    let mut schedule = SaveSchedule::default();
    schedule.changed(t0);

    schedule.changed(t0 + ms(300));

    assert_eq!(schedule.deadline(), Some(t0 + ms(500)));
}

#[test]
fn the_save_is_due_from_the_deadline_on() {
    let t0 = Instant::now();
    let mut schedule = SaveSchedule::default();
    schedule.changed(t0);

    assert!(!schedule.is_due(t0 + ms(499)));
    assert!(schedule.is_due(t0 + ms(500)));
    assert!(schedule.is_due(t0 + ms(800)));
}

#[test]
fn saved_clears_the_deadline_and_the_next_change_starts_a_new_one() {
    let t0 = Instant::now();
    let mut schedule = SaveSchedule::default();
    schedule.changed(t0);

    schedule.saved();

    assert_eq!(schedule.deadline(), None);
    schedule.changed(t0 + ms(900));
    assert_eq!(schedule.deadline(), Some(t0 + ms(1400)));
}

#[test]
fn a_failed_save_stays_pending_and_retries_5_s_later() {
    let t0 = Instant::now();
    let mut schedule = SaveSchedule::default();
    schedule.changed(t0);
    let failed_at = t0 + ms(510);

    schedule.save_failed(failed_at);

    assert_eq!(
        schedule.deadline(),
        Some(failed_at + Duration::from_secs(5))
    );
    schedule.changed(failed_at + ms(100));
    assert_eq!(
        schedule.deadline(),
        Some(failed_at + Duration::from_secs(5)),
        "a change while a retry is pending waits for the retry"
    );
}
