import { describe, expect, it } from "vitest";

import type {
  BundleId,
  DockAppName,
  SaveSettingsResult,
  Settings,
  SettingsInput,
  SettingsLimits,
} from "../ipc";
import {
  NO_FEEDBACK,
  createSettingsSaver,
  enteredDays,
  exclusionRows,
  nextFeedback,
  nextSettings,
  thresholdMessage,
  toFastForward,
} from "./model";

const id = (value: string): BundleId => value as BundleId;

const saved: Settings = {
  thresholds: { fuzz: 5, patchy: 10, overgrown: 30 },
  intensityPercent: 100,
  excluded: ["com.apple.Chess" as BundleId],
  fastForward: 1,
};

describe("nextSettings", () => {
  it("replaces the thresholds and keeps every other saved field", () => {
    const next = nextSettings(saved, {
      field: "thresholds",
      thresholds: { fuzz: 3, patchy: 7, overgrown: 60 },
    });

    expect(next).toEqual({
      ...saved,
      thresholds: { fuzz: 3, patchy: 7, overgrown: 60 },
    });
  });

  it("replaces the intensity and keeps every other saved field", () => {
    const next = nextSettings(saved, {
      field: "intensityPercent",
      percent: 40,
    });

    expect(next).toEqual({ ...saved, intensityPercent: 40 });
  });

  it("replaces the fast-forward speed and keeps every other saved field", () => {
    const next = nextSettings(saved, { field: "fastForward", speed: 100 });

    expect(next).toEqual({ ...saved, fastForward: 100 });
  });

  it("passes an unreadable threshold through as null for Rust to reject", () => {
    const next = nextSettings(saved, {
      field: "thresholds",
      thresholds: { fuzz: null, patchy: 10, overgrown: 30 },
    });

    expect(next.thresholds).toEqual({ fuzz: null, patchy: 10, overgrown: 30 });
    expect(next.intensityPercent).toBe(100);
  });

  it("adds an exclusion in sorted order and keeps every other saved field", () => {
    const next = nextSettings(saved, {
      field: "excluded",
      bundleId: id("com.apple.Calculator"),
      excluded: true,
    });

    expect(next).toEqual({
      ...saved,
      excluded: [id("com.apple.Calculator"), id("com.apple.Chess")],
    });
  });

  it("lists an app once when it is excluded again", () => {
    const next = nextSettings(saved, {
      field: "excluded",
      bundleId: id("com.apple.Chess"),
      excluded: true,
    });

    expect(next.excluded).toEqual([id("com.apple.Chess")]);
  });

  it("removes an exclusion", () => {
    const next = nextSettings(saved, {
      field: "excluded",
      bundleId: id("com.apple.Chess"),
      excluded: false,
    });

    expect(next).toEqual({ ...saved, excluded: [] });
  });

  it("leaves the saved settings untouched", () => {
    const before = structuredClone(saved);

    nextSettings(saved, { field: "intensityPercent", percent: 0 });

    expect(saved).toEqual(before);
  });
});

describe("thresholdMessage", () => {
  it("states the rule with the limits Rust sent", () => {
    const limits: SettingsLimits = {
      minFuzzDays: 2,
      maxOvergrownDays: 3650,
      maxExclusions: 256,
    };

    expect(thresholdMessage(limits)).toBe(
      "Days must rise from Fuzz to Patchy to Overgrown, from 2 to 3650.",
    );
  });

  it("follows the limits rather than restating them", () => {
    const limits: SettingsLimits = {
      minFuzzDays: 3,
      maxOvergrownDays: 400,
      maxExclusions: 10,
    };

    expect(thresholdMessage(limits)).toBe(
      "Days must rise from Fuzz to Patchy to Overgrown, from 3 to 400.",
    );
  });
});

describe("toFastForward", () => {
  it.each([
    ["1", 1],
    ["10", 10],
    ["100", 100],
    ["1000", 1000],
  ])("reads the radio value %s as a speed", (value, speed) => {
    expect(toFastForward(value)).toBe(speed);
  });

  it.each(["", "2", "1001", "ten"])("rejects %j", (value) => {
    expect(toFastForward(value)).toBeNull();
  });
});

