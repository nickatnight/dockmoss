use jiff::Timestamp;

use super::{DropReason, Dropped, LaunchLog, Recorded, decode, encode};
use crate::bundle_id::BundleId;
use crate::usage::spotlight::SpotlightSeed;

fn at(text: &str) -> Timestamp {
    text.parse().expect("test timestamps are valid RFC 3339")
}

fn id(text: &str) -> BundleId {
    BundleId::new(text).expect("non-empty bundle id")
}

const NOW: &str = "2026-11-02T12:00:00Z";

fn file_with(map: &str, entries: &str) -> String {
    let last_launched = if map == "last_launched" {
        ""
    } else {
        r#""last_launched": {}, "#
    };
    format!(r#"{{"version": 1, "baseline_at": null, {last_launched}"{map}": {entries}}}"#)
}

#[test]
fn record_on_an_empty_log_adds_the_entry() {
    let mut log = LaunchLog::default();

    let recorded = log.record(id("com.apple.Safari"), at("2026-11-02T09:14:03Z"));

    assert_eq!(recorded, Recorded::Changed);
    assert_eq!(
        log.last_launched(&id("com.apple.Safari")),
        Some(at("2026-11-02T09:14:03Z"))
    );
    assert_eq!(log.len(), 1);
}

#[test]
fn record_replaces_the_entry_whichever_way_the_time_moves() {
    let cases = [
        ("a later event", "2026-11-02T10:00:00Z"),
        (
            "an earlier event (latest event wins)",
            "2026-11-01T08:00:00Z",
        ),
    ];
    for (case, next) in cases {
        let mut log = LaunchLog::default();
        log.record(id("com.apple.Safari"), at("2026-11-02T09:00:00Z"));

        let recorded = log.record(id("com.apple.Safari"), at(next));

        assert_eq!(recorded, Recorded::Changed, "{case}");
        assert_eq!(
            log.last_launched(&id("com.apple.Safari")),
            Some(at(next)),
            "{case}"
        );
        assert_eq!(log.len(), 1, "{case}");
    }
}

#[test]
fn record_of_the_same_time_reports_no_change() {
    let mut log = LaunchLog::default();
    log.record(id("com.apple.Safari"), at("2026-11-02T09:00:00Z"));

    let recorded = log.record(id("com.apple.Safari"), at("2026-11-02T09:00:00Z"));

    assert_eq!(recorded, Recorded::Unchanged);
}

#[test]
fn an_unknown_app_has_no_entry() {
    let log = LaunchLog::default();

    assert_eq!(log.last_launched(&id("com.apple.Safari")), None);
    assert_eq!(log.baseline_at(), None);
}

#[test]
fn encode_then_decode_round_trips_entries_and_baseline() {
    let mut log = LaunchLog::default();
    log.record(id("com.apple.Safari"), at("2026-11-02T09:14:03Z"));
    log.record(id("com.apple.TextEdit"), at("2026-10-30T18:00:00.25Z"));
    log.record(id("com.apple.finder"), at("2026-11-02T11:59:59Z"));
    log.set_baseline_at(at("2026-10-01T00:00:00Z"));

    let decoded = decode(&encode(&log), at(NOW)).expect("an encoded log decodes");

    assert_eq!(decoded.log, log);
    assert_eq!(decoded.dropped, Vec::new());
}

#[test]
fn encode_writes_version_1_with_entries_ordered_by_bundle_id() {
    let mut log = LaunchLog::default();
    log.record(id("com.apple.finder"), at("2026-11-02T11:00:00Z"));
    log.record(id("com.apple.Safari"), at("2026-11-02T09:14:03Z"));
    log.record(id("com.apple.TextEdit"), at("2026-11-02T10:00:00Z"));

    let text = encode(&log);

    let json: serde_json::Value = serde_json::from_str(&text).expect("encode writes JSON");
    assert_eq!(
        json,
        serde_json::json!({
            "version": 1,
            "baseline_at": null,
            "last_launched": {
                "com.apple.Safari": "2026-11-02T09:14:03Z",
                "com.apple.TextEdit": "2026-11-02T10:00:00Z",
                "com.apple.finder": "2026-11-02T11:00:00Z",
            },
            "first_seen": {},
            "spotlight_seed": {},
        })
    );
    let safari = text.find("com.apple.Safari").expect("Safari is written");
    let text_edit = text
        .find("com.apple.TextEdit")
        .expect("TextEdit is written");
    let finder = text.find("com.apple.finder").expect("Finder is written");
    assert!(
        safari < text_edit && text_edit < finder,
        "byte order of the keys:\n{text}"
    );
}

#[test]
fn an_unreadable_file_is_an_error() {
    let cases = [
        ("not JSON", "not json"),
        ("empty", ""),
        ("a JSON array", "[]"),
        (
            "an unknown version",
            r#"{"version": 2, "baseline_at": null, "last_launched": {}}"#,
        ),
        (
            "no version",
            r#"{"baseline_at": null, "last_launched": {}}"#,
        ),
        (
            "last_launched is not an object",
            r#"{"version": 1, "baseline_at": null, "last_launched": ["com.apple.Safari"]}"#,
        ),
    ];
    for (case, text) in cases {
        assert!(decode(text, at(NOW)).is_err(), "{case}");
    }
}

#[test]
fn a_bad_entry_is_dropped_and_the_rest_of_the_log_loads() {
    let text = r#"{
        "version": 1,
        "baseline_at": null,
        "last_launched": {
            "com.apple.Safari": "2026-11-02T09:14:03Z",
            "com.apple.Notes": "yesterday",
            "": "2026-11-01T09:00:00Z",
            "com.apple.TextEdit": "2026-11-01T10:00:00Z"
        }
    }"#;

    let decoded = decode(text, at(NOW)).expect("the file is readable");

    assert_eq!(decoded.log.len(), 2);
    assert_eq!(
        decoded.log.last_launched(&id("com.apple.Safari")),
        Some(at("2026-11-02T09:14:03Z"))
    );
    assert_eq!(
        decoded.log.last_launched(&id("com.apple.TextEdit")),
        Some(at("2026-11-01T10:00:00Z"))
    );
    assert_eq!(
        decoded.dropped,
        vec![
            Dropped::Entry {
                bundle_id: String::new(),
                reason: DropReason::InvalidBundleId,
            },
            Dropped::Entry {
                bundle_id: "com.apple.Notes".into(),
                reason: DropReason::UnparseableTimestamp,
            },
        ]
    );
}

