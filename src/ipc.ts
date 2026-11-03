// Every invoke call goes through this module. The types mirror the Rust
// command signatures in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";

/** `BundleId` in Rust: a non-empty bundle identifier. */
export type BundleId = string & { readonly __brand: "BundleId" };

/** `geometry::LogicalRect`: overlay-local logical pixels, top-left origin. */
export interface LogicalRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** `dock::DockItem`. */
export interface DockItem {
  bundleId: BundleId;
  appPath: string;
  rect: LogicalRect;
}

/** The `kind` values of `crate::Error`. */
export type ErrorKind = "tauri" | "no_main_display" | "overlay" | "dock";

/** `crate::Error` as it reaches the webview. */
export interface CommandError {
  kind: ErrorKind;
  message: string;
}

/** Narrows a rejected `invoke` to `CommandError`. Tauri itself rejects with
 * a plain string, for example when a permission is missing. */
export function isCommandError(error: unknown): error is CommandError {
  return (
    typeof error === "object" &&
    error !== null &&
    typeof (error as Record<string, unknown>).kind === "string" &&
    typeof (error as Record<string, unknown>).message === "string"
  );
}

export function dockItems(): Promise<DockItem[]> {
  return invoke<DockItem[]>("dock_items");
}