describe("exclusionRows", () => {
  const dockApps: DockAppName[] = [
    { bundleId: id("com.apple.finder"), name: "Finder" },
    { bundleId: id("com.apple.Chess"), name: "Chess" },
    { bundleId: id("com.tinyspeck.slackmacgap"), name: "Slack" },
  ];

  it("lists the Dock's apps by name in Dock order, checked when excluded", () => {
    const rows = exclusionRows(dockApps, [id("com.apple.Chess")]);

    expect(rows).toEqual([
      { bundleId: "com.apple.finder", label: "Finder", excluded: false, place: "dock" },
      { bundleId: "com.apple.Chess", label: "Chess", excluded: true, place: "dock" },
      { bundleId: "com.tinyspeck.slackmacgap", label: "Slack", excluded: false, place: "dock" },
    ]);
  });

  it("follows with excluded apps that left the Dock, by bundle ID, so they can be cleared", () => {
    const rows = exclusionRows(dockApps, [
      id("com.apple.Chess"),
      id("com.example.Gone"),
      id("com.another.Removed"),
    ]);

    expect(rows.slice(3)).toEqual([
      { bundleId: "com.another.Removed", label: "com.another.Removed", excluded: true, place: "notInDock" },
      { bundleId: "com.example.Gone", label: "com.example.Gone", excluded: true, place: "notInDock" },
    ]);
    expect(rows.filter((row) => row.bundleId === "com.apple.Chess")).toHaveLength(1);
  });

  it("lists exclusions by bundle ID, without claiming they left the Dock, while the Dock is unknown", () => {
    expect(exclusionRows(null, [])).toEqual([]);
    expect(exclusionRows(null, [id("com.apple.Chess")])).toEqual([
      { bundleId: "com.apple.Chess", label: "com.apple.Chess", excluded: true, place: "unknown" },
    ]);
  });
});

describe("enteredDays", () => {
  it("reads an empty or unparseable number input as null", () => {
    expect(enteredDays(Number.NaN)).toBeNull();
  });

  it.each([5, 5.5, -1, 0])("passes %d through for Rust to check", (days) => {
    expect(enteredDays(days)).toBe(days);
  });
});

describe("createSettingsSaver", () => {
  function manualSave() {
    const calls: SettingsInput[] = [];
    const pending: ((result: SaveSettingsResult) => void)[] = [];
    const save = (settings: SettingsInput): Promise<SaveSettingsResult> => {
      calls.push(settings);
      return new Promise((resolve) => pending.push(resolve));
    };
    const settle = async (result: SaveSettingsResult): Promise<void> => {
      pending.shift()?.(result);
      await new Promise((resolve) => setTimeout(resolve, 0));
    };
    return { calls, save, settle };
  }

  const stored = (settings: SettingsInput): SaveSettingsResult => ({
    ok: true,
    settings: settings as Settings,
  });

  it("waits for one save to finish before starting the next", async () => {
    const { calls, save, settle } = manualSave();
    const saver = createSettingsSaver(saved, save);

    void saver.commit({ field: "intensityPercent", percent: 40 });
    void saver.commit({ field: "fastForward", speed: 100 });
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(calls).toHaveLength(1);
    await settle(stored(calls[0]));
    expect(calls).toHaveLength(2);
  });

  it("builds each save from what the one before it stored, so quick changes keep each other", async () => {
    const { calls, save, settle } = manualSave();
    const saver = createSettingsSaver(saved, save);

    void saver.commit({ field: "intensityPercent", percent: 40 });
    void saver.commit({ field: "fastForward", speed: 100 });
    await new Promise((resolve) => setTimeout(resolve, 0));
    await settle(stored(calls[0]));

    expect(calls[1]).toEqual({ ...saved, intensityPercent: 40, fastForward: 100 });
  });

  it("builds from the last stored settings after a rejected save", async () => {
    const { calls, save, settle } = manualSave();
    const saver = createSettingsSaver(saved, save);

    void saver.commit({
      field: "thresholds",
      thresholds: { fuzz: 1, patchy: 10, overgrown: 30 },
    });
    void saver.commit({ field: "intensityPercent", percent: 40 });
    await new Promise((resolve) => setTimeout(resolve, 0));
    await settle({
      ok: false,
      error: { kind: "invalid_settings", message: "bad fuzz" },
    });

    expect(calls[1]).toEqual({ ...saved, intensityPercent: 40 });
  });

  it("hands each commit its own result", async () => {
    const { calls, save, settle } = manualSave();
    const saver = createSettingsSaver(saved, save);

    const result = saver.commit({ field: "fastForward", speed: 10 });
    await new Promise((resolve) => setTimeout(resolve, 0));
    await settle(stored(calls[0]));

    await expect(result).resolves.toEqual({
      ok: true,
      settings: { ...saved, fastForward: 10 },
    });
  });

  it("keeps saving after a save throws", async () => {
    const calls: SettingsInput[] = [];
    const saver = createSettingsSaver(saved, (settings) => {
      calls.push(settings);
      return calls.length === 1
        ? Promise.reject(new Error("bridge gone"))
        : Promise.resolve(stored(settings));
    });

    const first = saver.commit({ field: "fastForward", speed: 10 });
    const second = saver.commit({ field: "fastForward", speed: 100 });

    await expect(first).rejects.toThrow("bridge gone");
    await expect(second).resolves.toMatchObject({ ok: true });
    expect(calls).toHaveLength(2);
  });
});

