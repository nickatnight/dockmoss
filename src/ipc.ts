import { invoke } from "@tauri-apps/api/core";

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

export type ErrorKind = "tauri" | "no_main_display" | "overlay" | "dock";

export interface CommandError {
  kind: ErrorKind;
  message: string;
}

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
