use jiff::Timestamp;

use crate::growth::{LastUsedSources, effective_last_used};

fn at(text: &str) -> Timestamp {
    text.parse().expect("test timestamps are valid RFC 3339")
}

#[test]
fn effective_last_used_takes_the_latest_source_clamped_to_now() {
    let now = at("2026-11-02T12:00:00Z");
    let jan = Some(at("2026-01-15T08:00:00Z"));
    let mar = Some(at("2026-03-01T09:30:00Z"));
    let jun = Some(at("2026-06-20T18:45:00Z"));
    let tomorrow = Some(at("2026-11-03T12:00:00Z"));

    let cases = [
        ("all absent", None, None, None, false, None),
        ("baseline only", None, None, jan, false, jan),
        ("log later than Spotlight", mar, jan, None, false, mar),
        ("Spotlight later than log", jan, mar, None, false, mar),
        (
            "log with a null Spotlight date",
            mar,
            None,
            None,
            false,
            mar,
        ),
        ("all three, baseline latest", jan, mar, jun, false, jun),
        ("all three, log latest", jun, jan, mar, false, jun),
        ("running with old sources", jan, mar, jun, true, Some(now)),
        ("running with no sources", None, None, None, true, Some(now)),
        (
            "a log entry in the future",
            tomorrow,
            jan,
            None,
            false,
            Some(now),
        ),
        (
            "a Spotlight date in the future",
            jan,
            tomorrow,
            None,
            false,
            Some(now),
        ),
        (
            "a baseline in the future",
            None,
            None,
            tomorrow,
            false,
            Some(now),
        ),
    ];

    for (name, launch_log, spotlight, baseline, running, expected) in cases {
        let sources = LastUsedSources {
            launch_log,
            spotlight,
            baseline,
            running,
        };

        assert_eq!(effective_last_used(now, &sources), expected, "{name}");
    }
}
