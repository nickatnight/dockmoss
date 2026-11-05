import { describe, expect, it } from "vitest";

import type { BundleId, Settings, SettingsLimits } from "../ipc";
import { nextSettings, thresholdMessage, toFastForward } from "./model";

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
