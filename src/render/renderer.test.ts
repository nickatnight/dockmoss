import { describe, expect, it } from "vitest";

import type { AppGrowth, BundleId, DockItem } from "../ipc";
import type { MossGl } from "./gl";
import type { MossInstance } from "./model";
import { createRenderer } from "./renderer";

type Call =
  | { op: "resize"; width: number; height: number }
  | { op: "draw"; instances: readonly MossInstance[]; time: number }
  | { op: "clear" };

function fakeGls() {
  const created: Call[][] = [];
  const create = (): MossGl => {
    const calls: Call[] = [];
    created.push(calls);
    return {
      resize: (width, height) => {
        calls.push({ op: "resize", width, height });
      },
      draw: (instances, time) => {
        calls.push({ op: "draw", instances, time });
      },
      clear: () => {
        calls.push({ op: "clear" });
      },
    };
  };
  return { created, create };
}

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

function setup(sway = false) {
  const gls = fakeGls();
  const frames = manualScheduler();
  const renderer = createRenderer({
    schedule: frames.schedule,
    createGl: gls.create,
    overgrowthOverride: null,
    sway,
  });
  renderer.resize(1512, 982, 2);
  const calls = (): Call[] => gls.created[gls.created.length - 1] ?? [];
  const ops = () => calls().map((call) => call.op);
  const draws = () => calls().flatMap((call) => (call.op === "draw" ? [call.instances] : []));
  return { renderer, frames, gls, calls, ops, draws };
}

const discord: DockItem = {
  bundleId: "com.hnc.Discord" as BundleId,
  rect: { x: 617, y: 918, width: 47, height: 59 },
};

const mossy: AppGrowth[] = [
  { bundleId: "com.hnc.Discord" as BundleId, overgrowth: 0.6, stage: "patchy" },
];

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
    const { renderer, frames, calls, draws } = drawn();

    renderer.resize(1920, 1080, 1);
    frames.runFrame();

    expect(calls()).toContainEqual({ op: "resize", width: 1920, height: 1080 });
    const artBottom = 918 + (59 + 0.79 * 47) / 2;
    expect(draws()).toHaveLength(1);
    expect(draws()[0][0].box.y).toBeCloseTo(1080 - artBottom, 6);
  });

  it("redraws on a new context after the old one is lost and restored", () => {
    const { renderer, frames, gls } = drawn();

    renderer.contextLost();
    renderer.dock([{ ...discord, rect: { ...discord.rect, x: 620 } }]);
    expect(frames.pending).toBe(0);
    renderer.contextRestored();
    frames.runFrame();

    expect(gls.created).toHaveLength(2);
    expect(gls.created[1].map((call) => call.op)).toContain("draw");
  });

  it("draws every mossy icon in one draw, and no clean one", () => {
    const { renderer, frames, draws } = setup();
    const telegram = { bundleId: "ru.keepcoder.Telegram" as BundleId, rect: { ...discord.rect, x: 664 } };
    const finder = { bundleId: "com.apple.finder" as BundleId, rect: { ...discord.rect, x: 476 } };

    renderer.dock([finder, discord, telegram]);
    renderer.growth([...mossy, { bundleId: telegram.bundleId, overgrowth: 1, stage: "overgrown" }]);
    frames.runFrame();

    expect(draws()).toHaveLength(1);
    expect(draws()[0].map(({ overgrowth }) => overgrowth)).toEqual([0.6015625, 1]);
  });

  it("draws nothing, and schedules nothing, for a clean Dock whose tiles move", () => {
    const { renderer, frames, ops } = setup();
    renderer.growth([{ bundleId: discord.bundleId, overgrowth: 0, stage: "clean" }]);
    renderer.dock([discord]);
    frames.runFrame();
    const before = ops().length;

    renderer.dock([{ ...discord, rect: { ...discord.rect, x: 600, width: 60 } }]);
    renderer.dock([{ ...discord, rect: { ...discord.rect, x: 590, width: 66 } }]);

    expect(frames.pending).toBe(0);
    expect(ops().slice(before)).toEqual([]);
  });

  it("does nothing while the scene is unchanged", () => {
    const { renderer, frames, ops } = drawn();

    renderer.dock([discord]);
    renderer.growth(mossy);
    frames.runFrame();

    expect(ops()).toEqual([]);
  });
});

describe("the sway loop", () => {
  const patchy = (overgrowth: number): AppGrowth[] => [
    { bundleId: discord.bundleId, overgrowth, stage: overgrowth >= 0.5 ? "patchy" : "fuzz" },
  ];

  function runFrames(env: ReturnType<typeof setup>, count: number, start = 0): number[] {
    const before = env.calls().length;
    for (let index = 0; index < count; index++) {
      env.frames.runFrame(start + (index * 1000) / 60);
    }
    return env.calls().slice(before).flatMap((call) => (call.op === "draw" ? [call.time] : []));
  }

  it("does not run with sway off, even with a patchy icon", () => {
    const env = setup(false);
    env.renderer.dock([discord]);
    env.renderer.growth(patchy(0.8));

    expect(runFrames(env, 1)).toHaveLength(1);
    expect(env.frames.pending).toBe(0);
  });

  it("does not run with sway on while every icon is below patchy", () => {
    const env = setup(true);
    env.renderer.dock([discord]);
    env.renderer.growth(patchy(0.45));

    expect(runFrames(env, 1)).toHaveLength(1);
    expect(env.frames.pending).toBe(0);
  });

  it("runs with sway on and an icon at patchy, at 30 frames a second", () => {
    const env = setup(true);
    env.renderer.dock([discord]);
    env.renderer.growth(patchy(0.5));

    const drawTimes = runFrames(env, 60, 1000);

    expect(env.frames.pending).toBe(1);
    expect(drawTimes).toHaveLength(30);
    for (let index = 1; index < drawTimes.length; index++) {
      expect(drawTimes[index] - drawTimes[index - 1]).toBeGreaterThanOrEqual(1 / 30 - 1e-9);
    }
  });

  it("stops scheduling once the last icon drops below patchy", () => {
    const env = setup(true);
    env.renderer.dock([discord]);
    env.renderer.growth(patchy(0.9));
    runFrames(env, 10);

    env.renderer.growth(patchy(0.3));
    runFrames(env, 1, 500);

    expect(env.frames.pending).toBe(0);
    expect(runFrames(env, 5, 600)).toEqual([]);
  });

  it("stops at once when the Dock empties", () => {
    const env = setup(true);
    env.renderer.dock([discord]);
    env.renderer.growth(patchy(0.9));
    runFrames(env, 3);

    env.renderer.dock([]);
    runFrames(env, 1, 100);

    expect(env.frames.pending).toBe(0);
  });

  it("keeps the last sway pose for a draw after the loop stops", () => {
    const env = setup(true);
    env.renderer.dock([discord]);
    env.renderer.growth(patchy(0.9));
    const swayed = runFrames(env, 4, 2000);
    env.renderer.growth(patchy(0.3));

    const settled = runFrames(env, 1, 3000);

    expect(settled).toEqual([swayed[swayed.length - 1]]);
  });
});
