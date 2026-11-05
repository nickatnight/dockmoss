use std::path::PathBuf;

use jiff::{SignedDuration, Timestamp};

use super::{Change, DistinctDockApps, DockApp, DockUpdate, SeedReport, UsageState};
use crate::bundle_id::BundleId;
use crate::growth::{FastForward, Stage, Thresholds};
use crate::settings::{Exclusions, Intensity, Settings};
use crate::usage::spotlight::SpotlightSeed;
use crate::usage::{AppEventKind, LaunchLog};

const TOLERANCE: f64 = 1e-5;
/// Safari's jitter, from the golden table in growth/jitter/tests.rs.
const SAFARI_JITTER_SECONDS: f64 = 38_842.0;
const SECONDS_PER_DAY: f64 = 86_400.0;
const DEFAULT_FUZZ_DAYS: f64 = 5.0;
const DEFAULT_PATCHY_DAYS: f64 = 10.0;
const OVERGROWTH_AT_PATCHY: f64 = 0.5;
const NOW: &str = "2026-11-03T08:00:00Z";

fn at(text: &str) -> Timestamp {
    text.parse().expect("test timestamps are valid RFC 3339")
}

fn now() -> Timestamp {
    at(NOW)
}

fn ago(duration: SignedDuration) -> Timestamp {
    now()
        .checked_sub(duration)
        .expect("test times stay in range")
}

fn days(n: i64) -> SignedDuration {
    SignedDuration::from_hours(n * 24)
}

/// Safari's overgrowth inside Fuzz after `unused` at `speed`: linear from
/// 0 at Fuzz to 0.5 at Patchy (SUMMARY.md "Growth curve").
fn safari_in_fuzz(unused: SignedDuration, speed: f64) -> f64 {
    let curve_days = (unused.as_secs_f64() * speed + SAFARI_JITTER_SECONDS) / SECONDS_PER_DAY;
    OVERGROWTH_AT_PATCHY * (curve_days - DEFAULT_FUZZ_DAYS)
        / (DEFAULT_PATCHY_DAYS - DEFAULT_FUZZ_DAYS)
}

fn id(text: &str) -> BundleId {
    BundleId::new(text).expect("non-empty bundle id")
}

fn app(bundle_id: &str) -> DockApp {
    DockApp {
        bundle_id: id(bundle_id),
        app_path: PathBuf::from(format!("/Applications/{bundle_id}.app")),
    }
}

fn baselined() -> LaunchLog {
    let mut log = LaunchLog::default();
    log.set_baseline_at(at("2026-10-01T00:00:00Z"));
    log
}

fn seeded_with_no_dates(apps: Vec<DockApp>, at: Timestamp) -> UsageState {
    let mut state = UsageState::new(LaunchLog::default());
    let DockUpdate::Seed(to_read) = state.dock_apps(apps, at) else {
        panic!("a first non-empty Dock list starts seeding");
    };
    state.seeded(
        to_read
            .into_iter()
            .map(|app| (app.bundle_id, None))
            .collect(),
        at,
    );
    state
}

fn at_speed(fast_forward: FastForward) -> Settings {
    Settings {
        fast_forward,
        ..Settings::default()
    }
}

fn stage_of(state: &UsageState, bundle_id: &str, at: Timestamp, ff: FastForward) -> (f32, Stage) {
    let growth = state.growth(at, &at_speed(ff));
    let entry = growth
        .iter()
        .find(|entry| entry.bundle_id == id(bundle_id))
        .expect("the app is in the Dock");
    (entry.overgrowth.get(), entry.stage)
}

#[test]
fn the_first_non_empty_dock_list_starts_seeding_once() {
    let mut state = UsageState::new(LaunchLog::default());

    let first = state.dock_apps(vec![app("com.apple.Safari"), app("com.apple.Chess")], now());
    let again = state.dock_apps(vec![app("com.apple.Safari"), app("com.apple.Chess")], now());

    assert_eq!(
        first,
        DockUpdate::Seed(vec![app("com.apple.Safari"), app("com.apple.Chess")])
    );
    assert_eq!(again, DockUpdate::Applied(Change::Unchanged));
    assert_eq!(state.log().baseline_at(), None, "not until seeded");
}

#[test]
fn an_empty_dock_list_never_starts_seeding() {
    let mut state = UsageState::new(LaunchLog::default());

    let empty = state.dock_apps(Vec::new(), now());
    let full = state.dock_apps(vec![app("com.apple.Safari")], now());

    assert_eq!(empty, DockUpdate::Applied(Change::Unchanged));
    assert_eq!(full, DockUpdate::Seed(vec![app("com.apple.Safari")]));
}