describe("nextFeedback", () => {
  const ok: SaveSettingsResult = { ok: true, settings: saved };
  const invalid: SaveSettingsResult = {
    ok: false,
    error: { kind: "invalid_settings", message: "bad" },
  };
  const notSaved: SaveSettingsResult = {
    ok: false,
    error: { kind: "settings_not_saved", message: "settings could not be saved" },
  };
  const ipcFailed: SaveSettingsResult = {
    ok: false,
    error: { kind: "ipc_failed", cause: "gone" },
  };
  const fuzz1 = {
    field: "thresholds",
    thresholds: { fuzz: 1, patchy: 10, overgrown: 30 },
  } as const;
  const intensity = { field: "intensityPercent", percent: 40 } as const;
  const speed = { field: "fastForward", speed: 10 } as const;
  const exclude = (bundleId: string) =>
    ({ field: "excluded", bundleId: id(bundleId), excluded: true }) as const;

  it("shows the threshold message for rejected thresholds, and no save notice", () => {
    const feedback = nextFeedback(NO_FEEDBACK, fuzz1, invalid);

    expect(feedback).toEqual({ thresholdError: true, unsaved: [] });
  });

  it("clears the threshold message on a valid threshold save", () => {
    const shown = nextFeedback(NO_FEEDBACK, fuzz1, invalid);

    expect(nextFeedback(shown, fuzz1, ok).thresholdError).toBe(false);
  });

  it("keeps the threshold message when another field saves", () => {
    const shown = nextFeedback(NO_FEEDBACK, fuzz1, invalid);

    expect(nextFeedback(shown, intensity, ok).thresholdError).toBe(true);
  });

  it.each([
    ["settings_not_saved", notSaved],
    ["ipc_failed", ipcFailed],
  ])("shows the save notice after %s, leaving the threshold message alone", (_kind, result) => {
    const feedback = nextFeedback(NO_FEEDBACK, fuzz1, result);

    expect(feedback).toEqual({ thresholdError: false, unsaved: ["thresholds"] });
  });

  it("keeps the save notice until the control that failed saves, not another one", () => {
    const failed = nextFeedback(NO_FEEDBACK, intensity, notSaved);
    const otherSaved = nextFeedback(failed, speed, ok);
    const retried = nextFeedback(otherSaved, intensity, ok);

    expect(otherSaved.unsaved).toEqual(["intensityPercent"]);
    expect(retried.unsaved).toEqual([]);
  });

  it("tracks each excluded app on its own", () => {
    const failed = nextFeedback(NO_FEEDBACK, exclude("com.a"), notSaved);
    const otherSaved = nextFeedback(failed, exclude("com.b"), ok);
    const retried = nextFeedback(otherSaved, exclude("com.a"), ok);

    expect(otherSaved.unsaved).toEqual(["excluded:com.a"]);
    expect(retried.unsaved).toEqual([]);
  });
});