#[test]
fn entries_dated_past_the_five_minute_tolerance_are_dropped() {
    let cases = [
        ("now", "2026-11-02T12:00:00Z", true),
        ("4 minutes ahead", "2026-11-02T12:04:00Z", true),
        ("exactly 5 minutes ahead", "2026-11-02T12:05:00Z", true),
        ("6 minutes ahead", "2026-11-02T12:06:00Z", false),
        ("a year ahead", "2027-11-02T12:00:00Z", false),
    ];
    for (case, timestamp, kept) in cases {
        let text = format!(
            r#"{{"version": 1, "baseline_at": null, "last_launched": {{"com.apple.Safari": "{timestamp}"}}}}"#
        );

        let decoded = decode(&text, at(NOW)).expect("the file is readable");

        let expected_entry = kept.then(|| at(timestamp));
        assert_eq!(
            decoded.log.last_launched(&id("com.apple.Safari")),
            expected_entry,
            "{case}"
        );
        let expected_dropped = if kept {
            vec![]
        } else {
            vec![Dropped::Entry {
                bundle_id: "com.apple.Safari".into(),
                reason: DropReason::InFuture,
            }]
        };
        assert_eq!(decoded.dropped, expected_dropped, "{case}");
    }
}

#[test]
fn a_bad_or_future_baseline_becomes_none_and_is_reported() {
    let cases = [
        ("null", "null", None, None),
        (
            "in the past",
            r#""2026-10-01T00:00:00Z""#,
            Some("2026-10-01T00:00:00Z"),
            None,
        ),
        (
            "6 minutes ahead",
            r#""2026-11-02T12:06:00Z""#,
            None,
            Some(DropReason::InFuture),
        ),
        (
            "unparseable",
            r#""soon""#,
            None,
            Some(DropReason::UnparseableTimestamp),
        ),
    ];
    for (case, baseline, expected, reason) in cases {
        let text = format!(
            r#"{{"version": 1, "baseline_at": {baseline}, "last_launched": {{"com.apple.Safari": "2026-11-01T10:00:00Z"}}}}"#
        );

        let decoded = decode(&text, at(NOW)).expect("the file is readable");

        assert_eq!(decoded.log.baseline_at(), expected.map(at), "{case}");
        assert_eq!(
            decoded.dropped,
            reason
                .map(|reason| Dropped::Baseline { reason })
                .into_iter()
                .collect::<Vec<_>>(),
            "{case}"
        );
        assert_eq!(decoded.log.len(), 1, "{case}: entries still load");
    }
}