#[test]
fn seeded_stores_sane_dates_as_seeds_and_gives_the_rest_first_seen() {
    let mut state = UsageState::new(LaunchLog::default());
    state.dock_apps(
        vec![
            app("com.apple.TextEdit"),
            app("com.apple.Chess"),
            app("com.apple.Notes"),
        ],
        now(),
    );

    let report = state.seeded(
        vec![
            (id("com.apple.TextEdit"), Some(at("2026-10-03T06:45:44Z"))),
            (id("com.apple.Chess"), None),
            (id("com.apple.Notes"), Some(at("2026-11-04T00:00:00Z"))),
        ],
        now(),
    );

    let log = state.log();
    assert_eq!(
        log.spotlight_seed(&id("com.apple.TextEdit"))
            .map(SpotlightSeed::at),
        Some(at("2026-10-03T06:45:44Z"))
    );
    assert_eq!(log.first_seen(&id("com.apple.TextEdit")), None, "Q1 (A)");
    assert_eq!(log.spotlight_seed(&id("com.apple.Chess")), None);
    assert_eq!(log.first_seen(&id("com.apple.Chess")), Some(now()));
    assert_eq!(
        log.spotlight_seed(&id("com.apple.Notes")),
        None,
        "future date"
    );
    assert_eq!(log.first_seen(&id("com.apple.Notes")), Some(now()));
    assert_eq!(log.baseline_at(), Some(now()));
    assert_eq!(
        report,
        Some(SeedReport {
            dock_apps: 3,
            dates_read: 2,
            seeds_kept: 1,
            migration_batch: false,
        })
    );
}

#[test]
fn after_a_migration_batch_every_app_is_first_seen_now_with_no_seed() {
    let names: Vec<String> = (0..6).map(|n| format!("com.example.app{n}")).collect();
    let mut state = UsageState::new(LaunchLog::default());
    state.dock_apps(names.iter().map(|name| app(name)).collect(), now());
    let batch = at("2026-09-01T10:00:00Z");
    let mut readings: Vec<_> = names
        .iter()
        .take(5)
        .map(|name| (id(name), Some(batch)))
        .collect();
    readings.push((id("com.example.app5"), Some(at("2026-10-03T06:45:44Z"))));

    let report = state.seeded(readings, now());

    for name in &names {
        assert_eq!(state.log().spotlight_seed(&id(name)), None, "{name}");
        assert_eq!(state.log().first_seen(&id(name)), Some(now()), "{name}");
    }
    assert_eq!(report.map(|report| report.migration_batch), Some(true));
}

#[test]
fn an_app_that_arrives_while_seeding_is_first_seen_when_seeding_ends() {
    let mut state = UsageState::new(LaunchLog::default());
    state.dock_apps(vec![app("com.apple.Safari")], now());
    let arrived = state.dock_apps(vec![app("com.apple.Safari"), app("com.apple.Chess")], now());
    assert_eq!(
        arrived,
        DockUpdate::Applied(Change::Growth),
        "no first-seen yet"
    );
    assert_eq!(state.log().first_seen(&id("com.apple.Chess")), None);

    let later = at("2026-11-03T08:00:02Z");
    state.seeded(vec![(id("com.apple.Safari"), None)], later);

    assert_eq!(state.log().first_seen(&id("com.apple.Chess")), Some(later));
    assert_eq!(state.log().first_seen(&id("com.apple.Safari")), Some(later));
}

#[test]
fn seeded_without_a_seeding_in_flight_changes_nothing() {
    let mut state = UsageState::new(baselined());
    state.dock_apps(vec![app("com.apple.Safari")], now());
    let before = state.log().clone();

    let report = state.seeded(vec![(id("com.apple.Safari"), Some(ago(days(40))))], now());

    assert_eq!(report, None);
    assert_eq!(state.log(), &before);
}

#[test]
fn a_job_007_log_with_launches_keeps_them_through_the_first_run() {
    let mut log = LaunchLog::default();
    log.record(id("com.apple.Safari"), at("2026-11-02T09:14:03Z"));
    let mut state = UsageState::new(log);
    state.dock_apps(vec![app("com.apple.Safari"), app("com.apple.Chess")], now());

    state.seeded(
        vec![
            (id("com.apple.Safari"), None),
            (id("com.apple.Chess"), None),
        ],
        now(),
    );

    let log = state.log();
    assert_eq!(
        log.last_launched(&id("com.apple.Safari")),
        Some(at("2026-11-02T09:14:03Z"))
    );
    assert_eq!(
        log.first_seen(&id("com.apple.Safari")),
        None,
        "it has a launch"
    );
    assert_eq!(log.first_seen(&id("com.apple.Chess")), Some(now()));
    assert_eq!(log.baseline_at(), Some(now()));
}

