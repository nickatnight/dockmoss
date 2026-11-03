use std::collections::BTreeSet;
use std::path::PathBuf;

use jiff::Timestamp;
use serde::Serialize;

use super::log::{LaunchLog, Recorded};
use super::running::AppEventKind;
use super::spotlight::{SpotlightSeed, sane_seeds};
use crate::bundle_id::BundleId;
use crate::growth::{
    self, FastForward, LastUsedSources, Overgrowth, Stage, Thresholds, effective_last_used,
};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockUpdate {
    /// First run: read these apps' Spotlight dates, then call `seeded`.
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
        if self.log.baseline_at().is_none() {
            if !self.seeding && !self.dock.is_empty() {
                self.seeding = true;
                return DockUpdate::Seed(self.dock.clone());
            }
        } else if self.first_see_new_apps(now) {
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
        let sane = sane_seeds(readings, now);
        let seeds_kept = sane.seeds.len();
        for (bundle_id, seed) in sane.seeds {
            self.log.set_spotlight_seed(bundle_id, seed);
        }
        self.log.set_baseline_at(now);
        self.seeding = false;
        self.first_see_new_apps(now);
        Some(SeedReport {
            dock_apps,
            dates_read,
            seeds_kept,
            migration_batch: sane.migration_batch,
        })
    }

    /// One entry per current Dock app, in Dock order.
    pub fn growth(
        &self,
        now: Timestamp,
        thresholds: &Thresholds,
        fast_forward: FastForward,
    ) -> Vec<AppGrowth> {
        self.dock
            .iter()
            .map(|app| {
                let id = &app.bundle_id;
                let sources = LastUsedSources {
                    launch_log: self.log.last_launched(id),
                    spotlight_seed: self.log.spotlight_seed(id).map(SpotlightSeed::at),
                    baseline: self.log.first_seen(id),
                    running: self.running.contains(id),
                };
                let last_used = effective_last_used(now, &sources);
                let growth =
                    growth::growth(now, last_used, growth::jitter(id), thresholds, fast_forward);
                AppGrowth {
                    bundle_id: id.clone(),
                    overgrowth: growth.overgrowth(),
                    stage: growth.stage(),
                }
            })
            .collect()
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
