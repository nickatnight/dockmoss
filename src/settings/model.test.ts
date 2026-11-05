import { describe, expect, it } from "vitest";

import type {
  BundleId,
  DockAppName,
  Settings,
  SettingsLimits,
} from "../ipc";
import {
  exclusionRows,
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

  it("passes an invalid threshold through for Rust to reject", () => {
    const next = nextSettings(saved, {
      field: "thresholds",
      thresholds: { fuzz: Number.NaN, patchy: 10, overgrown: 30 },
    });

    expect(next.thresholds.fuzz).toBeNaN();
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