#[test]
fn after_the_first_run_a_new_app_is_first_seen_now() {
    let mut state = UsageState::new(baselined());

    let update = state.dock_apps(vec![app("com.apple.Chess")], now());

    assert_eq!(update, DockUpdate::Applied(Change::Log));
    assert_eq!(state.log().first_seen(&id("com.apple.Chess")), Some(now()));
    assert_eq!(state.log().spotlight_seed(&id("com.apple.Chess")), None);
}

#[test]
fn an_app_with_any_data_gets_nothing_new() {
    let mut log = baselined();
    log.record(id("com.apple.Safari"), at("2026-11-01T00:00:00Z"));
    log.set_first_seen(id("com.apple.Chess"), at("2026-10-20T00:00:00Z"));
    let mut state = UsageState::new(log);

    let update = state.dock_apps(vec![app("com.apple.Safari"), app("com.apple.Chess")], now());

    assert_eq!(
        update,
        DockUpdate::Applied(Change::Growth),
        "only the list changed"
    );
    assert_eq!(state.log().first_seen(&id("com.apple.Safari")), None);
    assert_eq!(
        state.log().first_seen(&id("com.apple.Chess")),
        Some(at("2026-10-20T00:00:00Z"))
    );
}

#[test]
fn an_app_with_a_spotlight_seed_gets_no_first_seen_later() {
    let mut state = UsageState::new(LaunchLog::default());
    state.dock_apps(vec![app("com.apple.TextEdit")], ago(days(1)));
    state.seeded(
        vec![(id("com.apple.TextEdit"), Some(ago(days(30))))],
        ago(days(1)),
    );
    state.dock_apps(Vec::new(), now());

    let update = state.dock_apps(vec![app("com.apple.TextEdit")], now());

    assert_eq!(update, DockUpdate::Applied(Change::Growth));
    assert_eq!(state.log().first_seen(&id("com.apple.TextEdit")), None);
}

#[test]
fn the_same_dock_list_again_changes_nothing() {
    let mut state = UsageState::new(baselined());
    state.dock_apps(vec![app("com.apple.Chess")], now());
    let before = state.log().clone();

    let update = state.dock_apps(vec![app("com.apple.Chess")], at("2026-11-03T09:00:00Z"));

    assert_eq!(update, DockUpdate::Applied(Change::Unchanged));
    assert_eq!(state.log(), &before);
}

#[test]
fn used_records_the_time_and_reports_what_changed() {
    let mut state = UsageState::new(baselined());
    let launched = at("2026-11-03T07:00:00Z");

    let first = state.used(id("com.apple.Safari"), launched, AppEventKind::Launched);
    let repeat = state.used(id("com.apple.Safari"), launched, AppEventKind::Activated);
    let quit = state.used(id("com.apple.Safari"), launched, AppEventKind::Terminated);

    assert_eq!(first, Change::Log);
    assert_eq!(repeat, Change::Unchanged, "same time, still running");
    assert_eq!(quit, Change::Growth, "same time, but no longer running");
    assert_eq!(
        state.log().last_launched(&id("com.apple.Safari")),
        Some(launched)
    );
}

#[test]
fn a_running_app_is_clean_whatever_its_dates() {
    let mut log = baselined();
    log.set_first_seen(id("com.apple.Safari"), ago(days(60)));
    let mut state = UsageState::new(log);
    state.dock_apps(vec![app("com.apple.Safari")], now());
    assert_eq!(
        stage_of(&state, "com.apple.Safari", now(), FastForward::X1).1,
        Stage::Overgrown,
        "control: 60 days unused"
    );

    state.used(
        id("com.apple.Safari"),
        ago(days(60)),
        AppEventKind::Launched,
    );

    assert_eq!(
        stage_of(&state, "com.apple.Safari", now(), FastForward::X1000),
        (0.0, Stage::Clean)
    );
}

