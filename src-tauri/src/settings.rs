use std::collections::BTreeSet;
use std::path::Path;

use serde::{Serialize, Serializer};
use serde_json::{Map, Value, json};

use crate::bundle_id::BundleId;
use crate::growth::{self, FastForward, GrowthError, Overgrowth, Thresholds};
use crate::usage::{self, DistinctDockApps, DockApp};

pub const MAX_EXCLUSIONS: usize = 256;
/// More than three times the largest file a save writes (256 exclusions of
/// 255 bytes each).
pub const MAX_FILE_BYTES: u64 = 256 * 1024;
/// Longer lists are refused before their entries are read; duplicates make
/// some room above the cap.
const MAX_EXCLUDED_ENTRIES: usize = 4 * MAX_EXCLUSIONS;

pub const LIMITS: Limits = Limits {
    min_fuzz_days: growth::MIN_FUZZ_DAYS,
    max_overgrown_days: growth::MAX_OVERGROWN_DAYS,
    max_exclusions: MAX_EXCLUSIONS,
};

const VERSION_KEY: &str = "version";
const VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    pub thresholds: Thresholds,
    pub intensity: Intensity,
    pub excluded: Exclusions,
    pub fast_forward: FastForward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Intensity(u8);

impl Intensity {
    pub fn new(percent: u64) -> Option<Self> {
        u8::try_from(percent)
            .ok()
            .filter(|percent| *percent <= 100)
            .map(Self)
    }

    pub fn percent(self) -> u8 {
        self.0
    }

    pub fn scale(self, overgrowth: Overgrowth) -> Overgrowth {
        Overgrowth::new(overgrowth.get() * (f32::from(self.0) / 100.0))
    }
}

