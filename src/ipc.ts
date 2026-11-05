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

// A test in error.rs fails when this list and Error::kind() disagree.
export type ErrorKind =
  | "tauri"
  | "no_main_display"
  | "overlay"
  | "dock"
  | "tracker_stopped"
  | "usage_stopped"
  | "invalid_settings"
  | "settings_not_saved"
  | "login_item_not_changed";

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

export type EnteredThresholds = {
  [Stage in keyof Thresholds]: number | null;
};

export type SettingsInput = Omit<Settings, "thresholds"> & {
  thresholds: EnteredThresholds;
};

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
  dockApps: DockAppName[] | null;
  /** From macOS; null when it did not answer in time. */
  launchAtLogin: boolean | null;
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

export type SaveSettingsError =
  | { kind: "invalid_settings"; message: string }
  | { kind: "settings_not_saved"; message: string }
  | { kind: "ipc_failed"; cause: unknown };

export type SaveSettingsResult =
  | { ok: true; settings: Settings }
  | { ok: false; error: SaveSettingsError };

export function saveSettingsError(error: unknown): SaveSettingsError {
  if (
    isCommandError(error) &&
    (error.kind === "invalid_settings" || error.kind === "settings_not_saved")
  ) {
    return { kind: error.kind, message: error.message };
  }
  return { kind: "ipc_failed", cause: error };
}

export async function saveSettings(
  settings: SettingsInput,
): Promise<SaveSettingsResult> {
  try {
    const saved = await invoke<Settings>("save_settings", { settings });
    return { ok: true, settings: saved };
  } catch (error) {
    return { ok: false, error: saveSettingsError(error) };
  }
}

export type LaunchAtLoginResult =
  | { ok: true; enabled: boolean }
  | {
      ok: false;
      error:
        | { kind: "login_item_not_changed"; message: string }
        | { kind: "ipc_failed"; cause: unknown };
    };

/** Turns launch at login on or off. `enabled` is what macOS reports after. */
export async function setLaunchAtLogin(
  enabled: boolean,
): Promise<LaunchAtLoginResult> {
  try {
    const now = await invoke<boolean>("set_launch_at_login", { enabled });
    return { ok: true, enabled: now };
  } catch (error) {
    if (isCommandError(error) && error.kind === "login_item_not_changed") {
      return { ok: false, error: { kind: error.kind, message: error.message } };
    }
    return { ok: false, error: { kind: "ipc_failed", cause: error } };
  }
}