#[test]
fn encode_then_decode_round_trips_all_four_fields() {
    let mut log = LaunchLog::default();
    log.set_baseline_at(at("2026-11-02T09:00:00Z"));
    log.record(id("com.apple.Safari"), at("2026-11-02T09:14:03Z"));
    log.set_first_seen(id("com.apple.Chess"), at("2026-11-02T09:00:00Z"));
    log.set_spotlight_seed(
        id("com.apple.TextEdit"),
        SpotlightSeed::assume_sane(at("2026-10-03T06:45:44Z")),
    );

    let text = encode(&log);
    let decoded = decode(&text, at(NOW)).expect("an encoded log decodes");

    assert_eq!(decoded.log, log);
    assert_eq!(decoded.dropped, Vec::new());
    let json: serde_json::Value = serde_json::from_str(&text).expect("encode writes JSON");
    assert_eq!(
        json,
        serde_json::json!({
            "version": 1,
            "baseline_at": "2026-11-02T09:00:00Z",
            "last_launched": { "com.apple.Safari": "2026-11-02T09:14:03Z" },
            "first_seen": { "com.apple.Chess": "2026-11-02T09:00:00Z" },
            "spotlight_seed": { "com.apple.TextEdit": "2026-10-03T06:45:44Z" },
        })
    );
}

#[test]
fn a_job_007_file_without_the_new_maps_loads() {
    let text = r#"{
        "version": 1,
        "baseline_at": null,
        "last_launched": { "com.apple.Safari": "2026-11-02T09:14:03Z" }
    }"#;

    let decoded = decode(text, at(NOW)).expect("a job 007 file is readable");

    assert_eq!(
        decoded.log.last_launched(&id("com.apple.Safari")),
        Some(at("2026-11-02T09:14:03Z"))
    );
    assert_eq!(decoded.log.first_seen(&id("com.apple.Safari")), None);
    assert_eq!(decoded.log.spotlight_seed(&id("com.apple.Safari")), None);
    assert_eq!(decoded.dropped, Vec::new());
}

#[test]
fn a_value_that_is_not_a_string_drops_only_that_entry() {
    for map in ["last_launched", "first_seen", "spotlight_seed"] {
        for value in ["7", "null", "true", r#"["2026-11-01T10:00:00Z"]"#, "{}"] {
            let text = file_with(
                map,
                &format!(
                    r#"{{"com.apple.Bad": {value}, "com.apple.Good": "2026-11-01T10:00:00Z"}}"#
                ),
            );

            let decoded = decode(&text, at(NOW)).expect("the file is readable");

            let good = id("com.apple.Good");
            let loaded = match map {
                "last_launched" => decoded.log.last_launched(&good),
                "first_seen" => decoded.log.first_seen(&good),
                _ => decoded.log.spotlight_seed(&good).map(SpotlightSeed::at),
            };
            assert_eq!(loaded, Some(at("2026-11-01T10:00:00Z")), "{map}: {value}");
            assert_eq!(
                decoded.dropped,
                vec![Dropped::Entry {
                    bundle_id: "com.apple.Bad".into(),
                    reason: DropReason::NotAString,
                }],
                "{map}: {value}"
            );
        }
    }
}