#[test]
fn a_quit_app_grows_from_the_quit_time() {
    let mut state = UsageState::new(baselined());
    state.dock_apps(vec![app("com.apple.Safari")], ago(days(20)));
    state.used(
        id("com.apple.Safari"),
        ago(days(20)),
        AppEventKind::Launched,
    );

    state.used(
        id("com.apple.Safari"),
        ago(days(6)),
        AppEventKind::Terminated,
    );

    let (overgrowth, reached) = stage_of(&state, "com.apple.Safari", now(), FastForward::X1);
    assert_eq!(reached, Stage::Fuzz);
    let expected = safari_in_fuzz(days(6), 1.0);
    assert!(
        (f64::from(overgrowth) - expected).abs() < TOLERANCE,
        "{overgrowth}, expected {expected}"
    );
}

#[test]
fn an_app_first_seen_6_days_ago_is_in_fuzz() {
    let state = seeded_with_no_dates(vec![app("com.apple.Safari")], ago(days(6)));

    let (overgrowth, reached) = stage_of(&state, "com.apple.Safari", now(), FastForward::X1);

    assert_eq!(reached, Stage::Fuzz);
    let expected = safari_in_fuzz(days(6), 1.0);
    assert!(
        (f64::from(overgrowth) - expected).abs() < TOLERANCE,
        "{overgrowth}, expected {expected}"
    );
}

#[test]
fn at_1000x_an_app_first_seen_8_6_minutes_ago_is_in_fuzz() {
    let state = seeded_with_no_dates(
        vec![app("com.apple.Safari")],
        ago(SignedDuration::from_secs(516)),
    );

    let (overgrowth, reached) = stage_of(&state, "com.apple.Safari", now(), FastForward::X1000);

    assert_eq!(reached, Stage::Fuzz);
    let expected = safari_in_fuzz(SignedDuration::from_secs(516), 1000.0);
    assert!(
        (f64::from(overgrowth) - expected).abs() < TOLERANCE,
        "{overgrowth}, expected {expected}"
    );
    assert_eq!(
        stage_of(&state, "com.apple.Safari", now(), FastForward::X1).1,
        Stage::Clean,
        "control: the same 8.6 minutes at 1x"
    );
}

#[test]
fn a_spotlight_seed_drives_growth_on_day_one() {
    let mut state = UsageState::new(LaunchLog::default());
    state.dock_apps(vec![app("com.apple.finder"), app("com.apple.Chess")], now());
    state.seeded(
        vec![
            (id("com.apple.finder"), Some(ago(days(40)))),
            (id("com.apple.Chess"), None),
        ],
        now(),
    );

    assert_eq!(
        stage_of(&state, "com.apple.finder", now(), FastForward::X1),
        (1.0, Stage::Overgrown)
    );
    assert_eq!(
        stage_of(&state, "com.apple.Chess", now(), FastForward::X1),
        (0.0, Stage::Clean)
    );
}

#[test]
fn growth_lists_the_dock_apps_in_dock_order() {
    let mut state = UsageState::new(baselined());
    state.dock_apps(
        vec![
            app("com.microsoft.VSCode"),
            app("com.apple.Safari"),
            app("com.apple.finder"),
        ],
        now(),
    );

    let order: Vec<String> = state
        .growth(now(), &Settings::default())
        .into_iter()
        .map(|entry| entry.bundle_id.as_str().to_owned())
        .collect();

    assert_eq!(
        order,
        [
            "com.microsoft.VSCode",
            "com.apple.Safari",
            "com.apple.finder"
        ]
    );
}

#[test]
fn an_app_with_no_data_at_all_is_clean() {
    let mut state = UsageState::new(LaunchLog::default());
    state.dock_apps(vec![app("com.apple.Chess")], now());

    assert_eq!(
        stage_of(
            &state,
            "com.apple.Chess",
            at("2027-11-03T08:00:00Z"),
            FastForward::X1000
        ),
        (0.0, Stage::Clean),
        "seeding still in flight"
    );
}

#[test]
fn growth_serializes_in_camel_case_with_a_lowercase_stage() {
    let state = seeded_with_no_dates(vec![app("com.apple.Safari")], ago(days(6)));

    let json = serde_json::to_value(state.growth(now(), &Settings::default()))
        .expect("growth serializes to JSON");

    let entry = json.get(0).expect("one entry");
    assert_eq!(
        entry.get("bundleId"),
        Some(&serde_json::json!("com.apple.Safari"))
    );
    assert_eq!(entry.get("stage"), Some(&serde_json::json!("fuzz")));
    assert!(
        entry
            .get("overgrowth")
            .is_some_and(serde_json::Value::is_f64)
    );
}

