// The settings form's pure parts: no DOM and no IPC.

import type {
  FastForward,
  Settings,
  SettingsLimits,
  Thresholds,
} from "../ipc";

/** One committed change in the form. Each change replaces one field. */
export type SettingsChange =
  | { field: "thresholds"; thresholds: Thresholds }
  | { field: "intensityPercent"; percent: number }
  | { field: "fastForward"; speed: FastForward };

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
  }
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