impl Default for Intensity {
    fn default() -> Self {
        Self(100)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Exclusions(BTreeSet<BundleId>);

impl Exclusions {
    pub fn new(ids: impl IntoIterator<Item = BundleId>) -> Result<Self, SettingsError> {
        let ids: BTreeSet<BundleId> = ids.into_iter().collect();
        if ids.len() > MAX_EXCLUSIONS {
            return Err(SettingsError::TooManyExclusions);
        }
        Ok(Self(ids))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn contains(&self, bundle_id: &BundleId) -> bool {
        self.0.contains(bundle_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Thresholds,
    Intensity,
    Excluded,
    FastForward,
}

impl Field {
    pub fn key(self) -> &'static str {
        match self {
            Self::Thresholds => "thresholds",
            Self::Intensity => "intensityPercent",
            Self::Excluded => "excluded",
            Self::FastForward => "fastForward",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    #[error("{} is missing", .0.key())]
    Missing(Field),
    #[error(transparent)]
    Thresholds(#[from] GrowthError),
    #[error("stage thresholds must be whole numbers of days for fuzz, patchy and overgrown")]
    ThresholdsNotWholeDays,
    #[error("intensity must be a whole percent from 0 to 100")]
    Intensity,
    #[error("fast-forward must be 1, 10, 100 or 1000")]
    FastForward,
    #[error("excluded apps must be a list of bundle IDs")]
    ExcludedNotAList,
    #[error("an excluded bundle ID is not text, is empty or is longer than 255 bytes")]
    ExcludedBundleId,
    #[error("at most {MAX_EXCLUSIONS} apps can be excluded")]
    TooManyExclusions,
}

impl SettingsError {
    pub fn field(&self) -> Field {
        match self {
            Self::Missing(field) => *field,
            Self::Thresholds(_) | Self::ThresholdsNotWholeDays => Field::Thresholds,
            Self::Intensity => Field::Intensity,
            Self::FastForward => Field::FastForward,
            Self::ExcludedNotAList | Self::ExcludedBundleId | Self::TooManyExclusions => {
                Field::Excluded
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadProblem {
    #[error("the settings file was not read ({0}); every setting uses its default")]
    Unreadable(String),
    #[error("the settings file is not a JSON object; every setting uses its default")]
    Undecodable,
    #[error("the settings file is not version {VERSION}; every setting uses its default")]
    UnknownVersion,
    #[error("{0}; {key} uses its default", key = .0.field().key())]
    Invalid(SettingsError),
    #[error("{0} excluded bundle IDs dropped: not text, empty or longer than 255 bytes")]
    DroppedExclusions(usize),
}

#[derive(Debug, Default)]
pub struct Loaded {
    pub settings: Settings,
    pub problems: Vec<LoadProblem>,
}

impl Settings {
    pub fn parse(value: &Map<String, Value>) -> Result<Self, SettingsError> {
        Ok(Self {
            thresholds: thresholds(field(value, Field::Thresholds)?)?,
            intensity: intensity(field(value, Field::Intensity)?)?,
            excluded: excluded(field(value, Field::Excluded)?)?,
            fast_forward: fast_forward(field(value, Field::FastForward)?)?,
        })
    }

    /// The settings growth applies: these, with `fast_forward_override` (the
    /// debug-only `DOCKMOSS_FAST_FORWARD`) in place of the saved speed.
    #[must_use]
    pub fn applied(&self, fast_forward_override: Option<FastForward>) -> Self {
        Self {
            fast_forward: fast_forward_override.unwrap_or(self.fast_forward),
            ..self.clone()
        }
    }

    pub fn log(&self, message: &'static str) {
        let thresholds = self.thresholds;
        tracing::info!(
            fuzz = thresholds.fuzz(),
            patchy = thresholds.patchy(),
            overgrown = thresholds.overgrown(),
            intensity_percent = self.intensity.percent(),
            fast_forward = self.fast_forward.factor(),
            excluded_apps = self.excluded.len(),
            "{message}"
        );
    }

    pub fn encode_file(&self) -> String {
        format!("{:#}\n", Value::Object(self.to_stored()))
    }

    pub fn to_stored(&self) -> Map<String, Value> {
        let mut stored = self.to_map();
        stored.insert(VERSION_KEY.to_owned(), VERSION.into());
        stored
    }

    fn to_map(&self) -> Map<String, Value> {
        let thresholds = &self.thresholds;
        let excluded: Vec<&str> = self.excluded.0.iter().map(BundleId::as_str).collect();
        [
            (
                Field::Thresholds,
                json!({
                    "fuzz": thresholds.fuzz(),
                    "patchy": thresholds.patchy(),
                    "overgrown": thresholds.overgrown(),
                }),
            ),
            (Field::Intensity, self.intensity.percent().into()),
            (Field::Excluded, excluded.into()),
            (Field::FastForward, self.fast_forward.factor().into()),
        ]
        .into_iter()
        .map(|(field, value)| (field.key().to_owned(), value))
        .collect()
    }
}

impl Serialize for Settings {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_map().serialize(serializer)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    min_fuzz_days: u32,
    max_overgrown_days: u32,
    max_exclusions: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    settings: Settings,
    limits: Limits,
    dock_apps: Option<Vec<DockAppName>>,
    launch_at_login: Option<bool>,
}

impl SettingsView {
    pub fn new(
        settings: Settings,
        dock: Option<&DistinctDockApps>,
        launch_at_login: Option<bool>,
    ) -> Self {
        let dock_apps = dock
            .map(|dock| dock.iter().map(DockAppName::new).collect::<Vec<_>>())
            .filter(|names| !names.is_empty());
        Self {
            settings,
            limits: LIMITS,
            dock_apps,
            launch_at_login,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockAppName {
    pub bundle_id: BundleId,
    pub name: String,
}

impl DockAppName {
    pub fn new(app: &DockApp) -> Self {
        let name = bundle_name(&app.app_path).unwrap_or_else(|| app.bundle_id.as_str().to_owned());
        Self {
            bundle_id: app.bundle_id.clone(),
            name,
        }
    }
}

fn bundle_name(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    let name = file_name.strip_suffix(".app").unwrap_or(file_name);
    (!name.is_empty()).then(|| name.to_owned())
}

/// The settings file was not written, so nothing changed.
#[derive(Debug, thiserror::Error)]
#[error("settings file not written: {0}")]
pub struct NotSaved(#[source] Box<dyn std::error::Error + Send + Sync>);

impl NotSaved {
    pub fn new(cause: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self(Box::new(cause))
    }
}

/// Writes `next` with `write`, and only once that succeeds makes it the
/// current settings.
pub fn commit<E>(
    current: &mut Settings,
    next: Settings,
    write: impl FnOnce(&[u8]) -> Result<(), E>,
) -> Result<(), NotSaved>
where
    E: std::error::Error + Send + Sync + 'static,
{
    write(next.encode_file().as_bytes()).map_err(NotSaved::new)?;
    *current = next;
    Ok(())
}

/// Never fails and never writes: a missing file, one that cannot be read or
/// decoded, and every bad field fall back to defaults until the next save.
pub fn load_file(path: &Path) -> Loaded {
    match usage::read_capped(path, MAX_FILE_BYTES) {
        Ok(text) => decode_file(text.as_deref().map(str::as_bytes)),
        Err(error) => Loaded {
            settings: Settings::default(),
            problems: vec![LoadProblem::Unreadable(error.to_string())],
        },
    }
}

/// `None` when there is no file yet.
pub fn decode_file(bytes: Option<&[u8]>) -> Loaded {
    let Some(bytes) = bytes else {
        return Loaded::default();
    };
    match serde_json::from_slice(bytes) {
        Ok(Value::Object(stored)) => load(&stored),
        _ => Loaded {
            settings: Settings::default(),
            problems: vec![LoadProblem::Undecodable],
        },
    }
}

pub fn load(stored: &Map<String, Value>) -> Loaded {
    if stored.get(VERSION_KEY).and_then(Value::as_u64) != Some(VERSION) {
        return Loaded {
            settings: Settings::default(),
            problems: vec![LoadProblem::UnknownVersion],
        };
    }
    let mut problems = Vec::new();
    let thresholds = or_default(
        field(stored, Field::Thresholds).and_then(thresholds),
        &mut problems,
    );
    let intensity = or_default(
        field(stored, Field::Intensity).and_then(intensity),
        &mut problems,
    );
    let (excluded, dropped) = or_default(
        field(stored, Field::Excluded).and_then(excluded_lenient),
        &mut problems,
    );
    if dropped > 0 {
        problems.push(LoadProblem::DroppedExclusions(dropped));
    }
    let fast_forward = or_default(
        field(stored, Field::FastForward).and_then(fast_forward),
        &mut problems,
    );
    Loaded {
        settings: Settings {
            thresholds,
            intensity,
            excluded,
            fast_forward,
        },
        problems,
    }
}

fn or_default<T: Default>(result: Result<T, SettingsError>, problems: &mut Vec<LoadProblem>) -> T {
    result.unwrap_or_else(|error| {
        problems.push(LoadProblem::Invalid(error));
        T::default()
    })
}

fn field(map: &Map<String, Value>, field: Field) -> Result<&Value, SettingsError> {
    map.get(field.key()).ok_or(SettingsError::Missing(field))
}

fn thresholds(value: &Value) -> Result<Thresholds, SettingsError> {
    let day = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_u64)
            .and_then(|days| u32::try_from(days).ok())
    };
    let (Some(fuzz), Some(patchy), Some(overgrown)) =
        (day("fuzz"), day("patchy"), day("overgrown"))
    else {
        return Err(SettingsError::ThresholdsNotWholeDays);
    };
    Ok(Thresholds::new(fuzz, patchy, overgrown)?)
}

fn intensity(value: &Value) -> Result<Intensity, SettingsError> {
    value
        .as_u64()
        .and_then(Intensity::new)
        .ok_or(SettingsError::Intensity)
}

fn fast_forward(value: &Value) -> Result<FastForward, SettingsError> {
    value
        .as_u64()
        .and_then(|factor| u32::try_from(factor).ok())
        .and_then(FastForward::from_factor)
        .ok_or(SettingsError::FastForward)
}

fn excluded(value: &Value) -> Result<Exclusions, SettingsError> {
    let ids = excluded_entries(value)?
        .iter()
        .map(|entry| bundle_id(entry).ok_or(SettingsError::ExcludedBundleId))
        .collect::<Result<Vec<_>, _>>()?;
    Exclusions::new(ids)
}

fn excluded_lenient(value: &Value) -> Result<(Exclusions, usize), SettingsError> {
    let entries = excluded_entries(value)?;
    let ids: Vec<BundleId> = entries.iter().filter_map(bundle_id).collect();
    let dropped = entries.len() - ids.len();
    Ok((Exclusions::new(ids)?, dropped))
}

fn excluded_entries(value: &Value) -> Result<&Vec<Value>, SettingsError> {
    let entries = value.as_array().ok_or(SettingsError::ExcludedNotAList)?;
    if entries.len() > MAX_EXCLUDED_ENTRIES {
        return Err(SettingsError::TooManyExclusions);
    }
    Ok(entries)
}

fn bundle_id(entry: &Value) -> Option<BundleId> {
    entry.as_str().and_then(BundleId::new)
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value, json};

    use std::path::PathBuf;

    use super::{
        DockAppName, Exclusions, Field, Intensity, LIMITS, LoadProblem, MAX_EXCLUSIONS,
        MAX_FILE_BYTES, Settings, SettingsError, SettingsView, commit, decode_file, load_file,
    };
    use crate::bundle_id::BundleId;
    use crate::growth::{FastForward, GrowthError, Overgrowth, Thresholds};
    use crate::usage::{DistinctDockApps, DockApp};

    fn object(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            other => panic!("not a JSON object: {other}"),
        }
    }

    fn webview_with(field: &str, value: Value) -> Map<String, Value> {
        let mut map = object(json!({
            "thresholds": { "fuzz": 5, "patchy": 10, "overgrown": 30 },
            "intensityPercent": 100,
            "excluded": [],
            "fastForward": 1,
        }));
        map.insert(field.to_owned(), value);
        map
    }

    fn stored_with(field: &str, value: Value) -> Map<String, Value> {
        let mut map = object(json!({
            "version": 1,
            "thresholds": { "fuzz": 3, "patchy": 7, "overgrown": 60 },
            "intensityPercent": 40,
            "excluded": ["com.apple.Chess"],
            "fastForward": 100,
        }));
        map.insert(field.to_owned(), value);
        map
    }

    fn customised() -> Settings {
        Settings {
            thresholds: Thresholds::new(3, 7, 60).expect("valid thresholds"),
            intensity: Intensity::new(40).expect("valid intensity"),
            excluded: Exclusions::new([BundleId::new("com.apple.Chess").expect("valid bundle ID")])
                .expect("one exclusion"),
            fast_forward: FastForward::X100,
        }
    }

    fn bundle_ids(count: usize) -> Vec<Value> {
        (0..count)
            .map(|n| Value::from(format!("com.example.app{n}")))
            .collect()
    }

    #[test]
    fn threshold_bounds_follow_the_growth_rules() {
        let cases = [
            ("fuzz below the minimum", json!([1, 10, 30]), false),
            ("smallest valid", json!([2, 3, 4]), true),
            ("fuzz equals patchy", json!([5, 5, 30]), false),
            ("patchy equals overgrown", json!([5, 30, 30]), false),
            ("descending", json!([30, 10, 5]), false),
            ("overgrown at the maximum", json!([5, 10, 3650]), true),
            ("overgrown above the maximum", json!([5, 10, 3651]), false),
        ];

        for (case, days, valid) in cases {
            let thresholds = json!({ "fuzz": days[0], "patchy": days[1], "overgrown": days[2] });

            let parsed = Settings::parse(&webview_with("thresholds", thresholds));

            assert_eq!(parsed.is_ok(), valid, "{case}: {parsed:?}");
            if let Err(error) = parsed {
                assert!(
                    matches!(
                        error,
                        SettingsError::Thresholds(GrowthError::InvalidThresholds { .. })
                    ),
                    "{case}: {error:?}"
                );
                assert_eq!(error.field(), Field::Thresholds, "{case}");
            }
        }
    }

    #[test]
    fn thresholds_must_be_three_whole_numbers() {
        let cases = [
            (
                "a fraction",
                json!({ "fuzz": 5.5, "patchy": 10, "overgrown": 30 }),
            ),
            (
                "a string",
                json!({ "fuzz": "5", "patchy": 10, "overgrown": 30 }),
            ),
            (
                "null, as an empty input sends",
                json!({ "fuzz": null, "patchy": 10, "overgrown": 30 }),
            ),
            (
                "negative",
                json!({ "fuzz": -5, "patchy": 10, "overgrown": 30 }),
            ),
            (
                "beyond u32",
                json!({ "fuzz": 5, "patchy": 10, "overgrown": 4_294_967_326_u64 }),
            ),
            ("one missing", json!({ "fuzz": 5, "patchy": 10 })),
            ("not an object", json!([5, 10, 30])),
        ];

        for (case, thresholds) in cases {
            let parsed = Settings::parse(&webview_with("thresholds", thresholds));

            assert_eq!(parsed, Err(SettingsError::ThresholdsNotWholeDays), "{case}");
        }
    }

    #[test]
    fn intensity_is_a_whole_percent() {
        let cases = [
            (json!(0), true),
            (json!(100), true),
            (json!(101), false),
            (json!(356), false),
            (json!(-1), false),
            (json!(50.5), false),
            (json!("50"), false),
        ];

        for (value, valid) in cases {
            let parsed = Settings::parse(&webview_with("intensityPercent", value.clone()));

            match parsed {
                Ok(settings) => {
                    assert!(valid, "{value} accepted");
                    assert_eq!(
                        Some(u64::from(settings.intensity.percent())),
                        value.as_u64()
                    );
                }
                Err(error) => {
                    assert!(!valid, "{value} rejected: {error}");
                    assert_eq!(error, SettingsError::Intensity);
                }
            }
        }
    }

    #[test]
    fn fast_forward_is_one_of_the_four_speeds() {
        let cases = [
            (json!(1), Some(FastForward::X1)),
            (json!(10), Some(FastForward::X10)),
            (json!(100), Some(FastForward::X100)),
            (json!(1000), Some(FastForward::X1000)),
            (json!(0), None),
            (json!(2), None),
            (json!(1001), None),
            (json!(4_294_968_296_u64), None),
            (json!("10"), None),
        ];

        for (value, expected) in cases {
            let parsed = Settings::parse(&webview_with("fastForward", value.clone()));

            match expected {
                Some(speed) => assert_eq!(parsed.map(|s| s.fast_forward), Ok(speed), "{value}"),
                None => assert_eq!(parsed, Err(SettingsError::FastForward), "{value}"),
            }
        }
    }

    #[test]
    fn save_rejects_a_bad_excluded_bundle_id() {
        let cases = [
            ("empty", json!([""])),
            ("256 bytes", json!(["a".repeat(256)])),
            ("not text", json!([42])),
        ];

        for (case, excluded) in cases {
            let parsed = Settings::parse(&webview_with("excluded", excluded));

            assert_eq!(parsed, Err(SettingsError::ExcludedBundleId), "{case}");
        }
    }

    #[test]
    fn save_accepts_256_exclusions_and_rejects_257() {
        let at_limit =
            Settings::parse(&webview_with("excluded", bundle_ids(MAX_EXCLUSIONS).into()));
        let over_limit = Settings::parse(&webview_with(
            "excluded",
            bundle_ids(MAX_EXCLUSIONS + 1).into(),
        ));

        assert_eq!(at_limit.map(|s| s.excluded.len()), Ok(MAX_EXCLUSIONS));
        assert_eq!(over_limit, Err(SettingsError::TooManyExclusions));
    }

    #[test]
    fn exclusions_are_sorted_and_counted_once() {
        let parsed = Settings::parse(&webview_with(
            "excluded",
            json!(["com.b", "com.a", "com.b"]),
        ))
        .expect("valid settings");

        assert_eq!(
            serde_json::to_value(&parsed).expect("Settings serializes")["excluded"],
            json!(["com.a", "com.b"])
        );
    }

    #[test]
    fn save_rejects_a_missing_field_and_names_it() {
        for field in [
            Field::Thresholds,
            Field::Intensity,
            Field::Excluded,
            Field::FastForward,
        ] {
            let mut map = webview_with(
                "thresholds",
                json!({ "fuzz": 5, "patchy": 10, "overgrown": 30 }),
            );
            map.remove(field.key());

            assert_eq!(
                Settings::parse(&map),
                Err(SettingsError::Missing(field)),
                "{field:?}"
            );
        }
    }

    #[test]
    fn defaults_serialize_to_the_webview_shape() {
        let json = serde_json::to_value(Settings::default()).expect("Settings serializes");

        assert_eq!(
            json,
            json!({
                "thresholds": { "fuzz": 5, "patchy": 10, "overgrown": 30 },
                "intensityPercent": 100,
                "excluded": [],
                "fastForward": 1,
            })
        );
    }

    #[test]
    fn stored_shape_is_the_webview_shape_plus_version_1() {
        assert_eq!(
            Value::Object(customised().to_stored()),
            json!({
                "version": 1,
                "thresholds": { "fuzz": 3, "patchy": 7, "overgrown": 60 },
                "intensityPercent": 40,
                "excluded": ["com.apple.Chess"],
                "fastForward": 100,
            })
        );
    }

    #[test]
    fn a_saved_value_loads_back_unchanged() {
        let loaded = super::load(&customised().to_stored());

        assert_eq!(loaded.settings, customised());
        assert_eq!(loaded.problems, []);
    }

    #[test]
    fn what_the_webview_sends_back_parses_to_the_same_settings() {
        let sent = serde_json::to_value(customised()).expect("Settings serializes");

        assert_eq!(Settings::parse(&object(sent)), Ok(customised()));
    }

    #[test]
    fn an_empty_object_is_a_file_without_version_1() {
        let loaded = decode_file(Some(b"{}"));

        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(loaded.problems, [LoadProblem::UnknownVersion]);
    }

    #[test]
    fn a_file_without_version_1_loads_every_default_with_one_problem() {
        let cases = [
            ("version 2", stored_with("version", json!(2))),
            ("version as text", stored_with("version", json!("1"))),
            ("no version", {
                let mut map = stored_with("version", json!(1));
                map.remove("version");
                map
            }),
        ];

        for (case, stored) in cases {
            let loaded = super::load(&stored);

            assert_eq!(loaded.settings, Settings::default(), "{case}");
            assert_eq!(loaded.problems, [LoadProblem::UnknownVersion], "{case}");
        }
    }

    #[test]
    fn one_missing_or_invalid_field_loads_as_its_default_and_keeps_the_rest() {
        let defaults = Settings::default();
        let cases = [
            (
                Field::Thresholds,
                json!({ "fuzz": 1, "patchy": 10, "overgrown": 30 }),
            ),
            (Field::Thresholds, json!("5/10/30")),
            (Field::Intensity, json!(101)),
            (Field::Intensity, json!(null)),
            (Field::Excluded, json!("com.apple.Chess")),
            (Field::Excluded, bundle_ids(MAX_EXCLUSIONS + 1).into()),
            (Field::FastForward, json!(5)),
            (Field::FastForward, json!([100])),
        ];

        for (field, value) in cases {
            let invalid = super::load(&stored_with(field.key(), value.clone()));
            let missing = super::load(&{
                let mut map = stored_with(field.key(), value.clone());
                map.remove(field.key());
                map
            });

            for (how, loaded) in [("invalid", invalid), ("missing", missing)] {
                let mut expected = customised();
                match field {
                    Field::Thresholds => expected.thresholds = defaults.thresholds,
                    Field::Intensity => expected.intensity = defaults.intensity,
                    Field::Excluded => expected.excluded = defaults.excluded.clone(),
                    Field::FastForward => expected.fast_forward = defaults.fast_forward,
                }
                assert_eq!(loaded.settings, expected, "{how} {field:?} {value}");
                let [LoadProblem::Invalid(error)] = loaded.problems.as_slice() else {
                    panic!("{how} {field:?} {value}: {:?}", loaded.problems);
                };
                assert_eq!(error.field(), field, "{how} {value}");
            }
        }
    }

    #[test]
    fn a_bad_excluded_bundle_id_drops_only_that_entry_on_load() {
        let stored = stored_with(
            "excluded",
            json!(["com.apple.Chess", "", "a".repeat(256), 7, "com.apple.Notes"]),
        );

        let loaded = super::load(&stored);

        assert_eq!(
            serde_json::to_value(&loaded.settings).expect("Settings serializes")["excluded"],
            json!(["com.apple.Chess", "com.apple.Notes"])
        );
        assert_eq!(loaded.problems, [LoadProblem::DroppedExclusions(3)]);
        assert_eq!(
            loaded.settings.intensity,
            Intensity::new(40).expect("valid")
        );
    }

    #[test]
    fn load_problems_name_the_field_but_never_an_excluded_bundle_id() {
        let secret = "com.example.private-name";
        let stored = stored_with("excluded", json!([secret, "", 5]));
        let mut too_many = bundle_ids(MAX_EXCLUSIONS);
        too_many.push(secret.into());
        let stored_too_many = stored_with("excluded", too_many.into());

        for stored in [stored, stored_too_many] {
            for problem in super::load(&stored).problems {
                let text = problem.to_string();
                assert!(text.contains("excluded"), "{text}");
                assert!(!text.contains(secret), "{text}");
            }
        }
    }

    fn dock_app(bundle_id: &str, path: &str) -> DockApp {
        DockApp {
            bundle_id: BundleId::new(bundle_id).expect("valid bundle ID"),
            app_path: PathBuf::from(path),
        }
    }

    #[test]
    fn a_dock_app_is_named_after_its_bundle_without_dot_app() {
        let cases = [
            ("/Applications/Safari.app", "Safari"),
            ("/Applications/Visual Studio Code.app", "Visual Studio Code"),
            ("/System/Applications/Utilities/Terminal.app/", "Terminal"),
            ("/Applications/Tool", "Tool"),
            ("/Applications/My.app.app", "My.app"),
            ("/Applications/.app", "com.example.id"),
            ("/", "com.example.id"),
            ("", "com.example.id"),
        ];

        for (path, name) in cases {
            let named = DockAppName::new(&dock_app("com.example.id", path));

            assert_eq!(named.name, name, "{path:?}");
            assert_eq!(named.bundle_id.as_str(), "com.example.id", "{path:?}");
        }
    }

    #[test]
    fn limits_come_from_the_growth_rules_and_the_exclusion_cap() {
        assert_eq!(
            serde_json::to_value(LIMITS).expect("Limits serializes"),
            json!({ "minFuzzDays": 2, "maxOvergrownDays": 3650, "maxExclusions": 256 })
        );
    }

    #[test]
    fn the_settings_view_holds_settings_limits_and_named_dock_apps_without_paths() {
        let dock = [
            dock_app("com.apple.Safari", "/Users/someone/Applications/Safari.app"),
            dock_app("com.apple.Chess", "/System/Applications/Chess.app"),
        ];

        let view = SettingsView::new(
            Settings::default(),
            Some(&DistinctDockApps::new(&dock)),
            Some(true),
        );

        let json = serde_json::to_value(&view).expect("SettingsView serializes");
        assert_eq!(
            json,
            json!({
                "settings": serde_json::to_value(Settings::default()).expect("Settings serializes"),
                "limits": serde_json::to_value(LIMITS).expect("Limits serializes"),
                "dockApps": [
                    { "bundleId": "com.apple.Safari", "name": "Safari" },
                    { "bundleId": "com.apple.Chess", "name": "Chess" },
                ],
                "launchAtLogin": true,
            })
        );
        assert!(!json.to_string().contains("someone"), "{json}");
    }

    #[test]
    fn an_unknown_dock_list_is_sent_as_null() {
        let view = SettingsView::new(Settings::default(), None, None);

        let json = serde_json::to_value(&view).expect("SettingsView serializes");
        assert_eq!(json["dockApps"], Value::Null);
        assert_eq!(
            json["launchAtLogin"],
            Value::Null,
            "an unknown login status is sent as null"
        );
    }

    #[test]
    fn an_empty_dock_list_is_unknown_because_a_readable_dock_always_holds_finder() {
        let view = SettingsView::new(
            Settings::default(),
            Some(&DistinctDockApps::new(&[])),
            Some(false),
        );

        let json = serde_json::to_value(&view).expect("SettingsView serializes");
        assert_eq!(json["dockApps"], Value::Null);
    }

    #[test]
    fn a_missing_file_loads_defaults_silently() {
        let loaded = decode_file(None);

        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(loaded.problems, []);
    }

    #[test]
    fn a_file_that_is_not_a_json_object_loads_defaults_with_one_problem() {
        let cases: [&[u8]; 4] = [
            b"{\"version\":1,\"thresholds\":",
            b"[1, 2]",
            b"",
            b"\xff\xfe",
        ];

        for bytes in cases {
            let loaded = decode_file(Some(bytes));

            assert_eq!(loaded.settings, Settings::default(), "{bytes:?}");
            assert_eq!(loaded.problems, [LoadProblem::Undecodable], "{bytes:?}");
        }
    }

    #[test]
    fn an_encoded_file_decodes_to_the_same_settings() {
        let bytes = customised().encode_file();

        let loaded = decode_file(Some(bytes.as_bytes()));

        assert_eq!(loaded.settings, customised());
        assert_eq!(loaded.problems, []);
    }

    #[test]
    fn a_commit_writes_the_new_settings_and_then_adopts_them() {
        let mut current = Settings::default();
        let mut written = Vec::new();

        let result = commit(&mut current, customised(), |bytes| {
            written = bytes.to_vec();
            Ok::<(), std::io::Error>(())
        });

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(current, customised());
        assert_eq!(written, customised().encode_file().into_bytes());
    }

    #[test]
    fn a_failed_write_keeps_the_old_settings_in_memory_and_reports_not_saved() {
        let mut current = customised();

        let result = commit(&mut current, Settings::default(), |_| {
            Err(std::io::Error::other("disk full"))
        });

        let error = result.expect_err("a failed write is an error");
        assert_eq!(current, customised());
        assert_eq!(
            serde_json::to_value(crate::Error::from(error)).expect("Error serializes"),
            json!({ "kind": "settings_not_saved", "message": "settings could not be saved" })
        );
    }

    #[test]
    fn a_failed_write_to_disk_leaves_the_file_and_memory_unchanged() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        let write = |bytes: &[u8]| crate::usage::write_atomically(&path, bytes);
        let mut current = Settings::default();
        commit(&mut current, customised(), write).expect("first save succeeds");
        let before = std::fs::read(&path).expect("saved file");
        std::fs::create_dir(dir.path().join("settings.json.tmp")).expect("block the temp path");

        let result = commit(&mut current, Settings::default(), write);

        assert!(result.is_err(), "{result:?}");
        assert_eq!(current, customised());
        assert_eq!(std::fs::read(&path).expect("file still there"), before);
    }

    #[test]
    fn a_saved_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        let mut current = Settings::default();

        commit(&mut current, customised(), |bytes| {
            crate::usage::write_atomically(&path, bytes)
        })
        .expect("save succeeds");

        let mode = std::fs::metadata(&path)
            .expect("saved file")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn an_excluded_list_over_four_times_the_cap_is_refused_even_when_it_dedupes() {
        let copies = vec![Value::from("com.apple.Chess"); 4 * MAX_EXCLUSIONS + 1];
        let at_ceiling = vec![Value::from("com.apple.Chess"); 4 * MAX_EXCLUSIONS];

        assert_eq!(
            Settings::parse(&webview_with("excluded", copies.clone().into())),
            Err(SettingsError::TooManyExclusions)
        );
        assert_eq!(
            Settings::parse(&webview_with("excluded", at_ceiling.into())).map(|s| s.excluded.len()),
            Ok(1)
        );
        assert_eq!(
            super::load(&stored_with("excluded", copies.into())).problems,
            [LoadProblem::Invalid(SettingsError::TooManyExclusions)]
        );
    }

    fn padded_to(bytes: usize) -> Vec<u8> {
        let mut text = customised().encode_file().into_bytes();
        text.resize(bytes, b' ');
        text
    }

    #[test]
    fn a_file_at_the_size_cap_loads() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        let cap = usize::try_from(MAX_FILE_BYTES).expect("cap fits usize");
        std::fs::write(&path, padded_to(cap)).expect("write");

        let loaded = load_file(&path);

        assert_eq!(loaded.settings, customised());
        assert_eq!(loaded.problems, []);
    }

    #[test]
    fn a_file_over_the_size_cap_loads_defaults_with_one_problem_and_stays() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        let cap = usize::try_from(MAX_FILE_BYTES).expect("cap fits usize");
        std::fs::write(&path, padded_to(cap + 1)).expect("write");

        let loaded = load_file(&path);

        assert_eq!(loaded.settings, Settings::default());
        assert!(
            matches!(loaded.problems.as_slice(), [LoadProblem::Unreadable(_)]),
            "{:?}",
            loaded.problems
        );
        assert_eq!(
            std::fs::metadata(&path).expect("file").len(),
            MAX_FILE_BYTES + 1
        );
    }

    #[test]
    fn a_missing_file_on_disk_loads_defaults_silently() {
        let dir = tempfile::tempdir().expect("temp dir");

        let loaded = load_file(&dir.path().join("settings.json"));

        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(loaded.problems, []);
    }

    #[test]
    fn an_unreadable_file_is_one_problem_not_two() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        std::fs::create_dir(&path).expect("a directory where the file should be");

        let loaded = load_file(&path);

        assert_eq!(loaded.settings, Settings::default());
        assert!(
            matches!(loaded.problems.as_slice(), [LoadProblem::Unreadable(_)]),
            "{:?}",
            loaded.problems
        );
    }

    #[test]
    fn applied_settings_take_the_override_speed_only_when_there_is_one() {
        let saved = customised();
        let cases = [
            ("no override", None, FastForward::X100),
            ("an override", Some(FastForward::X1000), FastForward::X1000),
            (
                "an override back to 1x",
                Some(FastForward::X1),
                FastForward::X1,
            ),
        ];
        for (case, override_speed, expected) in cases {
            let applied = saved.applied(override_speed);

            assert_eq!(applied.fast_forward, expected, "{case}");
            assert_eq!(
                Settings {
                    fast_forward: saved.fast_forward,
                    ..applied
                },
                saved,
                "{case}: every other setting is the saved one"
            );
        }
    }

    #[test]
    fn intensity_scales_overgrowth_by_its_percent() {
        let cases = [
            (100, 0.73, 0.73),
            (100, 1.0, 1.0),
            (50, 0.8, 0.4),
            (30, 1.0, 0.3),
            (0, 1.0, 0.0),
            (0, 0.0, 0.0),
        ];
        for (percent, overgrowth, expected) in cases {
            let scaled = Intensity::new(percent)
                .expect("0 to 100")
                .scale(Overgrowth::new(overgrowth));

            assert!(
                (scaled.get() - expected).abs() < 1e-6,
                "{percent}% of {overgrowth}: {}",
                scaled.get()
            );
        }
        assert_eq!(
            Intensity::new(100)
                .expect("0 to 100")
                .scale(Overgrowth::new(0.712_345_6))
                .get(),
            0.712_345_6,
            "100% is exact"
        );
    }

    #[test]
    fn exclusions_contain_only_their_bundle_ids() {
        let excluded = customised().excluded;

        assert!(excluded.contains(&BundleId::new("com.apple.Chess").expect("valid")));
        assert!(!excluded.contains(&BundleId::new("com.apple.Chess2").expect("valid")));
    }
}
