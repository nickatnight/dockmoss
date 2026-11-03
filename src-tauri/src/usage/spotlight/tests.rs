use jiff::{SignedDuration, Timestamp};

use super::{SaneSeeds, is_migration_batch, parse_mdls, sane_seeds};
use crate::bundle_id::BundleId;

fn at(text: &str) -> Timestamp {
    text.parse().expect("test timestamps are valid RFC 3339")
}

fn id(text: &str) -> BundleId {
    BundleId::new(text).expect("non-empty bundle id")
}

const NOW: &str = "2026-11-03T08:00:00Z";

#[test]
fn parse_mdls_reads_the_raw_date_line() {
    let cases = [
        (
            "a real mdls -raw line",
            "2026-11-03 06:45:44 +0000",
            Some("2026-11-03T06:45:44Z"),
        ),
        (
            "with a trailing newline and spaces",
            "  2026-10-29 01:37:04 +0000\n",
            Some("2026-10-29T01:37:04Z"),
        ),
        (
            "a non-UTC offset",
            "2026-10-29 03:37:04 +0200",
            Some("2026-10-29T01:37:04Z"),
        ),
        ("(null)", "(null)", None),
        ("(null) with a newline", "(null)\n", None),
        ("empty", "", None),
        ("garbage", "not a date", None),
        ("a date with no zone", "2026-11-03 06:45:44", None),
        ("an RFC 3339 date", "2026-11-03T06:45:44Z", None),
        (
            "mdls's missing-file message",
            "/nonexistent.app: could not find /nonexistent.app.",
            None,
        ),
        ("an impossible date", "2026-02-30 06:45:44 +0000", None),
    ];
    for (case, stdout, expected) in cases {
        assert_eq!(parse_mdls(stdout), expected.map(at), "{case}");
    }
}

fn seconds_after(start: &str, offsets_ms: &[i64]) -> Vec<Timestamp> {
    let start = at(start);
    offsets_ms
        .iter()
        .map(|&ms| {
            start
                .checked_add(SignedDuration::from_millis(ms))
                .expect("test offsets stay in range")
        })
        .collect()
}

#[test]
fn five_dates_within_five_seconds_are_a_migration_batch() {
    let start = "2026-09-01T10:00:00Z";
    let spread: Vec<i64> = (0..20).map(|n| n * 3_600_000).collect();
    let mut cluster_in_spread = seconds_after(start, &spread);
    cluster_in_spread.extend(seconds_after(
        "2026-09-01T15:30:00Z",
        &[0, 1_000, 2_000, 3_000, 4_000],
    ));
    let cases = [
        (
            "5 within 5 s",
            seconds_after(start, &[0, 1_000, 2_000, 3_000, 4_000]),
            true,
        ),
        (
            "5 at the same second",
            seconds_after(start, &[0, 0, 0, 0, 0]),
            true,
        ),
        (
            "4 within 5 s",
            seconds_after(start, &[0, 1_000, 2_000, 3_000]),
            false,
        ),
        (
            "5 spread over 5.001 s",
            seconds_after(start, &[0, 1_000, 2_000, 3_000, 5_001]),
            false,
        ),
        (
            "5 exactly 5 s end to end",
            seconds_after(start, &[0, 1_250, 2_500, 3_750, 5_000]),
            true,
        ),
        (
            "unsorted, 5 within 5 s",
            seconds_after(start, &[4_000, 0, 3_000, 1_000, 2_000]),
            true,
        ),
        (
            "a cluster of 5 inside 20 spread dates",
            cluster_in_spread,
            true,
        ),
        (
            "20 dates an hour apart",
            seconds_after(start, &spread),
            false,
        ),
        ("empty", Vec::new(), false),
    ];
    for (case, dates, expected) in cases {
        assert_eq!(is_migration_batch(&dates), expected, "{case}");
    }
}

fn seeds(sane: &SaneSeeds) -> Vec<(&str, Timestamp)> {
    sane.seeds
        .iter()
        .map(|(bundle_id, seed)| (bundle_id.as_str(), seed.at()))
        .collect()
}

#[test]
fn sane_seeds_keeps_every_past_date_when_there_is_no_batch() {
    let readings = vec![
        (id("com.apple.TextEdit"), Some(at("2026-10-03T06:45:44Z"))),
        (id("com.apple.Chess"), None),
        (id("com.apple.Safari"), Some(at("2026-10-29T01:37:04Z"))),
    ];

    let sane = sane_seeds(readings, at(NOW));

    assert_eq!(
        seeds(&sane),
        vec![
            ("com.apple.TextEdit", at("2026-10-03T06:45:44Z")),
            ("com.apple.Safari", at("2026-10-29T01:37:04Z")),
        ]
    );
    assert!(!sane.migration_batch);
}

#[test]
fn sane_seeds_skips_dates_after_now() {
    let readings = vec![
        (id("com.apple.TextEdit"), Some(at("2026-11-03T08:00:00Z"))),
        (id("com.apple.Safari"), Some(at("2026-11-03T08:00:01Z"))),
        (id("com.apple.Notes"), Some(at("2027-01-01T00:00:00Z"))),
    ];

    let sane = sane_seeds(readings, at(NOW));

    assert_eq!(
        seeds(&sane),
        vec![("com.apple.TextEdit", at("2026-11-03T08:00:00Z"))],
        "now itself is kept; one second later is not"
    );
}

#[test]
fn a_migration_batch_drops_every_seed_including_those_outside_it() {
    let batch = seconds_after("2026-09-01T10:00:00Z", &[0, 500, 1_000, 1_500, 2_000]);
    let mut readings: Vec<_> = batch
        .into_iter()
        .enumerate()
        .map(|(n, date)| (id(&format!("com.example.migrated{n}")), Some(date)))
        .collect();
    readings.push((id("com.apple.TextEdit"), Some(at("2026-10-03T06:45:44Z"))));
    readings.push((id("com.apple.Chess"), None));

    let sane = sane_seeds(readings, at(NOW));

    assert_eq!(seeds(&sane), Vec::new());
    assert!(sane.migration_batch);
}

#[test]
fn future_dates_do_not_count_toward_a_batch() {
    let mut readings: Vec<_> = seconds_after("2026-09-01T10:00:00Z", &[0, 1_000, 2_000, 3_000])
        .into_iter()
        .enumerate()
        .map(|(n, date)| (id(&format!("com.example.app{n}")), Some(date)))
        .collect();
    readings.push((id("com.example.future"), Some(at("2027-09-01T10:00:00Z"))));

    let sane = sane_seeds(readings, at(NOW));

    assert_eq!(sane.seeds.len(), 4);
    assert!(!sane.migration_batch);
}
