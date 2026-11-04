import { describe, expect, it } from "vitest";

import type { AppGrowth, BundleId, DockItem } from "../ipc";
import type { MossGl } from "./gl";
import { type MossInstance, SHRIVEL_MS, artBox, fnv1a32, toDeviceBox } from "./model";
import { createRenderer } from "./renderer";

type Call =
  | { op: "resize"; width: number; height: number }
  | { op: "draw"; instances: readonly MossInstance[]; swaySeconds: number }
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
      draw: (instances, swaySeconds) => {
        calls.push({ op: "draw", instances, swaySeconds });
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
  let scheduled = 0;
  return {
    schedule: (callback: (time: number) => void) => {
      scheduled += 1;
      pending.push(callback);
    },
    get pending() {
      return pending.length;
    },
    get scheduled() {
      return scheduled;
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
    return env.calls().slice(before).flatMap((call) => (call.op === "draw" ? [call.swaySeconds] : []));
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

  it("wraps the sway clock every SWAY_CYCLE_SECONDS", () => {
    const env = setup(true);
    env.renderer.dock([discord]);
    env.renderer.growth(patchy(0.8));

    expect(runFrames(env, 1, 61_000)[0]).toBeCloseTo(1, 6);
  });

  it("draws a restarted loop's first frame at its own time, when it stopped in a frame", () => {
    const env = setup(true);
    env.renderer.dock([discord]);
    env.renderer.growth(patchy(0.9));
    runFrames(env, 1, 2000);
    env.renderer.growth(patchy(0.3));
    runFrames(env, 1, 2010);

    env.renderer.growth(patchy(0.9));

    expect(runFrames(env, 1, 2020)).toEqual([2.02]);
  });

  it("draws a restarted loop's first frame at its own time, when the Dock emptied between frames", () => {
    const env = setup(true);
    env.renderer.dock([discord]);
    env.renderer.growth(patchy(0.9));
    runFrames(env, 1, 2000);
    env.renderer.dock([]);

    env.renderer.dock([discord]);

    expect(runFrames(env, 1, 2020)).toEqual([2.02]);
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

describe("the shrivel", () => {
  const chess: DockItem = {
    bundleId: "com.apple.Chess" as BundleId,
    rect: { x: 664, y: 918, width: 47, height: 59 },
  };
  const chessSeed = fnv1a32("com.apple.Chess#0");
  const overgrownChess: AppGrowth[] = [...mossy, { bundleId: chess.bundleId, overgrowth: 1, stage: "overgrown" }];
  const launchedChess: AppGrowth[] = [...mossy, { bundleId: chess.bundleId, overgrowth: 0, stage: "clean" }];

  function mossyDock(sway = false) {
    const env = setup(sway);
    env.renderer.dock([discord, chess]);
    env.renderer.growth(overgrownChess);
    env.frames.runFrame(500);
    env.calls().length = 0;
    return env;
  }

  type Drawn = { time: number; instances: readonly MossInstance[] };

  function run(env: ReturnType<typeof setup>, start: number, ms: number): Drawn[] {
    const drawnFrames: Drawn[] = [];
    for (let index = 0; index * (1000 / 60) <= ms; index++) {
      const time = start + index * (1000 / 60);
      const before = env.calls().length;
      env.frames.runFrame(time);
      for (const call of env.calls().slice(before)) {
        if (call.op === "draw") drawnFrames.push({ time, instances: call.instances });
      }
    }
    return drawnFrames;
  }

  const chessIn = ({ instances }: Drawn) => instances.find(({ seed }) => seed === chessSeed);

  function shrivelling(env: ReturnType<typeof setup>): void {
    env.renderer.growth(launchedChess);
    const drawnFrames = run(env, 1000, 100);
    expect(chessIn(drawnFrames[drawnFrames.length - 1])?.wither).toBeGreaterThan(0);
  }

  it("starts when a growth frame zeroes a mossy icon, from the icon as last drawn", () => {
    const env = mossyDock();

    env.renderer.growth(launchedChess);

    expect(env.frames.pending).toBe(1);
    const [first] = run(env, 1000, 0);
    expect(chessIn(first)).toMatchObject({ overgrowth: 1, wither: 0 });
    expect(first.instances).toHaveLength(2);
  });

  it("draws at no more than 30 frames a second, withering as it goes", () => {
    const env = mossyDock();
    env.renderer.growth(launchedChess);

    const drawnFrames = run(env, 1000, 1000);

    const times = drawnFrames.map(({ time }) => time);
    for (let index = 1; index < times.length; index++) {
      expect(times[index] - times[index - 1]).toBeGreaterThanOrEqual(1000 / 30 - 1e-9);
    }
    const withers = drawnFrames.flatMap((drawn) => chessIn(drawn)?.wither ?? []);
    expect(withers.length).toBeGreaterThan(10);
    expect(withers[withers.length - 1]).toBe(1);
  });

  it("ends within SHRIVEL_MS with a frame that has no instance for the icon, then schedules nothing", () => {
    const env = mossyDock();
    env.renderer.growth(launchedChess);

    const drawnFrames = run(env, 1000, 1000);

    const last = drawnFrames[drawnFrames.length - 1];
    expect(drawnFrames.length).toBeGreaterThan(10);
    expect(last.time).toBeLessThanOrEqual(1000 + SHRIVEL_MS);
    expect(chessIn(last)).toBeUndefined();
    expect(last.instances.map(({ overgrowth }) => overgrowth)).toEqual([0.6015625]);
    expect(env.frames.pending).toBe(0);
  });

  it("stays idle after it ends: updates that change nothing make no schedule call", () => {
    const env = mossyDock();
    shrivelling(env);
    run(env, 1100, 900);
    const scheduled = env.frames.scheduled;
    const ops = env.calls().length;

    env.renderer.dock([discord, chess]);
    env.renderer.growth(launchedChess);
    env.renderer.growth(launchedChess);

    expect(env.frames.scheduled).toBe(scheduled);
    expect(env.calls().length).toBe(ops);
  });

  it("follows the icon's tile while the Dock magnifies", () => {
    const env = mossyDock();
    shrivelling(env);
    const magnified = { ...chess, rect: { x: 655, y: 905, width: 66, height: 78 } };

    env.renderer.dock([discord, magnified]);
    const [next] = run(env, 1150, 0);

    expect(chessIn(next)?.box).toEqual(toDeviceBox(artBox(magnified.rect), 2, 1964));
  });

  it("draws the new state, and stops, when the app has moss again", () => {
    const env = mossyDock();
    shrivelling(env);

    env.renderer.growth([...mossy, { bundleId: chess.bundleId, overgrowth: 0.3, stage: "fuzz" }]);
    const drawnFrames = run(env, 1150, 500);

    expect(drawnFrames).toHaveLength(1);
    expect(chessIn(drawnFrames[0])).toMatchObject({ overgrowth: 77 / 256, wither: 0 });
    expect(env.frames.pending).toBe(0);
  });

  it("ends when the item leaves the Dock, with one clean frame", () => {
    const env = mossyDock();
    shrivelling(env);

    env.renderer.dock([discord]);
    const drawnFrames = run(env, 1150, 500);

    expect(drawnFrames).toHaveLength(1);
    expect(chessIn(drawnFrames[0])).toBeUndefined();
    expect(env.frames.pending).toBe(0);
  });

  it("ends when the item that alone had moss leaves the Dock, clearing the canvas at once", () => {
    const env = setup();
    env.renderer.dock([chess]);
    env.renderer.growth(overgrownChess);
    env.frames.runFrame(500);
    shrivelling(env);
    env.calls().length = 0;

    env.renderer.dock([{ bundleId: "com.apple.finder" as BundleId, rect: chess.rect }]);
    run(env, 1150, 500);

    expect(env.ops()).toEqual(["clear"]);
    expect(env.frames.pending).toBe(0);
  });

  it("clears at once when the Dock empties, and draws nothing after", () => {
    const env = mossyDock();
    shrivelling(env);
    env.calls().length = 0;

    env.renderer.dock([]);

    expect(env.ops()).toEqual(["clear"]);
    expect(run(env, 1150, 500)).toEqual([]);
    expect(env.frames.pending).toBe(0);
  });

  it("drops every shrivel on context loss, and the restore draws the clean scene once", () => {
    const env = mossyDock();
    shrivelling(env);

    env.renderer.contextLost();
    env.renderer.contextRestored();
    const drawnFrames = run(env, 1150, 500);

    expect(env.gls.created).toHaveLength(2);
    expect(drawnFrames).toHaveLength(1);
    expect(chessIn(drawnFrames[0])).toBeUndefined();
    expect(env.frames.pending).toBe(0);
  });

  it("starts none on the first frame after a context restore", () => {
    const env = mossyDock();
    env.renderer.contextLost();
    env.renderer.growth(launchedChess);
    env.renderer.contextRestored();

    const drawnFrames = run(env, 1000, 500);

    expect(drawnFrames).toHaveLength(1);
    expect(chessIn(drawnFrames[0])).toBeUndefined();
    expect(env.frames.pending).toBe(0);
  });

  it("starts none on the first frame after load", () => {
    const env = setup();
    env.renderer.dock([discord, chess]);
    env.renderer.growth(launchedChess);

    const drawnFrames = run(env, 1000, 500);

    expect(drawnFrames).toHaveLength(1);
    expect(chessIn(drawnFrames[0])).toBeUndefined();
    expect(env.frames.pending).toBe(0);
  });

  it("shrivels two icons at once in the one draw, each to its end", () => {
    const env = mossyDock();

    env.renderer.growth([
      { bundleId: discord.bundleId, overgrowth: 0, stage: "clean" },
      { bundleId: chess.bundleId, overgrowth: 0, stage: "clean" },
    ]);
    const drawnFrames = run(env, 1000, 1000);

    expect(drawnFrames[0].instances).toHaveLength(2);
    expect(drawnFrames[5].instances.map(({ wither }) => wither > 0)).toEqual([true, true]);
    expect(drawnFrames[drawnFrames.length - 1].instances).toEqual([]);
    expect(env.frames.pending).toBe(0);
  });

  it("shrivels both Dock items of an app pinned twice", () => {
    const env = setup();
    const chessAgain = { ...chess, rect: { ...chess.rect, x: 760 } };
    env.renderer.dock([chess, chessAgain]);
    env.renderer.growth(overgrownChess);
    env.frames.runFrame(500);

    env.renderer.growth(launchedChess);
    const drawnFrames = run(env, 1000, 1000);

    expect(drawnFrames[5].instances.map(({ seed, wither }) => ({ seed, withered: wither > 0 }))).toEqual([
      { seed: chessSeed, withered: true },
      { seed: fnv1a32("com.apple.Chess#1"), withered: true },
    ]);
    expect(drawnFrames[drawnFrames.length - 1].instances).toEqual([]);
  });

  it("does not sway the moss while it shrivels with sway off", () => {
    const env = mossyDock(false);
    env.renderer.growth(launchedChess);

    const drawnFrames = run(env, 1000, 1000);

    expect(drawnFrames.length).toBeGreaterThan(10);
    expect(env.calls().flatMap((call) => (call.op === "draw" ? [call.swaySeconds] : []))).toEqual(
      drawnFrames.map(() => 0),
    );
  });
});