#[test]
fn a_baseline_that_is_not_a_string_or_null_is_dropped() {
    for baseline in ["5", "true", "[]", "{}"] {
        let text = format!(
            r#"{{"version": 1, "baseline_at": {baseline}, "last_launched": {{"com.apple.Safari": "2026-11-01T10:00:00Z"}}}}"#
        );

        let decoded = decode(&text, at(NOW)).expect("the file is readable");

        assert_eq!(decoded.log.baseline_at(), None, "{baseline}");
        assert_eq!(
            decoded.dropped,
            vec![Dropped::Baseline {
                reason: DropReason::NotAString
            }],
            "{baseline}"
        );
        assert_eq!(decoded.log.len(), 1, "{baseline}: entries still load");
    }
}

#[test]
fn a_missing_baseline_reads_as_none() {
    let text = r#"{"version": 1, "last_launched": {}}"#;

    let decoded = decode(text, at(NOW)).expect("the file is readable");

    assert_eq!(decoded.log.baseline_at(), None);
    assert_eq!(decoded.dropped, Vec::new());
}

#[test]
fn a_bundle_id_over_255_bytes_is_dropped_in_every_map() {
    let long = "a".repeat(256);
    let longest = "b".repeat(255);
    for map in ["last_launched", "first_seen", "spotlight_seed"] {
        let text = file_with(
            map,
            &format!(
                r#"{{"{long}": "2026-11-01T10:00:00Z", "{longest}": "2026-11-01T10:00:00Z"}}"#
            ),
        );

        let decoded = decode(&text, at(NOW)).expect("the file is readable");

        let kept = id(&longest);
        let loaded = match map {
            "last_launched" => decoded.log.last_launched(&kept),
            "first_seen" => decoded.log.first_seen(&kept),
            _ => decoded.log.spotlight_seed(&kept).map(SpotlightSeed::at),
        };
        assert_eq!(loaded, Some(at("2026-11-01T10:00:00Z")), "{map}");
        assert_eq!(
            decoded.dropped,
            vec![Dropped::Entry {
                bundle_id: long.clone(),
                reason: DropReason::InvalidBundleId,
            }],
            "{map}"
        );
    }
}

#[test]
fn bad_or_future_first_seen_and_spotlight_entries_are_dropped() {
    let cases = [
        ("first_seen", "2026-11-02T12:06:00Z", DropReason::InFuture),
        ("first_seen", "soon", DropReason::UnparseableTimestamp),
        (
            "spotlight_seed",
            "2026-11-02T12:06:00Z",
            DropReason::InFuture,
        ),
        ("spotlight_seed", "soon", DropReason::UnparseableTimestamp),
    ];
    for (map, value, reason) in cases {
        let text = file_with(map, &format!(r#"{{"com.apple.TextEdit": "{value}"}}"#));

        let decoded = decode(&text, at(NOW)).expect("the file is readable");

        let text_edit = id("com.apple.TextEdit");
        assert_eq!(decoded.log.first_seen(&text_edit), None, "{map}: {value}");
        assert_eq!(
            decoded.log.spotlight_seed(&text_edit),
            None,
            "{map}: {value}"
        );
        assert_eq!(
            decoded.dropped,
            vec![Dropped::Entry {
                bundle_id: "com.apple.TextEdit".into(),
                reason,
            }],
            "{map}: {value}"
        );
    }
}

#[test]
fn drop_reasons_read_as_sentences() {
    let cases = [
        (
            DropReason::InvalidBundleId,
            "the bundle ID is empty or longer than 255 bytes",
        ),
        (DropReason::NotAString, "the value is not a string"),
        (
            DropReason::UnparseableTimestamp,
            "the timestamp is not RFC 3339",
        ),
        (
            DropReason::InFuture,
            "the timestamp is more than 5 minutes in the future",
        ),
    ];
    for (reason, text) in cases {
        assert_eq!(reason.to_string(), text, "{reason:?}");
    }
}
