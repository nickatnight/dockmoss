// The settings form's pure parts: no DOM and no IPC.

import type {
  BundleId,
  DockAppName,
  FastForward,
  Settings,
  SettingsLimits,
  Thresholds,
} from "../ipc";

/** One committed change in the form. Each change replaces one field. */
export type SettingsChange =
  | { field: "thresholds"; thresholds: Thresholds }
  | { field: "intensityPercent"; percent: number }
  | { field: "fastForward"; speed: FastForward }
  | { field: "excluded"; bundleId: BundleId; excluded: boolean };

/**
 * The settings to save after one change: the last saved settings with that
 * one field replaced. So a pending invalid threshold never blocks saving
 * another field. Values are not checked here; Rust checks every field.
 */
export function nextSettings(
  saved: Settings,
  change: SettingsChange,
): Settings {
  switch (change.field) {
    case "thresholds":
      return { ...saved, thresholds: { ...change.thresholds } };
    case "intensityPercent":
      return { ...saved, intensityPercent: change.percent };
    case "fastForward":
      return { ...saved, fastForward: change.speed };
    case "excluded": {
      const excluded = new Set(saved.excluded);
      if (change.excluded) {
        excluded.add(change.bundleId);
      } else {
        excluded.delete(change.bundleId);
      }
      return { ...saved, excluded: [...excluded].sort() };
    }
  }
}

/** One checkbox in the excluded apps list. */
export interface ExclusionRow {
  bundleId: BundleId;
  /** The app's name, or its bundle ID when it is not in the Dock. */
  label: string;
  excluded: boolean;
  inDock: boolean;
}

/**
 * The Dock's apps in Dock order, then the excluded apps that are no longer
 * in the Dock, sorted, so an exclusion can always be cleared.
 */
export function exclusionRows(
  dockApps: readonly DockAppName[],
  excluded: readonly BundleId[],
): ExclusionRow[] {
  const excludedSet = new Set(excluded);
  const inDock = new Set(dockApps.map((app) => app.bundleId));
  const docked = dockApps.map((app) => ({
    bundleId: app.bundleId,
    label: app.name,
    excluded: excludedSet.has(app.bundleId),
    inDock: true,
  }));
  const strays = [...excludedSet]
    .filter((bundleId) => !inDock.has(bundleId))
    .sort()
    .map((bundleId) => ({
      bundleId,
      label: bundleId,
      excluded: true,
      inDock: false,
    }));
  return [...docked, ...strays];
}

/** The stage fieldset's message when Rust rejects the thresholds. */
export function thresholdMessage(limits: SettingsLimits): string {
  return `Days must rise from Fuzz to Patchy to Overgrown, from ${limits.minFuzzDays} to ${limits.maxOvergrownDays}.`;
}

const FAST_FORWARD_SPEEDS: readonly FastForward[] = [1, 10, 100, 1000];

/** Reads a fast-forward radio's value; null for anything but the four speeds. */
export function toFastForward(value: string): FastForward | null {
  return FAST_FORWARD_SPEEDS.find((speed) => String(speed) === value) ?? null;
}
