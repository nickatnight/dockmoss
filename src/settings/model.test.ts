import { describe, expect, it } from "vitest";

import type {
  BundleId,
  DockAppName,
  LaunchAtLoginResult,
  SaveSettingsResult,
  Settings,
  SettingsInput,
  SettingsLimits,
} from "../ipc";
import {
  NO_FEEDBACK,
  createLoginToggle,
  createSettingsSaver,
  enteredDays,
  exclusionLimitMessage,
  exclusionRows,
  invalidDays,
  launchAtLoginBox,
  launchAtLoginShown,
  nextFeedback,
  nextSettings,
  announceLoginNote,
  reannounced,
  saveNotice,
  thresholdMessage,
  toFastForward,
  type Outcome,
  type PageChange,
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
      "Use whole days from 2 to 3650, each stage longer than the one before.",
    );
  });

  it("follows the limits rather than restating them", () => {
    const limits: SettingsLimits = {
      minFuzzDays: 3,
      maxOvergrownDays: 400,
      maxExclusions: 10,
    };

    expect(thresholdMessage(limits)).toBe(
      "Use whole days from 3 to 400, each stage longer than the one before.",
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

  it("builds the next save from Rust's reply, not from what was sent", async () => {
    const { calls, save, settle } = manualSave();
    const saver = createSettingsSaver(saved, save);

    void saver.commit({ field: "intensityPercent", percent: 40 });
    void saver.commit({ field: "fastForward", speed: 100 });
    await new Promise((resolve) => setTimeout(resolve, 0));
    await settle({
      ok: true,
      settings: { ...saved, intensityPercent: 40, excluded: [] },
    });

    expect(calls[1]?.excluded).toEqual([]);
    expect(calls[1]?.fastForward).toBe(100);
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

  it("gives a save that rejects an ipc_failed result, and keeps saving", async () => {
    const gone = new Error("bridge gone");
    const calls: SettingsInput[] = [];
    const saver = createSettingsSaver(saved, (settings) => {
      calls.push(settings);
      return calls.length === 1 ? Promise.reject(gone) : Promise.resolve(stored(settings));
    });

    const first = saver.commit({ field: "fastForward", speed: 10 });
    const second = saver.commit({ field: "fastForward", speed: 100 });

    expect(await first).toEqual({ ok: false, error: { kind: "ipc_failed", cause: gone } });
    expect(await second).toEqual({ ok: true, settings: { ...saved, fastForward: 100 } });
    expect(calls).toHaveLength(2);
  });

  it("gives a save that throws before it returns a promise an ipc_failed result, and keeps saving", async () => {
    const gone = new Error("bridge gone");
    const calls: SettingsInput[] = [];
    const saver = createSettingsSaver(saved, (settings) => {
      calls.push(settings);
      if (calls.length === 1) {
        throw gone;
      }
      return Promise.resolve(stored(settings));
    });

    const first = saver.commit({ field: "fastForward", speed: 10 });
    const second = saver.commit({ field: "fastForward", speed: 100 });

    expect(await first).toEqual({ ok: false, error: { kind: "ipc_failed", cause: gone } });
    expect(await second).toEqual({ ok: true, settings: { ...saved, fastForward: 100 } });
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

    expect(feedback).toEqual({
      thresholdError: true,
      exclusionLimit: false,
      unsaved: [],
      loginNotChanged: false,
    });
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

    expect(feedback).toEqual({
      thresholdError: false,
      exclusionLimit: false,
      unsaved: ["thresholds"],
      loginNotChanged: false,
    });
  });

  it("keeps the save notice until the control that failed saves, not another one", () => {
    const failed = nextFeedback(NO_FEEDBACK, intensity, notSaved);
    const otherSaved = nextFeedback(failed, speed, ok);
    const retried = nextFeedback(otherSaved, intensity, ok);

    expect(otherSaved.unsaved).toEqual(["intensityPercent"]);
    expect(retried.unsaved).toEqual([]);
  });

  it.each([
    ["settings_not_saved", notSaved],
    ["ipc_failed", ipcFailed],
  ])("keeps a threshold message already shown when a save fails with %s", (_kind, result) => {
    const shown = nextFeedback(NO_FEEDBACK, fuzz1, invalid);

    expect(nextFeedback(shown, intensity, result)).toEqual({
      thresholdError: true,
      exclusionLimit: false,
      unsaved: ["intensityPercent"],
      loginNotChanged: false,
    });
  });

  it("clears the threshold message when a corrected entry passes the rules but is not written", () => {
    const shown = nextFeedback(NO_FEEDBACK, fuzz1, invalid);
    const fuzz3 = {
      field: "thresholds",
      thresholds: { fuzz: 3, patchy: 10, overgrown: 30 },
    } as const;

    expect(nextFeedback(shown, fuzz3, notSaved)).toEqual({
      thresholdError: false,
      exclusionLimit: false,
      unsaved: ["thresholds"],
      loginNotChanged: false,
    });
  });

  it("keeps the threshold message when a threshold save gets no reply, since the entry was not checked", () => {
    const shown = nextFeedback(NO_FEEDBACK, fuzz1, invalid);

    expect(nextFeedback(shown, fuzz1, ipcFailed).thresholdError).toBe(true);
  });

  it("keeps another control's save notice when thresholds are rejected", () => {
    const failed = nextFeedback(NO_FEEDBACK, intensity, notSaved);

    expect(nextFeedback(failed, fuzz1, invalid)).toEqual({
      thresholdError: true,
      exclusionLimit: false,
      unsaved: ["intensityPercent"],
      loginNotChanged: false,
    });
  });

  it("states the exclusion limit, not the threshold message or 'try again', when an exclusion is refused", () => {
    expect(nextFeedback(NO_FEEDBACK, exclude("com.a"), invalid)).toEqual({
      thresholdError: false,
      exclusionLimit: true,
      unsaved: [],
      loginNotChanged: false,
    });
  });

  it("clears the exclusion limit once an exclusion saves", () => {
    const refused = nextFeedback(NO_FEEDBACK, exclude("com.a"), invalid);
    const unexcluded = {
      field: "excluded",
      bundleId: id("com.b"),
      excluded: false,
    } as const;

    expect(nextFeedback(refused, unexcluded, ok).exclusionLimit).toBe(false);
    expect(nextFeedback(refused, intensity, ok).exclusionLimit).toBe(true);
  });

  it("keeps the threshold message and another control's save notice when an exclusion is refused", () => {
    const shown = nextFeedback(nextFeedback(NO_FEEDBACK, intensity, notSaved), fuzz1, invalid);

    expect(nextFeedback(shown, exclude("com.a"), invalid)).toEqual({
      thresholdError: true,
      exclusionLimit: true,
      unsaved: ["intensityPercent"],
      loginNotChanged: false,
    });
  });

  it("keeps the exclusion limit when another save fails or thresholds are rejected", () => {
    const refused = nextFeedback(NO_FEEDBACK, exclude("com.a"), invalid);

    expect(nextFeedback(refused, intensity, notSaved).exclusionLimit).toBe(true);
    expect(nextFeedback(refused, fuzz1, invalid).exclusionLimit).toBe(true);
  });

  it("shows the save notice when Rust rejects a field that has no message of its own", () => {
    expect(nextFeedback(NO_FEEDBACK, speed, invalid)).toEqual({
      thresholdError: false,
      exclusionLimit: false,
      unsaved: ["fastForward"],
      loginNotChanged: false,
    });
  });

  it("tracks each excluded app on its own", () => {
    const failed = nextFeedback(NO_FEEDBACK, exclude("com.a"), notSaved);
    const otherSaved = nextFeedback(failed, exclude("com.b"), ok);
    const retried = nextFeedback(otherSaved, exclude("com.a"), ok);

    expect(otherSaved.unsaved).toEqual(["excluded:com.a"]);
    expect(retried.unsaved).toEqual([]);
  });
});

describe("reannounced", () => {
  const invalid: SaveSettingsResult = {
    ok: false,
    error: { kind: "invalid_settings", message: "bad" },
  };
  const ok: SaveSettingsResult = { ok: true, settings: saved };
  const fuzz1 = {
    field: "thresholds",
    thresholds: { fuzz: 1, patchy: 10, overgrown: 30 },
  } as const;
  const exclude = {
    field: "excluded",
    bundleId: id("com.a"),
    excluded: true,
  } as const;
  const intensity = { field: "intensityPercent", percent: 40 } as const;
  const shown = nextFeedback(NO_FEEDBACK, fuzz1, invalid);

  it("re-announces a repeated threshold rejection", () => {
    expect(reannounced(shown, fuzz1, invalid).threshold).toBe(true);
  });

  it("does not re-announce a first rejection, which is announced as it appears", () => {
    expect(reannounced(NO_FEEDBACK, fuzz1, invalid).threshold).toBe(false);
  });

  it("does not re-announce the threshold message when another field saves", () => {
    expect(reannounced(shown, intensity, ok).threshold).toBe(false);
  });

  it("re-announces the save notice when another save fails while it shows", () => {
    const notSaved: SaveSettingsResult = {
      ok: false,
      error: { kind: "settings_not_saved", message: "settings could not be saved" },
    };
    const failed = nextFeedback(NO_FEEDBACK, intensity, notSaved);

    expect(reannounced(failed, exclude, notSaved).saveNotice).toBe(true);
    expect(reannounced(NO_FEEDBACK, intensity, notSaved).saveNotice).toBe(false);
    expect(reannounced(failed, exclude, ok).saveNotice).toBe(false);
  });

  it("re-announces only the message the failed field owns", () => {
    const notSaved: SaveSettingsResult = {
      ok: false,
      error: { kind: "settings_not_saved", message: "settings could not be saved" },
    };
    const all = nextFeedback(nextFeedback(shown, exclude, invalid), intensity, notSaved);

    expect(reannounced(all, fuzz1, invalid)).toEqual({
      threshold: true,
      exclusionLimit: false,
      saveNotice: false,
    });
    expect(reannounced(all, exclude, invalid)).toEqual({
      threshold: false,
      exclusionLimit: true,
      saveNotice: false,
    });
    expect(reannounced(all, fuzz1, notSaved)).toEqual({
      threshold: false,
      exclusionLimit: false,
      saveNotice: true,
    });
    expect(reannounced(all, exclude, notSaved)).toEqual({
      threshold: false,
      exclusionLimit: false,
      saveNotice: true,
    });
  });

  it("re-announces a repeated exclusion refusal", () => {
    const refused = nextFeedback(NO_FEEDBACK, exclude, invalid);

    expect(reannounced(refused, exclude, invalid).exclusionLimit).toBe(true);
    expect(reannounced(NO_FEEDBACK, exclude, invalid).exclusionLimit).toBe(false);
  });
});

describe("exclusionLimitMessage", () => {
  it("states the cap Rust sent", () => {
    expect(
      exclusionLimitMessage({ minFuzzDays: 2, maxOvergrownDays: 3650, maxExclusions: 256 }),
    ).toBe("At most 256 apps can be excluded.");
  });

  it("follows the cap rather than restating it", () => {
    expect(
      exclusionLimitMessage({ minFuzzDays: 2, maxOvergrownDays: 3650, maxExclusions: 300 }),
    ).toBe("At most 300 apps can be excluded.");
  });
});

describe("invalidDays", () => {
  const limits: SettingsLimits = { minFuzzDays: 2, maxOvergrownDays: 3650, maxExclusions: 256 };

  it.each([
    ["fuzz below the minimum", { fuzz: 1, patchy: 10, overgrown: 30 }, ["fuzz"]],
    ["patchy equal to fuzz", { fuzz: 5, patchy: 5, overgrown: 30 }, ["patchy"]],
    ["overgrown equal to patchy", { fuzz: 5, patchy: 30, overgrown: 30 }, ["overgrown"]],
    ["overgrown above the maximum", { fuzz: 5, patchy: 10, overgrown: 3651 }, ["overgrown"]],
    ["an empty fuzz", { fuzz: null, patchy: 10, overgrown: 30 }, ["fuzz"]],
    ["a fractional patchy", { fuzz: 5, patchy: 10.5, overgrown: 30 }, ["patchy"]],
    ["patchy above overgrown", { fuzz: 5, patchy: 40, overgrown: 30 }, ["overgrown"]],
    ["two wrong at once", { fuzz: 1, patchy: 10, overgrown: 4000 }, ["fuzz", "overgrown"]],
    ["fuzz at the minimum, patchy equal to it", { fuzz: 2, patchy: 2, overgrown: 30 }, ["patchy"]],
    [
      "overgrown at the maximum, equal to patchy",
      { fuzz: 5, patchy: 3650, overgrown: 3650 },
      ["overgrown"],
    ],
    [
      "fuzz above the maximum, the rest in order",
      { fuzz: 4000, patchy: 10, overgrown: 30 },
      ["fuzz"],
    ],
  ] as const)("marks only the wrong field: %s", (_case, thresholds, expected) => {
    expect(invalidDays(thresholds, limits)).toEqual(expected);
  });

  it("marks all three when no single field breaks a rule", () => {
    expect(invalidDays({ fuzz: 5, patchy: 10, overgrown: 30 }, limits)).toEqual([
      "fuzz",
      "patchy",
      "overgrown",
    ]);
  });
});

describe("launch at login feedback", () => {
  const turnOn = { field: "launchAtLogin", enabled: true } as const;
  const intensity = { field: "intensityPercent", percent: 40 } as const;
  const notChanged: LaunchAtLoginResult = {
    ok: false,
    error: { kind: "login_item_not_changed", message: "launch at login was not changed" },
  };
  const ipcFailed: LaunchAtLoginResult = {
    ok: false,
    error: { kind: "ipc_failed", cause: "gone" },
  };
  const turnedOn: LaunchAtLoginResult = { ok: true, status: "on" };
  const notSaved: SaveSettingsResult = {
    ok: false,
    error: { kind: "settings_not_saved", message: "settings could not be saved" },
  };

  it.each([
    ["login_item_not_changed", notChanged, true],
    ["ipc_failed", ipcFailed, false],
  ] as const)(
    "shows the save notice after %s, and no field message",
    (_kind, result, loginNotChanged) => {
      expect(nextFeedback(NO_FEEDBACK, turnOn, result)).toEqual({
        thresholdError: false,
        exclusionLimit: false,
        unsaved: ["launchAtLogin"],
        loginNotChanged,
      });
    },
  );

  it("clears its own notice once a change goes through, and no other control's", () => {
    const failed = nextFeedback(NO_FEEDBACK, turnOn, notChanged);
    const bothFailed = nextFeedback(failed, intensity, notSaved);

    expect(nextFeedback(bothFailed, turnOn, turnedOn)).toEqual({
      thresholdError: false,
      exclusionLimit: false,
      unsaved: ["intensityPercent"],
      loginNotChanged: false,
    });
  });

  it("re-announces the save notice when it fails again while the notice shows", () => {
    const failed = nextFeedback(NO_FEEDBACK, turnOn, notChanged);

    expect(reannounced(failed, turnOn, notChanged).saveNotice).toBe(true);
    expect(reannounced(NO_FEEDBACK, turnOn, notChanged).saveNotice).toBe(false);
  });

  it("keeps what macOS said about the login item while another control fails", () => {
    const failed = nextFeedback(NO_FEEDBACK, turnOn, notChanged);

    expect(nextFeedback(failed, intensity, notSaved).loginNotChanged).toBe(true);
  });
});

describe("saveNotice", () => {
  const tryAgain = "This change could not be saved. Try again.";
  const checkLoginItems =
    "macOS did not change launch at login. Check System\u00a0Settings\u00a0›\u00a0General\u00a0›\u00a0Login\u00a0Items.";
  const turnOn = { field: "launchAtLogin", enabled: true } as const;
  const intensity = { field: "intensityPercent", percent: 40 } as const;
  const notChanged: LaunchAtLoginResult = {
    ok: false,
    error: { kind: "login_item_not_changed", message: "launch at login was not changed" },
  };
  const ipcFailed: LaunchAtLoginResult = {
    ok: false,
    error: { kind: "ipc_failed", cause: "gone" },
  };
  const notSaved: SaveSettingsResult = {
    ok: false,
    error: { kind: "settings_not_saved", message: "settings could not be saved" },
  };
  const turnedOn: LaunchAtLoginResult = { ok: true, status: "on" };
  const after = (...steps: [PageChange, Outcome][]) =>
    steps.reduce((feedback, [change, result]) => nextFeedback(feedback, change, result), NO_FEEDBACK);

  it.each([
    ["nothing failed", after(), null],
    ["a setting was not saved: a retry can work", after([intensity, notSaved]), tryAgain],
    [
      "the login change never reached Rust: a retry can work",
      after([turnOn, ipcFailed]),
      tryAgain,
    ],
    [
      "macOS did not change the login item: a retry cannot fix it",
      after([turnOn, notChanged]),
      checkLoginItems,
    ],
    [
      "both: each sentence once",
      after([turnOn, notChanged], [intensity, notSaved]),
      `${tryAgain} ${checkLoginItems}`,
    ],
    [
      "the login item failed again, this time over IPC",
      after([turnOn, notChanged], [turnOn, ipcFailed]),
      tryAgain,
    ],
    [
      "the login item went through after failing",
      after([turnOn, notChanged], [turnOn, turnedOn]),
      null,
    ],
  ] as const)("%s", (_case, feedback, expected) => {
    expect(saveNotice(feedback)).toBe(expected);
  });
});

describe("launchAtLoginShown", () => {
  it.each([
    ["turned on", "off", { ok: true, status: "on" }, "on"],
    ["turned off", "on", { ok: true, status: "off" }, "off"],
    ["on, but macOS reports off", "off", { ok: true, status: "off" }, "off"],
    ["on, waiting for the user's approval", "off", { ok: true, status: "needsApproval" }, "needsApproval"],
    [
      "on refused",
      "off",
      { ok: false, error: { kind: "login_item_not_changed", message: "x" } },
      "off",
    ],
    [
      "off refused",
      "on",
      { ok: false, error: { kind: "login_item_not_changed", message: "x" } },
      "on",
    ],
    ["on, no reply", "off", { ok: false, error: { kind: "ipc_failed", cause: "gone" } }, "off"],
    [
      "a failed change from needs approval keeps needs approval",
      "needsApproval",
      { ok: false, error: { kind: "login_item_not_changed", message: "x" } },
      "needsApproval",
    ],
  ] as const)("%s: the box shows what macOS ends in", (_case, before, result, expected) => {
    expect(launchAtLoginShown(before, result)).toBe(expected);
  });
});

describe("createLoginToggle", () => {
  const ok = (enabled: boolean): LaunchAtLoginResult => ({
    ok: true,
    status: enabled ? "on" : "off",
  });
  const notChanged: LaunchAtLoginResult = {
    ok: false,
    error: { kind: "login_item_not_changed", message: "launch at login was not changed" },
  };
  function fakeSet() {
    const calls: { wanted: boolean; reply: (result: LaunchAtLoginResult) => void }[] = [];
    const set = (wanted: boolean) =>
      new Promise<LaunchAtLoginResult>((reply) => calls.push({ wanted, reply }));
    return { calls, set };
  }
  const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

  it("sets the box from a lone change's result", async () => {
    const { calls, set } = fakeSet();
    const toggle = createLoginToggle(set, "off");

    const done = toggle(true);
    await settle();
    calls[0]?.reply(ok(true));

    expect(await done).toEqual({ result: ok(true), shown: "on" });
  });

  it("runs a second change only after the first has answered", async () => {
    const { calls, set } = fakeSet();
    const toggle = createLoginToggle(set, "off");

    void toggle(true);
    void toggle(false);
    await settle();
    expect(calls.map(({ wanted }) => wanted)).toEqual([true]);

    calls[0]?.reply(ok(true));
    await settle();
    expect(calls.map(({ wanted }) => wanted)).toEqual([true, false]);
  });

  it("leaves the box alone for a result a later change has overtaken, and the last result sets it", async () => {
    const { calls, set } = fakeSet();
    const toggle = createLoginToggle(set, "off");

    const first = toggle(true);
    const second = toggle(false);
    await settle();
    calls[0]?.reply(ok(true));
    await settle();
    calls[1]?.reply(ok(false));

    expect(await first).toEqual({ result: ok(true), shown: null });
    expect(await second).toEqual({ result: ok(false), shown: "off" });
  });

  it("lets the last result set the box even when it fails", async () => {
    const { calls, set } = fakeSet();
    const toggle = createLoginToggle(set, "off");

    const first = toggle(true);
    const second = toggle(false);
    await settle();
    calls[0]?.reply(ok(true));
    await settle();
    calls[1]?.reply(notChanged);

    expect((await first).shown).toBeNull();
    expect(await second).toEqual({ result: notChanged, shown: "on" });
  });

  it("gives a change that throws an ipc_failed result, and the box what macOS kept", async () => {
    const gone = new Error("bridge gone");
    const toggle = createLoginToggle(() => Promise.reject(gone), "off");

    expect(await toggle(true)).toEqual({
      result: { ok: false, error: { kind: "ipc_failed", cause: gone } },
      shown: "off",
    });
  });

  it("gives a set that throws before it returns a promise an ipc_failed result, and runs the next change", async () => {
    const gone = new Error("bridge gone");
    const wanted: boolean[] = [];
    const toggle = createLoginToggle((change) => {
      wanted.push(change);
      if (wanted.length === 1) {
        throw gone;
      }
      return Promise.resolve(ok(change));
    }, "off");

    const first = toggle(true);
    const second = toggle(false);

    expect(await first).toEqual({
      result: { ok: false, error: { kind: "ipc_failed", cause: gone } },
      shown: null,
    });
    expect(await second).toEqual({ result: ok(false), shown: "off" });
    expect(wanted).toEqual([true, false]);
  });

  it("gives a lone set that throws before it returns a promise an ipc_failed result, and the box what macOS kept", async () => {
    const gone = new Error("bridge gone");
    const toggle = createLoginToggle(() => {
      throw gone;
    }, "off");

    expect(await toggle(true)).toEqual({
      result: { ok: false, error: { kind: "ipc_failed", cause: gone } },
      shown: "off",
    });
  });

  it("keeps running changes after a change throws, and an overtaken throw leaves the box alone", async () => {
    const gone = new Error("bridge gone");
    const wanted: boolean[] = [];
    const toggle = createLoginToggle((change) => {
      wanted.push(change);
      return wanted.length === 1 ? Promise.reject(gone) : Promise.resolve(ok(change));
    }, "off");

    const first = toggle(true);
    const second = toggle(false);

    expect(await first).toEqual({
      result: { ok: false, error: { kind: "ipc_failed", cause: gone } },
      shown: null,
    });
    expect(await second).toEqual({ result: ok(false), shown: "off" });
    expect(wanted).toEqual([true, false]);
  });

  it("keeps the approval note when a change made from needs approval fails", async () => {
    const toggle = createLoginToggle(() => Promise.resolve(notChanged), "needsApproval");

    expect((await toggle(true)).shown).toBe("needsApproval");
  });

  it("shows the status of the last change that went through when a later one fails", async () => {
    const replies = [ok(true), notChanged];
    const toggle = createLoginToggle(() => Promise.resolve(replies.shift() ?? notChanged), "off");

    expect((await toggle(true)).shown).toBe("on");
    expect((await toggle(false)).shown).toBe("on");
  });

  it("sets the box again for a change made after the last one answered", async () => {
    const { calls, set } = fakeSet();
    const toggle = createLoginToggle(set, "off");

    const first = toggle(true);
    await settle();
    calls[0]?.reply(ok(true));
    expect((await first).shown).toBe("on");

    const second = toggle(false);
    await settle();
    calls[1]?.reply(ok(false));
    expect((await second).shown).toBe("off");
  });
});

describe("announceLoginNote", () => {
  it.each([
    ["a change that ends waiting for approval: say so, even when the note already shows", { ok: true, status: "needsApproval" }, true],
    ["a change that ends on: no note to announce", { ok: true, status: "on" }, false],
    ["a change that ends off: no note to announce", { ok: true, status: "off" }, false],
    [
      "a refused change: the red notice speaks, not the note",
      { ok: false, error: { kind: "login_item_not_changed", message: "x" } },
      false,
    ],
    ["no reply: the red notice speaks", { ok: false, error: { kind: "ipc_failed", cause: "gone" } }, false],
  ] as const)("%s", (_case, result, expected) => {
    expect(announceLoginNote(result)).toBe(expected);
  });
});

describe("launchAtLoginBox", () => {
  it.each([
    ["on", "on", { checked: true, disabled: false, note: null }],
    ["off", "off", { checked: false, disabled: false, note: null }],
    [
      "waiting for the user's approval: off, and the note says where to approve it",
      "needsApproval",
      {
        checked: false,
        disabled: false,
        note: "DockMoss needs your approval in System\u00a0Settings\u00a0›\u00a0General\u00a0›\u00a0Login\u00a0Items.",
      },
    ],
    [
      "unknown: macOS did not answer in time, and the note says why",
      null,
      {
        checked: false,
        disabled: true,
        note: "macOS did not say whether this is on. To ask again, close and reopen Settings.",
      },
    ],
  ] as const)("%s", (_case, status, expected) => {
    expect(launchAtLoginBox(status)).toEqual(expected);
  });
});
