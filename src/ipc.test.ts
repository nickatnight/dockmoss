import { describe, expect, it } from "vitest";

import { launchAtLoginError, saveSettingsError } from "./ipc";

describe("saveSettingsError", () => {
  it("keeps the two save failures Rust names", () => {
    expect(
      saveSettingsError({ kind: "invalid_settings", message: "bad fuzz" }),
    ).toEqual({ kind: "invalid_settings", message: "bad fuzz" });
    expect(
      saveSettingsError({
        kind: "settings_not_saved",
        message: "settings could not be saved",
      }),
    ).toEqual({
      kind: "settings_not_saved",
      message: "settings could not be saved",
    });
  });

  it.each([
    ["another command error", { kind: "tauri", message: "window not found" }],
    ["an IPC rejection string", "save_settings not allowed on window"],
    ["nothing", undefined],
  ])("reports %s as a failed call", (_case, error) => {
    expect(saveSettingsError(error)).toEqual({ kind: "ipc_failed", cause: error });
  });
});

describe("launchAtLoginError", () => {
  it.each([
    [
      "macOS did not change the item",
      { kind: "login_item_not_changed", message: "launch at login was not changed" },
      { kind: "login_item_not_changed", message: "launch at login was not changed" },
    ],
    [
      "macOS did not report the status in time",
      { kind: "login_status_unknown", message: "macOS did not report launch at login in time" },
      { kind: "login_status_unknown", message: "macOS did not report launch at login in time" },
    ],
  ])("keeps %s", (_case, error, expected) => {
    expect(launchAtLoginError(error)).toEqual(expected);
  });

  it.each([
    ["another command error", { kind: "tauri", message: "window not found" }],
    ["an IPC rejection string", "set_launch_at_login not allowed on window"],
  ])("reports %s as a failed call", (_case, error) => {
    expect(launchAtLoginError(error)).toEqual({ kind: "ipc_failed", cause: error });
  });
});
