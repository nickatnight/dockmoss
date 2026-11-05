import type {
  BundleId,
  DockAppName,
  FastForward,
  Settings,
  SettingsLimits,
  Thresholds,
} from "../ipc";

export type SettingsChange =
  | { field: "thresholds"; thresholds: Thresholds }
  | { field: "intensityPercent"; percent: number }
  | { field: "fastForward"; speed: FastForward }
  | { field: "excluded"; bundleId: BundleId; excluded: boolean };

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

export interface ExclusionRow {
  bundleId: BundleId;
  label: string;
  excluded: boolean;
  inDock: boolean;
}

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

export function thresholdMessage(limits: SettingsLimits): string {
  return `Days must rise from Fuzz to Patchy to Overgrown, from ${limits.minFuzzDays} to ${limits.maxOvergrownDays}.`;
}

const FAST_FORWARD_SPEEDS: readonly FastForward[] = [1, 10, 100, 1000];

export function toFastForward(value: string): FastForward | null {
  return FAST_FORWARD_SPEEDS.find((speed) => String(speed) === value) ?? null;
}
