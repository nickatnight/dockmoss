import { describe, expect, it } from "vitest";

import type { AppGrowth, BundleId, DockItem } from "../ipc";
import type { MossGl } from "./gl";
import type { DeviceBox } from "./model";
import { createRenderer } from "./renderer";

type Call =
  | { op: "resize"; width: number; height: number }
  | { op: "setBox"; box: DeviceBox }
  | { op: "setMoss"; seed: number; overgrowth: number }
  | { op: "draw" }
  | { op: "clear" };

/** A MossGl that records its calls, and a list of every one created. */
function fakeGls() {
  const created: Call[][] = [];
  const create = (): MossGl => {
    const calls: Call[] = [];
    created.push(calls);
    return {
      resize: (width, height) => calls.push({ op: "resize", width, height }),
      setBox: (box) => calls.push({ op: "setBox", box }),
      setMoss: (seed, overgrowth) => calls.push({ op: "setMoss", seed, overgrowth }),
      draw: () => calls.push({ op: "draw" }),
      clear: () => calls.push({ op: "clear" }),
    };
  };
  return { created, create };
}

/** Holds scheduled callbacks until the test runs the next frame. */
function manualScheduler() {
  let pending: ((time: number) => void)[] = [];
  return {
    schedule: (callback: (time: number) => void) => {
      pending.push(callback);
    },
    get pending() {
      return pending.length;
    },
    runFrame(time = 0) {
      const due = pending;
      pending = [];
      for (const callback of due) callback(time);
    },
  };
}

function setup() {
  const gls = fakeGls();
  const frames = manualScheduler();
  const renderer = createRenderer({
    schedule: frames.schedule,
    createGl: gls.create,
    overgrowthOverride: null,
  });
  renderer.resize(1512, 982, 2);
  const calls = (): Call[] => gls.created[gls.created.length - 1] ?? [];
  const ops = () => calls().map((call) => call.op);
  return { renderer, frames, gls, calls, ops };
}

const discord: DockItem = {
  bundleId: "com.hnc.Discord" as BundleId,
  rect: { x: 617, y: 918, width: 47, height: 59 },
};

const mossy: AppGrowth[] = [
  { bundleId: "com.hnc.Discord" as BundleId, overgrowth: 0.6, stage: "patchy" },
];

/** A renderer that has drawn Discord's moss once, with its call log emptied. */
function drawn() {
  const env = setup();
  env.renderer.dock([discord]);
  env.renderer.growth(mossy);
  env.frames.runFrame();
  env.calls().length = 0;
  return env;
}

describe("createRenderer", () => {
  it("draws a scene once in the next frame, not in the channel callback", () => {
    const { renderer, frames, ops } = setup();

    renderer.dock([discord]);
    renderer.growth(mossy);

    expect(ops()).not.toContain("draw");
    expect(frames.pending).toBe(1);
    frames.runFrame();
    expect(ops().filter((op) => op === "draw")).toHaveLength(1);
  });

  it("clears at once for an empty Dock frame, and schedules nothing", () => {
    const { renderer, frames, ops } = drawn();

    renderer.dock([]);

    expect(ops()).toEqual(["clear"]);
    expect(frames.pending).toBe(0);
  });

  it("gives two updates before one frame a single draw", () => {
    const { renderer, frames, ops } = drawn();

    renderer.dock([{ ...discord, rect: { ...discord.rect, x: 620 } }]);
    renderer.dock([{ ...discord, rect: { ...discord.rect, x: 624 } }]);

    expect(frames.pending).toBe(1);
    frames.runFrame();
    expect(ops().filter((op) => op === "draw")).toHaveLength(1);
  });

  it("redraws the same scene after a resize, at the new size", () => {
    const { renderer, frames, calls } = drawn();

    renderer.resize(1920, 1080, 1);
    frames.runFrame();

    expect(calls()).toContainEqual({ op: "resize", width: 1920, height: 1080 });
    expect(calls()[calls().length - 1]).toEqual({ op: "draw" });
    // The art's bottom edge in CSS px, measured up from the new canvas bottom.
    const artBottom = 918 + (59 + 0.79 * 47) / 2;
    const boxes = calls().flatMap((call) => (call.op === "setBox" ? [call.box] : []));
    expect(boxes).toHaveLength(1);
    expect(boxes[0].y).toBeCloseTo(1080 - artBottom, 6);
  });

  it("redraws on a new context after the old one is lost and restored", () => {
    const { renderer, frames, gls } = drawn();

    renderer.contextLost();
    renderer.dock([{ ...discord, rect: { ...discord.rect, x: 620 } }]);
    expect(frames.pending).toBe(0);
    renderer.contextRestored();
    frames.runFrame();

    expect(gls.created).toHaveLength(2);
    expect(gls.created[1][gls.created[1].length - 1]).toEqual({ op: "draw" });
  });

  it("does nothing while the scene is unchanged", () => {
    const { renderer, frames, ops } = drawn();

    renderer.dock([discord]);
    renderer.growth(mossy);
    frames.runFrame();

    expect(ops()).toEqual([]);
  });
});
