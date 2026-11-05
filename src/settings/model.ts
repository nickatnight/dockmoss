import type {
  BundleId,
  DockAppName,
  EnteredThresholds,
  FastForward,
  SaveSettingsResult,
  Settings,
  SettingsInput,
  SettingsLimits,
} from "../ipc";

export type SettingsChange =
  | { field: "thresholds"; thresholds: EnteredThresholds }
  | { field: "intensityPercent"; percent: number }
  | { field: "fastForward"; speed: FastForward }
  | { field: "excluded"; bundleId: BundleId; excluded: boolean };

export function nextSettings(
  saved: Settings,
  change: SettingsChange,
): SettingsInput {
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

export interface SettingsSaver {
  commit(change: SettingsChange): Promise<SaveSettingsResult>;
}

export function createSettingsSaver(
  initial: Settings,
  save: (settings: SettingsInput) => Promise<SaveSettingsResult>,
): SettingsSaver {
  let saved = initial;
  let previous: Promise<unknown> = Promise.resolve();
  return {
    commit(change) {
      const result = previous.then(async () => {
        const outcome = await save(nextSettings(saved, change));
        if (outcome.ok) {
          saved = outcome.settings;
        }
        return outcome;
      });
      previous = result.catch(() => undefined);
      return result;
    },
  };
}

export interface Feedback {
  thresholdError: boolean;
  unsaved: readonly string[];
}

export const NO_FEEDBACK: Feedback = { thresholdError: false, unsaved: [] };

function changedControl(change: SettingsChange): string {
  return change.field === "excluded"
    ? `excluded:${change.bundleId}`
    : change.field;
}

export function nextFeedback(
  feedback: Feedback,
  change: SettingsChange,
  result: SaveSettingsResult,
): Feedback {
  const control = changedControl(change);
  if (result.ok) {
    return {
      thresholdError:
        change.field === "thresholds" ? false : feedback.thresholdError,
      unsaved: feedback.unsaved.filter((other) => other !== control),
    };
  }
  if (result.error.kind === "invalid_settings" && change.field === "thresholds") {
    return { ...feedback, thresholdError: true };
  }
  return {
    ...feedback,
    unsaved: [...new Set([...feedback.unsaved, control])].sort(),
  };
}

export interface ExclusionRow {
  bundleId: BundleId;
  label: string;
  excluded: boolean;
  place: "dock" | "notInDock" | "unknown";
}

export function exclusionRows(
  dockApps: readonly DockAppName[] | null,
  excluded: readonly BundleId[],
): ExclusionRow[] {
  const excludedSet = new Set(excluded);
  const docked = (dockApps ?? []).map(
    (app): ExclusionRow => ({
      bundleId: app.bundleId,
      label: app.name,
      excluded: excludedSet.has(app.bundleId),
      place: "dock",
    }),
  );
  const inDock = new Set(docked.map((row) => row.bundleId));
  const strayPlace = dockApps === null ? "unknown" : "notInDock";
  const strays = [...excludedSet]
    .filter((bundleId) => !inDock.has(bundleId))
    .sort()
    .map(
      (bundleId): ExclusionRow => ({
        bundleId,
        label: bundleId,
        excluded: true,
        place: strayPlace,
      }),
    );
  return [...docked, ...strays];
}

export function enteredDays(valueAsNumber: number): number | null {
  return Number.isNaN(valueAsNumber) ? null : valueAsNumber;
}

export function thresholdMessage(limits: SettingsLimits): string {
  return `Days must rise from Fuzz to Patchy to Overgrown, from ${limits.minFuzzDays} to ${limits.maxOvergrownDays}.`;
}

const FAST_FORWARD_SPEEDS: readonly FastForward[] = [1, 10, 100, 1000];

export function toFastForward(value: string): FastForward | null {
  return FAST_FORWARD_SPEEDS.find((speed) => String(speed) === value) ?? null;
}
