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
  appPath: string;
  rect: LogicalRect;
}

// Mirrors Error::kind() in src-tauri/src/error.rs.
export type ErrorKind =
  | "tauri"
  | "no_main_display"
  | "overlay"
  | "dock"
  | "tracker_stopped";

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
