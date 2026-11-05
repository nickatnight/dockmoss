use std::collections::BTreeSet;
use std::path::PathBuf;

use jiff::Timestamp;
use serde::Serialize;

use super::log::{LaunchLog, Recorded};
use super::running::AppEventKind;
use super::spotlight::{SaneSeeds, SpotlightSeed, sane_seeds};
use crate::bundle_id::BundleId;
use crate::growth::{self, LastUsedSources, Overgrowth, Stage, effective_last_used};
use crate::settings::Settings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockApp {
    pub bundle_id: BundleId,
    pub app_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppGrowth {
    pub bundle_id: BundleId,
    pub overgrowth: Overgrowth,
    pub stage: Stage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Unchanged,
    Growth,
    Log,
}

impl Change {
    pub fn saves(self) -> bool {
        self == Self::Log
    }

    pub fn moves_growth(self) -> bool {
        self != Self::Unchanged
    }
}

/// The Dock's apps, one per bundle ID (its first entry), in Dock order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistinctDockApps(Vec<DockApp>);

impl DistinctDockApps {
    pub fn new(dock: &[DockApp]) -> Self {
        let mut seen = BTreeSet::new();
        Self(
            dock.iter()
                .filter(|app| seen.insert(&app.bundle_id))
                .cloned()
                .collect(),
        )
    }

    pub fn iter(&self) -> impl Iterator<Item = &DockApp> {
        self.0.iter()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockUpdate {
    /// First run: read these apps' Spotlight dates, one entry per bundle ID,
    /// then call `seeded`.
    Seed(Vec<DockApp>),
    Applied(Change),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeedReport {
    pub dock_apps: usize,
    pub dates_read: usize,
    pub seeds_kept: usize,
    pub migration_batch: bool,
}

#[derive(Debug)]
pub struct UsageState {
    log: LaunchLog,
    running: BTreeSet<BundleId>,
    dock: Vec<DockApp>,
    seeding: bool,
}

impl UsageState {
    pub fn new(log: LaunchLog) -> Self {
        Self {
            log,
            running: BTreeSet::new(),
            dock: Vec::new(),
            seeding: false,
        }
    }

    pub fn log(&self) -> &LaunchLog {
        &self.log
    }

    pub fn used(&mut self, bundle_id: BundleId, at: Timestamp, kind: AppEventKind) -> Change {
        let running_changed = match kind {
            AppEventKind::Launched | AppEventKind::Activated => {
                self.running.insert(bundle_id.clone())
            }
            AppEventKind::Terminated => self.running.remove(&bundle_id),
        };
        if self.log.record(bundle_id, at) == Recorded::Changed {
            Change::Log
        } else if running_changed {
            Change::Growth
        } else {
            Change::Unchanged
        }
    }

    pub fn dock_apps(&mut self, apps: Vec<DockApp>, now: Timestamp) -> DockUpdate {
        let list_changed = apps != self.dock;
        self.dock = apps;
        let first_run_done = self.log.baseline_at().is_some();
        if !first_run_done && !self.seeding && !self.dock.is_empty() {
            self.seeding = true;
            return DockUpdate::Seed(self.distinct_dock_apps().0);
        }
        if first_run_done && self.first_see_new_apps(now) {
            return DockUpdate::Applied(Change::Log);
        }
        DockUpdate::Applied(if list_changed {
            Change::Growth
        } else {
            Change::Unchanged
        })
    }

    /// Ends the first run with the Spotlight dates read for the apps that
    /// `dock_apps` returned. `None` when no first run is in flight.
    pub fn seeded(
        &mut self,
        readings: Vec<(BundleId, Option<Timestamp>)>,
        now: Timestamp,
    ) -> Option<SeedReport> {
        if !self.seeding {
            return None;
        }
        let dock_apps = readings.len();
        let dates_read = readings.iter().filter(|(_, date)| date.is_some()).count();
        let (seeds, migration_batch) = match sane_seeds(readings, now) {
            SaneSeeds::Kept(seeds) => (seeds, false),
            SaneSeeds::MigrationBatch => (Vec::new(), true),
        };
        let seeds_kept = seeds.len();
        for (bundle_id, seed) in seeds {
            self.log.set_spotlight_seed(bundle_id, seed);
        }
        self.log.set_baseline_at(now);
        self.seeding = false;
        self.first_see_new_apps(now);
        Some(SeedReport {
            dock_apps,
            dates_read,
            seeds_kept,
            migration_batch,
        })
    }

    /// One entry per current Dock app, in Dock order, under the applied
    /// settings: an excluded app is clean, and intensity scales the
    /// overgrowth but not the stage.
    pub fn growth(&self, now: Timestamp, settings: &Settings) -> Vec<AppGrowth> {
        self.dock
            .iter()
            .map(|app| {
                let id = &app.bundle_id;
                if settings.excluded.contains(id) {
                    return AppGrowth {
                        bundle_id: id.clone(),
                        overgrowth: Overgrowth::new(0.0),
                        stage: Stage::Clean,
                    };
                }
                let sources = LastUsedSources {
                    launch_log: self.log.last_launched(id),
                    spotlight_seed: self.log.spotlight_seed(id).map(SpotlightSeed::at),
                    baseline: self.log.first_seen(id),
                    running: self.running.contains(id),
                };
                let last_used = effective_last_used(now, &sources);
                let growth = growth::growth(
                    now,
                    last_used,
                    growth::jitter(id),
                    &settings.thresholds,
                    settings.fast_forward,
                );
                AppGrowth {
                    bundle_id: id.clone(),
                    overgrowth: settings.intensity.scale(growth.overgrowth()),
                    stage: growth.stage(),
                }
            })
            .collect()
    }

    pub fn distinct_dock_apps(&self) -> DistinctDockApps {
        DistinctDockApps::new(&self.dock)
    }

    fn first_see_new_apps(&mut self, now: Timestamp) -> bool {
        let mut added = false;
        for app in &self.dock {
            let id = &app.bundle_id;
            if self.log.last_launched(id).is_none()
                && self.log.spotlight_seed(id).is_none()
                && self.log.first_seen(id).is_none()
            {
                self.log.set_first_seen(id.clone(), now);
                added = true;
            }
        }
        added
    }
}

#[cfg(test)]
mod tests;