#[test]
fn a_log_change_saves_and_any_change_recomputes_growth() {
    let cases = [
        (Change::Unchanged, false, false),
        (Change::Growth, false, true),
        (Change::Log, true, true),
    ];
    for (change, saves, moves_growth) in cases {
        assert_eq!(change.saves(), saves, "{change:?}");
        assert_eq!(change.moves_growth(), moves_growth, "{change:?}");
    }
}

#[test]
fn an_app_pinned_twice_is_read_once_and_counted_once() {
    let mut state = UsageState::new(LaunchLog::default());
    let dock = vec![
        app("com.microsoft.VSCode"),
        app("com.apple.Safari"),
        app("com.microsoft.VSCode"),
    ];

    let DockUpdate::Seed(to_read) = state.dock_apps(dock, now()) else {
        panic!("a first non-empty Dock list starts seeding");
    };
    assert_eq!(
        to_read,
        vec![app("com.microsoft.VSCode"), app("com.apple.Safari")]
    );
    let readings = to_read
        .into_iter()
        .map(|app| (app.bundle_id, Some(ago(days(40)))))
        .collect();
    let report = state.seeded(readings, now());

    assert_eq!(
        report,
        Some(SeedReport {
            dock_apps: 2,
            dates_read: 2,
            seeds_kept: 2,
            migration_batch: false,
        })
    );
    assert_eq!(
        state.growth(now(), &Settings::default()).len(),
        3,
        "growth keeps one entry per Dock item"
    );
}

#[test]
fn distinct_dock_apps_keep_the_first_entry_per_bundle_id_in_dock_order() {
    let second_copy = DockApp {
        bundle_id: id("com.apple.Safari"),
        app_path: PathBuf::from("/Users/someone/Applications/Safari.app"),
    };
    let dock = [
        app("com.apple.Safari"),
        app("com.apple.Chess"),
        second_copy,
        app("com.apple.Notes"),
    ];

    let distinct = DistinctDockApps::new(&dock);

    assert_eq!(
        distinct.iter().cloned().collect::<Vec<_>>(),
        vec![
            app("com.apple.Safari"),
            app("com.apple.Chess"),
            app("com.apple.Notes")
        ]
    );
}

fn three_apps() -> UsageState {
    let mut log = baselined();
    log.set_first_seen(id("com.apple.finder"), ago(days(40)));
    log.set_first_seen(id("com.apple.Safari"), ago(days(6)));
    log.set_first_seen(id("com.apple.Chess"), now());
    let mut state = UsageState::new(log);
    state.dock_apps(
        vec![
            app("com.apple.finder"),
            app("com.apple.Safari"),
            app("com.apple.Chess"),
        ],
        now(),
    );
    state.used(
        id("com.apple.finder"),
        ago(days(40)),
        AppEventKind::Terminated,
    );
    state
}

fn frame(state: &UsageState, settings: &Settings) -> Vec<(String, f32, Stage)> {
    state
        .growth(now(), settings)
        .into_iter()
        .map(|entry| {
            (
                entry.bundle_id.as_str().to_owned(),
                entry.overgrowth.get(),
                entry.stage,
            )
        })
        .collect()
}

fn excluding(ids: &[&str]) -> Exclusions {
    Exclusions::new(ids.iter().map(|text| id(text))).expect("under the cap")
}

fn intensity(percent: u64) -> Intensity {
    Intensity::new(percent).expect("0 to 100")
}

fn approx(actual: f32, expected: f64) -> bool {
    (f64::from(actual) - expected).abs() < TOLERANCE
}

#[test]
fn the_default_settings_draw_the_curve_unchanged() {
    let got = frame(&three_apps(), &Settings::default());

    let safari = safari_in_fuzz(days(6), 1.0);
    assert_eq!(got.len(), 3);
    assert_eq!(
        got[0],
        ("com.apple.finder".to_owned(), 1.0, Stage::Overgrown)
    );
    assert_eq!(
        (got[1].0.as_str(), got[1].2),
        ("com.apple.Safari", Stage::Fuzz)
    );
    assert!(approx(got[1].1, safari), "{}, expected {safari}", got[1].1);
    assert_eq!(got[2], ("com.apple.Chess".to_owned(), 0.0, Stage::Clean));
}

/// Finder's jitter, from the golden table in growth/jitter/tests.rs.
const FINDER_JITTER_SECONDS: f64 = 31_764.0;

