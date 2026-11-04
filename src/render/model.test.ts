import { describe, expect, it } from "vitest";

import type { AppGrowth, BundleId, DockItem } from "../ipc";
import {
  type ItemKey,
  type Scene,
  artBox,
  drawnIcon,
  fnv1a32,
  itemKeys,
  parseOvergrowthOverride,
  quantize,
  step,
  toDeviceBox,
} from "./model";

const TILE = { y: 918, width: 56, height: 56 };

function item(bundleId: string, x: number): DockItem {
  return { bundleId: bundleId as BundleId, rect: { x, ...TILE } };
}

function growth(bundleId: string, overgrowth: number): AppGrowth {
  return { bundleId: bundleId as BundleId, overgrowth, stage: "patchy" };
}

function scene(overrides: Partial<Scene> = {}): Scene {
  return {
    key: "com.hnc.Discord#0" as ItemKey,
    seed: 3244016102,
    rect: { x: 700, ...TILE },
    overgrowth: 0.5,
    ...overrides,
  };
}

describe("drawnIcon", () => {
  it("draws the Dock item with the highest overgrowth", () => {
    const items = [
      item("com.apple.finder", 100),
      item("com.hnc.Discord", 160),
      item("com.apple.Safari", 220),
    ];
    const frame = [
      growth("com.apple.finder", 0),
      growth("com.hnc.Discord", 0.65),
      growth("com.apple.Safari", 0.2),
    ];

    const drawn = drawnIcon(items, frame, null);

    expect(drawn?.key).toBe("com.hnc.Discord#0");
    expect(drawn?.rect.x).toBe(160);
    expect(drawn?.overgrowth).toBe(quantize(0.65));
  });

  it("breaks a tie in favour of the first item in Dock order", () => {
    const items = [item("com.apple.Safari", 100), item("com.hnc.Discord", 160)];
    const frame = [
      growth("com.apple.Safari", 0.4),
      growth("com.hnc.Discord", 0.4),
    ];

    expect(drawnIcon(items, frame, null)?.key).toBe("com.apple.Safari#0");
  });

  it("treats two overgrowths within one visible step as a tie", () => {
    const items = [item("com.apple.Safari", 100), item("com.hnc.Discord", 160)];
    const frame = [
      growth("com.apple.Safari", 0.5),
      growth("com.hnc.Discord", 0.5 + 1 / 1024),
    ];

    expect(drawnIcon(items, frame, null)?.key).toBe("com.apple.Safari#0");
  });

  it("draws the first item, clean, when every item is clean", () => {
    const items = [item("com.apple.finder", 100), item("com.apple.Safari", 160)];
    const frame = [growth("com.apple.finder", 0), growth("com.apple.Safari", 0)];

    const drawn = drawnIcon(items, frame, null);

    expect(drawn?.key).toBe("com.apple.finder#0");
    expect(drawn?.overgrowth).toBe(0);
  });

  it("counts an item with no growth entry yet as clean", () => {
    const items = [item("com.apple.finder", 100), item("com.hnc.Discord", 160)];

    expect(drawnIcon(items, [], null)?.overgrowth).toBe(0);
    expect(
      drawnIcon(items, [growth("com.hnc.Discord", 0.2)], null)?.key,
    ).toBe("com.hnc.Discord#0");
  });

  it("draws nothing for an empty Dock", () => {
    expect(drawnIcon([], [growth("com.hnc.Discord", 1)], null)).toBeNull();
  });

  it("draws the first of two items that share the most overgrown bundle ID", () => {
    const items = [
      item("com.apple.Safari", 100),
      item("com.microsoft.VSCode", 160),
      item("com.microsoft.VSCode", 220),
    ];
    const frame = [
      growth("com.apple.Safari", 0),
      growth("com.microsoft.VSCode", 0.7),
      growth("com.microsoft.VSCode", 0.7),
    ];

    const drawn = drawnIcon(items, frame, null);

    expect(drawn?.key).toBe("com.microsoft.VSCode#0");
    expect(drawn?.rect.x).toBe(160);
    expect(drawn?.seed).toBe(0x96d9fb5b);
  });

  it("gives the drawn icon the dev override's overgrowth", () => {
    const items = [item("com.apple.finder", 100), item("com.hnc.Discord", 160)];
    const frame = [growth("com.apple.finder", 0), growth("com.hnc.Discord", 0.65)];

    const drawn = drawnIcon(items, frame, 0.3);

    expect(drawn?.key).toBe("com.hnc.Discord#0");
    expect(drawn?.overgrowth).toBe(quantize(0.3));
  });
});

describe("itemKeys", () => {
  it("numbers each item among the items that share its bundle ID, so each copy gets its own seed", () => {
    const items = [
      item("com.microsoft.VSCode", 100),
      item("com.apple.Safari", 160),
      item("com.microsoft.VSCode", 220),
    ];

    expect(itemKeys(items)).toEqual([
      "com.microsoft.VSCode#0",
      "com.apple.Safari#0",
      "com.microsoft.VSCode#1",
    ]);
  });
});

