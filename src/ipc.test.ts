import { describe, expect, it } from "vitest";

import { saveSettingsError } from "./ipc";

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