/// Where an app unused for `unused_days` sits on the curve, jitter included.
fn curve_day(unused_days: f64, jitter_seconds: f64) -> f64 {
    unused_days + jitter_seconds / SECONDS_PER_DAY
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the test's curve days are small and positive"
)]
fn whole(days: f64) -> u32 {
    days as u32
}

#[test]
fn thresholds_move_each_app_along_the_curve() {
    let safari = curve_day(6.0, SAFARI_JITTER_SECONDS);
    let finder = curve_day(40.0, FINDER_JITTER_SECONDS);
    let cases = [
        (
            "fuzz starts after Safari's day, overgrown before Finder's",
            (
                whole(safari.ceil()),
                whole(safari.ceil()) + 1,
                whole(finder.floor()),
            ),
            Stage::Clean,
            Stage::Overgrown,
        ),
        (
            "every stage starts before Safari's day",
            (2, 3, whole(safari.floor())),
            Stage::Overgrown,
            Stage::Overgrown,
        ),
        (
            "Safari between fuzz and patchy, overgrown after Finder's day",
            (5, 10, whole(finder.ceil())),
            Stage::Fuzz,
            Stage::Patchy,
        ),
    ];
    for (case, (fuzz, patchy, overgrown), safari, finder) in cases {
        let settings = Settings {
            thresholds: Thresholds::new(fuzz, patchy, overgrown).expect("valid"),
            ..Settings::default()
        };

        let got = frame(&three_apps(), &settings);

        assert_eq!(got[1].2, safari, "{case}: Safari");
        assert_eq!(got[0].2, finder, "{case}: Finder");
        assert_eq!(got[2].2, Stage::Clean, "{case}: Chess");
    }
}

#[test]
fn fast_forward_from_the_settings_speeds_up_growth() {
    let settings = at_speed(FastForward::X10);

    let got = frame(&three_apps(), &settings);

    assert_eq!(got[1].2, Stage::Overgrown);
    assert_eq!(got[2].2, Stage::Clean);
}

#[test]
fn an_excluded_app_is_clean_and_keeps_its_entry() {
    let settings = Settings {
        excluded: excluding(&["com.apple.finder", "com.example.NotInTheDock"]),
        ..Settings::default()
    };

    let got = frame(&three_apps(), &settings);

    assert_eq!(got.len(), 3, "one entry per Dock app");
    assert_eq!(got[0], ("com.apple.finder".to_owned(), 0.0, Stage::Clean));
    assert_eq!(got[1].2, Stage::Fuzz, "Safari is not excluded");
}

#[test]
fn intensity_scales_overgrowth_and_keeps_the_stage() {
    let safari = safari_in_fuzz(days(6), 1.0);
    let cases = [
        (100, 1.0, safari),
        (30, 0.3, 0.3 * safari),
        (50, 0.5, 0.5 * safari),
        (1, 0.01, 0.01 * safari),
        (0, 0.0, 0.0),
    ];
    for (percent, finder, expected_safari) in cases {
        let settings = Settings {
            intensity: intensity(percent),
            ..Settings::default()
        };

        let got = frame(&three_apps(), &settings);

        assert!(approx(got[0].1, finder), "{percent}%: Finder {}", got[0].1);
        assert_eq!(got[0].2, Stage::Overgrown, "{percent}%: Finder's stage");
        assert!(
            approx(got[1].1, expected_safari),
            "{percent}%: Safari {}, expected {expected_safari}",
            got[1].1
        );
        assert_eq!(got[1].2, Stage::Fuzz, "{percent}%: Safari's stage");
        assert_eq!(got[2].1, 0.0, "{percent}%: Chess");
    }
}

#[test]
fn intensity_100_sends_exactly_the_unscaled_overgrowth() {
    let unscaled = frame(&three_apps(), &Settings::default());
    let settings = Settings {
        intensity: intensity(100),
        ..Settings::default()
    };

    assert_eq!(frame(&three_apps(), &settings), unscaled);
}

#[test]
fn an_excluded_app_stays_clean_at_any_intensity() {
    for percent in [0, 30, 100] {
        let settings = Settings {
            excluded: excluding(&["com.apple.Safari"]),
            intensity: intensity(percent),
            ..Settings::default()
        };

        let got = frame(&three_apps(), &settings);

        assert_eq!(
            got[1],
            ("com.apple.Safari".to_owned(), 0.0, Stage::Clean),
            "{percent}%"
        );
        assert_eq!(got[0].2, Stage::Overgrown, "{percent}%: Finder");
    }
}
