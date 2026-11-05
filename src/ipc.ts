import { Channel, invoke } from "@tauri-apps/api/core";

export type BundleId = string & { readonly __brand: "BundleId" };

export interface LogicalRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface DockItem {
  bundleId: BundleId;
  rect: LogicalRect;
}

/** Mirrors `usage::AppGrowth`: one entry per Dock app, in Dock order. */
export interface AppGrowth {
  bundleId: BundleId;
  /** 0 to 1. */
  overgrowth: number;
  stage: "clean" | "fuzz" | "patchy" | "overgrown";
}

// error/tests.rs fails when this list and Error::kind() disagree.
export type ErrorKind =
  | "tauri"
  | "no_main_display"
  | "overlay"
  | "dock"
  | "tracker_stopped"
  | "usage_stopped"
  | "invalid_settings"
  | "settings_not_saved";

export interface Thresholds {
  fuzz: number;
  patchy: number;
  overgrown: number;
}

export type FastForward = 1 | 10 | 100 | 1000;

export interface Settings {
  thresholds: Thresholds;
  intensityPercent: number;
  excluded: BundleId[];
  fastForward: FastForward;
}

export interface SettingsLimits {
  minFuzzDays: number;
  maxOvergrownDays: number;
  maxExclusions: number;
}

export interface DockAppName {
  bundleId: BundleId;
  name: string;
}

export interface SettingsView {
  settings: Settings;
  limits: SettingsLimits;
  dockApps: DockAppName[];
}

export interface CommandError {
  kind: ErrorKind;
  message: string;
}

export function isCommandError(error: unknown): error is CommandError {
  return (
    typeof error === "object" &&
    error !== null &&
    "kind" in error &&
    typeof error.kind === "string" &&
    "message" in error &&
    typeof error.message === "string"
  );
}

/** Calls `onItems` with the current Dock items, then again on every change. */
export function watchDock(
  onItems: (items: DockItem[]) => void,
): Promise<void> {
  const channel = new Channel<DockItem[]>();
  channel.onmessage = onItems;
  return invoke<void>("watch_dock", { onItems: channel });
}

/** Calls `onGrowth` with every Dock app's growth, then again on every change. */
export function watchGrowth(
  onGrowth: (growth: AppGrowth[]) => void,
): Promise<void> {
  const channel = new Channel<AppGrowth[]>();
  channel.onmessage = onGrowth;
  return invoke<void>("watch_growth", { onGrowth: channel });
}

export function getSettings(): Promise<SettingsView> {
  return invoke<SettingsView>("get_settings");
}

export function saveSettings(settings: Settings): Promise<Settings> {
  return invoke<Settings>("save_settings", { settings });
}