describe("fnv1a32", () => {
  it.each([
    ["", 0x811c9dc5],
    ["a", 0xe40c292c],
    ["foobar", 0xbf9cf968],
    ["com.apple.Safari#0", 0x46a0038d],
    ["com.microsoft.VSCode#0", 0x96d9fb5b],
    ["com.microsoft.VSCode#1", 0x95d9f9c8],
    ["é#0", 0x5d848442],
  ])("hashes the UTF-8 bytes of %j to the golden value", (text, expected) => {
    expect(fnv1a32(text)).toBe(expected);
  });
});

describe("quantize", () => {
  it.each([
    [0, 0],
    [1, 1],
    [0.5, 0.5],
    [0.3, 77 / 256],
    [0.5 + 1 / 1024, 0.5],
    [1 / 1024, 0],
  ])("rounds %d to the nearest 1/256", (overgrowth, expected) => {
    expect(quantize(overgrowth)).toBe(expected);
  });
});

describe("step", () => {
  const all = { geometry: true, moss: true };

  it("draws everything for the first scene", () => {
    expect(step(null, scene())).toEqual({ kind: "draw", changes: all });
  });

  it("does nothing when the scene is unchanged", () => {
    expect(step(scene(), scene())).toEqual({ kind: "none" });
  });

  it("updates only the geometry when the drawn icon's rect moves", () => {
    const moved = scene({ rect: { x: 704, y: 912, width: 64, height: 64 } });

    expect(step(scene(), moved)).toEqual({
      kind: "draw",
      changes: { geometry: true, moss: false },
    });
  });

  it("updates only the moss when the overgrowth moves by a visible step", () => {
    const grown = scene({ overgrowth: 0.5 + 1 / 256 });

    expect(step(scene(), grown)).toEqual({
      kind: "draw",
      changes: { geometry: false, moss: true },
    });
  });

  it("does nothing for an overgrowth change smaller than one visible step", () => {
    const items = [item("com.hnc.Discord", 700)];
    const before = drawnIcon(items, [growth("com.hnc.Discord", 0.5)], null);
    const after = drawnIcon(items, [growth("com.hnc.Discord", 0.5 + 1 / 1024)], null);

    expect(step(before, after)).toEqual({ kind: "none" });
  });

  it("redraws everything when another icon becomes the drawn one", () => {
    const other = scene({ key: "com.apple.Safari#0" as ItemKey, seed: 1184891789 });

    expect(step(scene(), other)).toEqual({ kind: "draw", changes: all });
  });

  it("clears at once when the Dock becomes empty", () => {
    expect(step(scene(), null)).toEqual({ kind: "clear" });
  });

  it("does nothing while the Dock stays empty", () => {
    expect(step(null, null)).toEqual({ kind: "none" });
  });
});

describe("parseOvergrowthOverride", () => {
  it.each([
    ["0", 0],
    ["0.3", 0.3],
    [".6", 0.6],
    ["1", 1],
    ["1.0", 1],
  ])("accepts %j", (raw, overgrowth) => {
    expect(parseOvergrowthOverride(raw)).toEqual({ kind: "set", overgrowth });
  });

  it.each(["1.5", "-0.1", "abc", "0x1", "NaN", "Infinity", "0.3abc"])(
    "rejects %j",
    (raw) => {
      expect(parseOvergrowthOverride(raw)).toEqual({ kind: "invalid", raw });
    },
  );

  it.each([undefined, ""])("is unset for %j", (raw) => {
    expect(parseOvergrowthOverride(raw)).toEqual({ kind: "unset" });
  });
});

describe("toDeviceBox", () => {
  it("scales a CSS-pixel rect by the pixel ratio and measures y up from the canvas bottom", () => {
    const rect = { x: 700, y: 918, width: 56, height: 56 };

    expect(toDeviceBox(rect, 2, 1964)).toEqual({ x: 1400, y: 16, width: 112, height: 112 });
  });

  it("keeps CSS pixels at a pixel ratio of 1", () => {
    const rect = { x: 10.5, y: 1000, width: 64, height: 64 };

    expect(toDeviceBox(rect, 1, 1080)).toEqual({ x: 10.5, y: 16, width: 64, height: 64 });
  });
});

describe("artBox", () => {
  it.each([
    ["a normal 47 x 59 tile", { x: 711, y: 918, width: 47, height: 59 }, { x: 716, y: 929, side: 37 }, 0.5],
    ["a magnified 57 x 69 tile", { x: 578, y: 907, width: 57, height: 69 }, { x: 584, y: 919, side: 46 }, 1],
  ])("finds the square icon art in %s", (_, tile, art, tolerance) => {
    const box = artBox(tile);

    expect(box.width).toBe(box.height);
    expect(Math.abs(box.width - art.side)).toBeLessThanOrEqual(tolerance);
    expect(Math.abs(box.x - art.x)).toBeLessThanOrEqual(tolerance);
    expect(Math.abs(box.y - art.y)).toBeLessThanOrEqual(tolerance);
  });
});
