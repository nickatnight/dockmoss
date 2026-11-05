use std::collections::BTreeSet;
use std::path::Path;

use serde::{Serialize, Serializer};
use serde_json::{Map, Value, json};

use crate::bundle_id::BundleId;
use crate::growth::{self, FastForward, GrowthError, Thresholds};
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
}

impl SettingsView {
    pub fn new(settings: Settings, dock: Option<&DistinctDockApps>) -> Self {
        let dock_apps = dock
            .map(|dock| dock.iter().map(DockAppName::new).collect::<Vec<_>>())
            .filter(|names| !names.is_empty());
        Self {
            settings,
            limits: LIMITS,
            dock_apps,
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
    if stored.is_empty() {
        return Loaded::default();
    }
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
mod tests;
