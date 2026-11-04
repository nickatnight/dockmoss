import { describe, expect, it } from "vitest";

import type { AppGrowth, BundleId, DockItem } from "../ipc";
import {
  CORNER_RADIUS_PER_SIDE,
  INSTANCE_ATTRIBUTES,
  SHRIVEL_MS,
  SPRITE_TIP_LEAN,
  WITHER_TINT,
  type MossInstance,
  type MossOvergrowth,
  type MossyIcon,
  type ItemKey,
  type Drawn,
  type Shrivel,
  type Shrivels,
  type Sprite,
  artBox,
  glslConstants,
  fnv1a32,
  itemKeys,
  mossHue,
  mossInstances,
  mossyIcons,
  parseOvergrowthOverride,
  parseSwayFlag,
  mossOvergrowth,
  quantize,
  seedOffset,
  shrivelFrame,
  shrivelInstance,
  shrivelProgress,
  shrivelsAfter,
  spritesFor,
  step,
  toDeviceBox,
  worstCaseFront,
} from "./model";
import * as model from "./model";
import fragmentSource from "./moss.frag.glsl?raw";
import vertexSource from "./moss.vert.glsl?raw";

const TILE = { y: 918, width: 56, height: 56 };

function item(bundleId: string, x: number): DockItem {
  return { bundleId: bundleId as BundleId, rect: { x, ...TILE } };
}

function growth(
  bundleId: string,
  overgrowth: number,
  stage: AppGrowth["stage"] = "patchy",
): AppGrowth {
  return { bundleId: bundleId as BundleId, overgrowth, stage };
}

function mossy(overgrowth: number): MossOvergrowth {
  const value = mossOvergrowth(overgrowth);
  if (value === null) {
    throw new Error(`no moss at overgrowth ${overgrowth}`);
  }
  return value;
}

function icon(overrides: Partial<MossyIcon> = {}): MossyIcon {
  return {
    key: "com.hnc.Discord#0" as ItemKey,
    seed: 3244016102,
    tile: { x: 700, ...TILE },
    overgrowth: mossy(0.5),
    overgrown: false,
    ...overrides,
  };
}

/** A canvas showing `icons` and no shrivel frame. */
function shown(icons: readonly MossyIcon[]): Drawn {
  return { icons, showsShrivel: false };
}

const noShrivels: Shrivels = new Map();

describe("mossyIcons", () => {
  it("gives every Dock item with moss an icon, in Dock order, and leaves clean items out", () => {
    const items = [
      item("com.apple.finder", 100),
      item("com.hnc.Discord", 160),
      item("com.apple.Safari", 220),
      item("ru.keepcoder.Telegram", 280),
    ];
    const frame = [
      growth("com.apple.finder", 0, "clean"),
      growth("com.hnc.Discord", 0.65),
      growth("com.apple.Safari", 0.2, "fuzz"),
      growth("ru.keepcoder.Telegram", 1, "overgrown"),
    ];

    const icons = mossyIcons(items, frame, null);

    expect(icons.map(({ key, tile, overgrowth, overgrown }) => ({ key, x: tile.x, overgrowth, overgrown }))).toEqual([
      { key: "com.hnc.Discord#0", x: 160, overgrowth: quantize(0.65), overgrown: false },
      { key: "com.apple.Safari#0", x: 220, overgrowth: quantize(0.2), overgrown: false },
      { key: "ru.keepcoder.Telegram#0", x: 280, overgrowth: 1, overgrown: true },
    ]);
  });

  it("leaves out an item whose overgrowth rounds to 0", () => {
    const items = [item("com.hnc.Discord", 160)];

    expect(mossyIcons(items, [growth("com.hnc.Discord", 1 / 1024, "fuzz")], null)).toEqual([]);
  });

  it("counts an item with no growth entry yet as clean", () => {
    const items = [item("com.apple.finder", 100), item("com.hnc.Discord", 160)];

    expect(mossyIcons(items, [growth("com.hnc.Discord", 0.2)], null).map(({ key }) => key)).toEqual([
      "com.hnc.Discord#0",
    ]);
  });

  it("draws nothing for an empty Dock", () => {
    expect(mossyIcons([], [growth("com.hnc.Discord", 1)], null)).toEqual([]);
    expect(mossyIcons([], [], 0.5)).toEqual([]);
  });

  it("gives two items for one app the app's overgrowth and a seed each", () => {
    const items = [
      item("com.apple.Safari", 100),
      item("com.microsoft.VSCode", 160),
      item("com.microsoft.VSCode", 220),
    ];
    const frame = [growth("com.apple.Safari", 0, "clean"), growth("com.microsoft.VSCode", 0.7)];

    const icons = mossyIcons(items, frame, null);

    expect(icons.map(({ key, seed, overgrowth }) => ({ key, seed, overgrowth }))).toEqual([
      { key: "com.microsoft.VSCode#0", seed: 0x96d9fb5b, overgrowth: quantize(0.7) },
      { key: "com.microsoft.VSCode#1", seed: 0x95d9f9c8, overgrowth: quantize(0.7) },
    ]);
  });

  it("gives every Dock item the dev override's overgrowth, clean or not", () => {
    const items = [item("com.apple.finder", 100), item("com.hnc.Discord", 160)];
    const frame = [growth("com.apple.finder", 0, "clean"), growth("com.hnc.Discord", 1, "overgrown")];

    const icons = mossyIcons(items, frame, 0.3);

    expect(icons.map(({ key, overgrowth, overgrown }) => ({ key, overgrowth, overgrown }))).toEqual([
      { key: "com.apple.finder#0", overgrowth: quantize(0.3), overgrown: false },
      { key: "com.hnc.Discord#0", overgrowth: quantize(0.3), overgrown: false },
    ]);
  });

  it("makes every Dock item overgrown under an override of 1", () => {
    const items = [item("com.apple.finder", 100)];

    expect(mossyIcons(items, [], 1)).toMatchObject([{ overgrowth: 1, overgrown: true }]);
  });

  it("draws nothing under an override of 0", () => {
    const items = [item("com.hnc.Discord", 160)];

    expect(mossyIcons(items, [growth("com.hnc.Discord", 1, "overgrown")], 0)).toEqual([]);
  });
});

describe("step", () => {
  it("draws the first scene", () => {
    expect(step("unknown", [icon()], noShrivels)).toEqual({ kind: "draw" });
  });

  it("does nothing when the scene is unchanged", () => {
    expect(step(shown([icon()]), [icon()], noShrivels)).toEqual({ kind: "none" });
  });

  it("redraws when another app's icon takes a tile with the same overgrowth", () => {
    const key = "com.apple.Safari#0" as ItemKey;

    expect(step(shown([icon()]), [icon({ key, seed: fnv1a32(key) })], noShrivels)).toEqual({ kind: "draw" });
  });

  it("redraws when two saturated neighbours trade places", () => {
    const saturated = { overgrowth: mossy(1), overgrown: true };
    const safariKey = "com.apple.Safari#0" as ItemKey;
    const left = { x: 700, ...TILE };
    const right = { x: 760, ...TILE };
    const discordAt = (tile: typeof left) => icon({ ...saturated, tile });
    const safariAt = (tile: typeof left) => icon({ ...saturated, key: safariKey, seed: fnv1a32(safariKey), tile });

    expect(step(shown([discordAt(left), safariAt(right)]), [safariAt(left), discordAt(right)], noShrivels)).toEqual({ kind: "draw" });
  });

  it.each([{ x: 704 }, { y: 912 }, { width: 64 }, { height: 64 }])(
    "redraws when one field of a mossy icon's tile moves: %j",
    (moved) => {
      const next = icon({ tile: { ...icon().tile, ...moved } });

      expect(step(shown([icon()]), [next], noShrivels)).toEqual({ kind: "draw" });
    },
  );

  it("redraws when a mossy icon's overgrowth moves by a visible step", () => {
    expect(step(shown([icon()]), [icon({ overgrowth: mossy(0.5 + 1 / 256) })], noShrivels)).toEqual({ kind: "draw" });
  });

  it("redraws when a mossy icon becomes overgrown", () => {
    expect(step(shown([icon({ overgrowth: mossy(1) })]), [icon({ overgrowth: mossy(1), overgrown: true })], noShrivels)).toEqual({
      kind: "draw",
    });
  });

  it("does nothing for an overgrowth change smaller than one visible step", () => {
    const items = [item("com.hnc.Discord", 700)];
    const before = mossyIcons(items, [growth("com.hnc.Discord", 0.5)], null);
    const after = mossyIcons(items, [growth("com.hnc.Discord", 0.5 + 1 / 1024)], null);

    expect(step(shown(before), after, noShrivels)).toEqual({ kind: "none" });
  });

  it("redraws when an icon gains moss or loses it", () => {
    const safari = icon({ key: "com.apple.Safari#0" as ItemKey, seed: 1184891789, tile: { x: 760, ...TILE } });

    expect(step(shown([icon()]), [icon(), safari], noShrivels)).toEqual({ kind: "draw" });
    expect(step(shown([icon(), safari]), [safari], noShrivels)).toEqual({ kind: "draw" });
  });

  it("draws nothing for a clean Dock whose tiles move", () => {
    const frame = [growth("com.apple.finder", 0, "clean"), growth("com.hnc.Discord", 0, "clean")];
    const before = mossyIcons([item("com.apple.finder", 100), item("com.hnc.Discord", 160)], frame, null);
    const after = mossyIcons([item("com.apple.finder", 96), item("com.hnc.Discord", 152)], frame, null);

    expect(step(shown(before), after, noShrivels)).toEqual({ kind: "none" });
  });

  it("clears at once when the last mossy icon goes", () => {
    expect(step(shown([icon()]), [], noShrivels)).toEqual({ kind: "clear" });
  });

  it("clears a canvas in an unknown state, after a resize or a restore, when nothing is mossy", () => {
    expect(step("unknown", [], noShrivels)).toEqual({ kind: "clear" });
  });

  it("does nothing while nothing is mossy", () => {
    expect(step(shown([]), [], noShrivels)).toEqual({ kind: "none" });
  });

  describe("with a shrivel frame on the canvas", () => {
    const showingShrivel = (icons: readonly MossyIcon[]): Drawn => ({ icons, showsShrivel: true });
    const shrivelling: Shrivels = new Map([[icon().key, { icon: icon(), startMs: 1000 }]]);

    it("redraws the scene once no shrivel runs", () => {
      expect(step(showingShrivel([icon()]), [icon()], noShrivels)).toEqual({ kind: "draw" });
    });

    it("clears at once when no shrivel runs and nothing is mossy", () => {
      expect(step(showingShrivel([]), [], noShrivels)).toEqual({ kind: "clear" });
    });

    it("leaves the redraws to the shrivel loop while one runs", () => {
      expect(step(showingShrivel([icon()]), [icon()], shrivelling)).toEqual({ kind: "none" });
    });

    it("does not clear while a shrivel runs and nothing else is mossy", () => {
      expect(step(showingShrivel([]), [], shrivelling)).toEqual({ kind: "none" });
    });

    it("draws when an icon drops out to shrivel", () => {
      expect(step(shown([icon()]), [], shrivelling)).toEqual({ kind: "draw" });
    });
  });
});

describe("mossInstances", () => {
  it("draws static moss unwithered", () => {
    const [instance] = mossInstances([icon({ overgrowth: mossy(1), overgrown: true })], 2, 1964);

    expect(instance.wither).toBe(0);
  });

  it("gives an overgrown icon its sprites, and any other icon none", () => {
    const [overgrown, patchy] = mossInstances(
      [icon({ overgrowth: mossy(1), overgrown: true }), icon({ overgrowth: mossy(0.9) })],
      2,
      1964,
    );

    expect(overgrown.sprites).toEqual(spritesFor(3244016102));
    expect(overgrown.sprites.length).toBeGreaterThan(0);
    expect(patchy.sprites).toEqual([]);
  });

  it("puts each icon's art box in canvas device pixels, with its seed and overgrowth", () => {
    const tile = { x: 711, y: 918, width: 47, height: 59 };
    const icons = [icon({ tile, overgrowth: mossy(0.6) })];

    const [instance] = mossInstances(icons, 2, 1964);

    expect(instance.box).toEqual(toDeviceBox(artBox(tile), 2, 1964));
    expect(instance.seed).toBe(3244016102);
    expect(instance.overgrowth).toBe(quantize(0.6));
    expect(instance.hue).toBe(mossHue(3244016102));
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

describe("mossOvergrowth", () => {
  it.each([
    [0, null],
    [1 / 1024, null],
    [1 / 256, 1 / 256],
    [0.6, quantize(0.6)],
    [1, 1],
  ])("of %d is %s", (overgrowth, expected) => {
    expect(mossOvergrowth(overgrowth)).toBe(expected);
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

describe("parseSwayFlag", () => {
  it("turns sway on for 1", () => {
    expect(parseSwayFlag("1")).toEqual({ kind: "set", sway: true });
  });

  it.each(["0", ""])("leaves sway off for %j", (raw) => {
    expect(parseSwayFlag(raw)).toEqual({ kind: "set", sway: false });
  });

  it("leaves sway off when unset", () => {
    expect(parseSwayFlag(undefined)).toEqual({ kind: "set", sway: false });
  });

  it.each(["yes", "true", "2"])("rejects %j", (raw) => {
    expect(parseSwayFlag(raw)).toEqual({ kind: "invalid", raw });
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

describe("art boxes", () => {
  // Job 009's AX tile rects in points, normal and magnified over Discord.
  it.each([
    ["normal", [[476, 46], [523, 47], [570, 47], [617, 47], [664, 47], [711, 47]]],
    ["magnified", [[435, 46], [482, 46], [529, 48], [578, 57], [635, 65], [701, 67], [769, 65]]],
  ])("of adjacent %s tiles leave at least 1 pt between them", (_, tiles) => {
    const boxes = tiles.map(([x, width]) => artBox({ x, y: 918, width, height: width + 12 }));

    for (let index = 1; index < boxes.length; index++) {
      const left = boxes[index - 1];
      expect(boxes[index].x - (left.x + left.width)).toBeGreaterThanOrEqual(1);
    }
  });
});

describe("seedOffset", () => {
  it.each([
    [0, [0, 0]],
    [0x96d9fb5b, [62.8388671875, 37.7119140625]],
    [0xffffffff, [63.9990234375, 63.9990234375]],
  ])("moves seed %d to its own patch of noise, from the seed's low and high halves", (seed, offset) => {
    expect(seedOffset(seed)).toEqual(offset);
  });
});

describe("worstCaseFront", () => {
  it("keeps moss at overgrowth 1 at or below 0.55 of the art height on the centre column", () => {
    expect(worstCaseFront(0.5, 1)).toBeLessThanOrEqual(0.55);
  });

  it.each([0, 0.066, 0.934, 1])("keeps moss below the top corner arcs at x = %d", (x) => {
    expect(worstCaseFront(x, 1)).toBeLessThan(1 - CORNER_RADIUS_PER_SIDE);
  });

  it("raises the centre front as overgrowth rises through 0.3, 0.6 and 1.0", () => {
    const fronts = [0.3, 0.6, 1].map((overgrowth) => worstCaseFront(0.5, overgrowth));

    expect(fronts[0]).toBeGreaterThan(0);
    expect(fronts[1]).toBeGreaterThan(fronts[0]);
    expect(fronts[2]).toBeGreaterThan(fronts[1]);
  });

  it.each([0, 0.5, 1])("draws nothing at overgrowth 0, at x = %d", (x) => {
    expect(worstCaseFront(x, 0)).toBe(0);
  });

  // Derived by hand, not from a run. At overgrowth 1 the reach is 0.62 and the
  // front peaks at 0.62 + 0.46875 * 0.24 (ragged) + 0.5 * 0.05 (tufts) + 0.012
  // (sway) + max(0.04, 0.03) (fuzz or shadow) = 0.8095. On x = 0.5 the corner
  // distance is the smaller one: 1.25 * sqrt(0.434^2 + (y - 0.066)^2) = 0.8095
  // at y = 0.5467, so the 1/1000 scan returns 0.547. The other rows follow the
  // same sum (reach 0.62 * 0.6^0.8 and 0.62 * 0.3^0.8), checked in Python.
  // Dropping any term of the sum moves at least one row.
  it.each([
    [0.5, 1, 0.547],
    [0.066, 1, 0.714],
    [0.5, 0.6, 0.354],
    [0.5, 0.3, 0.251],
  ])("puts the worst front on x = %d at overgrowth %d at %d", (x, overgrowth, expected) => {
    expect(worstCaseFront(x, overgrowth)).toBe(expected);
  });
});

describe("mossHue", () => {
  const hues = Array.from({ length: 2000 }, (_, index) => mossHue(fnv1a32(`app.${index}#0`)));

  it("stays from -1 (warmer) to 1 (cooler)", () => {
    expect(Math.min(...hues)).toBeGreaterThanOrEqual(-1);
    expect(Math.max(...hues)).toBeLessThanOrEqual(1);
  });

  it("spreads icons across the range, not all one hue", () => {
    expect(hues.filter((hue) => hue < -0.5).length).toBeGreaterThan(300);
    expect(hues.filter((hue) => hue > 0.5).length).toBeGreaterThan(300);
  });

  // Golden values: a change here recolours every icon's moss.
  it.each([
    [0, -1],
    [0xffffffff, -0.23921568627450984],
    [3244016102, 0.3411764705882352],
  ])("is fixed for seed %d", (seed, hue) => {
    expect(mossHue(seed)).toBe(hue);
  });
});

describe("the sway clock", () => {
  it("wraps at a whole number of every sway period", () => {
    for (const period of [model.MOSS_SWAY_PERIOD, model.SPRITE_SWAY_PERIOD]) {
      expect(model.SWAY_CYCLE_SECONDS % period, String(period)).toBe(0);
    }
  });
});

describe("glslConstants", () => {
  it("declares each shader constant with the model's own value", () => {
    const lines = glslConstants().split("\n");
    const declared = new Map(
      lines.flatMap((line) => {
        const match = /^const (float|int|vec2|vec3) (\w+) = (.+);$/.exec(line);
        return match === null ? [] : [[match[2], match[3]] as const];
      }),
    );
    const values = model as unknown as Record<string, number | readonly number[]>;

    expect(declared.size).toBe(lines.length);
    for (const [name, literal] of declared) {
      const expected = values[name];
      const vector = /^vec[23]\((.+)\)$/.exec(literal);
      const parsed = vector === null ? Number(literal) : vector[1].split(",").map(Number);
      expect(parsed, name).toEqual(expected);
    }
  });

  it("declares every upper-case name moss.frag.glsl uses and does not declare itself", () => {
    const code = fragmentSource.replace(/\/\/.*$/gm, "");
    const ownConstants = new Set([...code.matchAll(/\bconst \w+ ([A-Z][A-Z0-9_]+)\b/g)].map(([, name]) => name));
    const used = new Set([...code.matchAll(/\b[A-Z][A-Z0-9_]*[A-Z0-9]\b/g)].map(([name]) => name));
    const generated = new Set([...glslConstants().matchAll(/^const \w+ (\w+) =/gm)].map(([, name]) => name));
    const missing = [...used].filter((name) => !ownConstants.has(name) && !generated.has(name));

    expect(used.size).toBeGreaterThan(ownConstants.size);
    expect(missing).toEqual([]);
    expect([...ownConstants].filter((name) => generated.has(name))).toEqual([]);
  });

  it("writes every float with a decimal point, as GLSL requires", () => {
    for (const [, value] of glslConstants().matchAll(/^const float \w+ = (.+);$/gm)) {
      expect(value).toMatch(/\./);
    }
  });
});

function clipDistance(x: number, y: number): number {
  const r = CORNER_RADIUS_PER_SIDE;
  const qx = Math.abs(x - 0.5) - 0.5 + r;
  const qy = Math.abs(y - 0.5) - 0.5 + r;
  return Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - r;
}

function leanedCorners({ x, y, size }: Sprite): [number, number][] {
  const half = size / 2;
  const lean = SPRITE_TIP_LEAN * size;
  return [
    [x - half, y - half],
    [x + half, y - half],
    [x - half - lean, y + half],
    [x + half + lean, y + half],
  ];
}

describe("spritesFor", () => {
  const seeds = Array.from({ length: 2000 }, (_, index) => fnv1a32(`app.${index}#0`));

  it("gives every seed 1 to 3 sprites, and each count to some seed", () => {
    const counts = new Set(seeds.map((seed) => spritesFor(seed).length));

    expect([...counts].sort()).toEqual([1, 2, 3]);
  });

  it("uses both mushrooms and flowers", () => {
    const kinds = new Set(seeds.flatMap((seed) => spritesFor(seed).map(({ kind }) => kind)));

    expect([...kinds].sort()).toEqual(["flower", "mushroom"]);
  });

  it("keeps every sprite's box, leaning either way, inside the clip and at or below 0.55 of the art height", () => {
    for (const seed of seeds) {
      for (const sprite of spritesFor(seed)) {
        for (const [cx, cy] of leanedCorners(sprite)) {
          expect(clipDistance(cx, cy), `seed ${seed}`).toBeLessThan(0);
        }
        expect(sprite.y + sprite.size / 2, `seed ${seed}`).toBeLessThanOrEqual(0.55);
      }
    }
  });

  it("leans a sprite's tip by at most 1.5 device px on tile size 45's 74 px art box", () => {
    const largest = Math.max(...seeds.flatMap((seed) => spritesFor(seed).map(({ size }) => size)));

    expect(SPRITE_TIP_LEAN * largest * 74).toBeLessThanOrEqual(1.5);
  });

  it("never overlaps two sprites on one icon", () => {
    for (const seed of seeds) {
      const sprites = spritesFor(seed);
      for (const [index, a] of sprites.entries()) {
        for (const b of sprites.slice(index + 1)) {
          const apart = Math.max(Math.abs(a.x - b.x), Math.abs(a.y - b.y)) >= (a.size + b.size) / 2;
          expect(apart, `seed ${seed}`).toBe(true);
        }
      }
    }
  });

  // Golden values: a change here moves every overgrown icon's sprites.
  it.each([
    [
      "com.hnc.Discord#0",
      [
        { kind: "flower", x: 0.37352608400397, y: 0.22917886829003692, size: 0.18107235140632838 },
        { kind: "mushroom", x: 0.7711941741872579, y: 0.16338266673963517, size: 0.15032056090887636 },
        { kind: "mushroom", x: 0.581096185836941, y: 0.1918341681500897, size: 0.16903756269719453 },
      ],
    ],
    [
      "ru.keepcoder.Telegram#0",
      [{ kind: "mushroom", x: 0.6663523845374585, y: 0.1644334005890414, size: 0.17918860734906047 }],
    ],
  ])("places the same sprites for %s on every run", (key, sprites) => {
    expect(spritesFor(fnv1a32(key))).toEqual(sprites);
  });
});

const GLSL_SIZES: Record<string, number> = { float: 1, vec2: 2, vec3: 3, vec4: 4 };

const layoutInstance: MossInstance = {
  box: { x: 10, y: 20, width: 74, height: 74 },
  seed: 0x12345678,
  overgrowth: mossy(0.6),
  hue: -0.5,
  wither: 0.25,
  sprites: [{ kind: "flower", x: 0.4, y: 0.2, size: 0.15 }],
};

describe("INSTANCE_ATTRIBUTES", () => {
  it("lists every input of moss.vert.glsl with its size, in the shader's order", () => {
    const inputs = [...vertexSource.matchAll(/^in (\w+) (\w+);$/gm)].map(([, type, name]) => ({
      name,
      size: GLSL_SIZES[type],
    }));

    expect(inputs.length).toBeGreaterThan(0);
    expect(INSTANCE_ATTRIBUTES.map(({ name, size }) => ({ name, size }))).toEqual(inputs);
  });

  it("reads every input in moss.vert.glsl's main, so each one reaches the fragment shader", () => {
    const main = vertexSource.slice(vertexSource.indexOf("void main()"));
    const unread = INSTANCE_ATTRIBUTES.map(({ name }) => name).filter((name) => !new RegExp(`\\b${name}\\b`).test(main));

    expect(vertexSource).toContain("void main()");
    expect(unread).toEqual([]);
  });

  it("writes exactly `size` values per attribute, zeros for a missing sprite", () => {
    const values = INSTANCE_ATTRIBUTES.map((attribute) => attribute.values(layoutInstance));

    expect(values.map((written) => written.length)).toEqual(INSTANCE_ATTRIBUTES.map(({ size }) => size));
    expect(values.flat()).toEqual([
      10, 20, 74, 74,
      ((0x5678 / 0x10000) * 64), ((0x1234 / 0x10000) * 64),
      quantize(0.6), -0.5, 0.25,
      0.4, 0.2, 0.15, 2,
      0, 0, 0, 0,
      0, 0, 0, 0,
    ]);
  });
});

describe("shrivelsAfter", () => {
  const chess = item("com.apple.Chess", 700);
  const discord = item("com.hnc.Discord", 760);
  const overgrownChess = [growth("com.apple.Chess", 1, "overgrown"), growth("com.hnc.Discord", 0.6)];
  const cleanChess = [growth("com.apple.Chess", 0, "clean"), growth("com.hnc.Discord", 0.6)];
  const none = new Map<ItemKey, Shrivel>();

  function shrivelling(drawn: readonly MossyIcon[], items: DockItem[], frame: AppGrowth[]): Shrivel[] {
    return [...shrivelsAfter(none, shown(drawn), items, frame, mossyIcons(items, frame, null)).values()];
  }

  it("starts a shrivel for a drawn mossy item that the next growth frame lists as clean, from its drawn state", () => {
    const drawn = mossyIcons([chess, discord], overgrownChess, null);

    expect(shrivelling(drawn, [chess, discord], cleanChess)).toEqual([{ icon: drawn[0], startMs: null }]);
  });

  it("starts one for each Dock item of an app pinned twice", () => {
    const chessAgain = item("com.apple.Chess", 900);
    const drawn = mossyIcons([chess, chessAgain], overgrownChess, null);

    const keys = shrivelling(drawn, [chess, chessAgain], cleanChess).map(({ icon }) => icon.key);

    expect(keys).toEqual(["com.apple.Chess#0", "com.apple.Chess#1"]);
  });

  it("starts none for an item that leaves the Dock", () => {
    const drawn = mossyIcons([chess, discord], overgrownChess, null);

    expect(shrivelling(drawn, [discord], cleanChess)).toEqual([]);
  });

  it("starts none when the Dock list empties", () => {
    const drawn = mossyIcons([chess, discord], overgrownChess, null);

    expect(shrivelling(drawn, [], cleanChess)).toEqual([]);
  });

  it("starts none on the first scene after a load, reload or context restore", () => {
    expect(shrivelling([], [chess], cleanChess)).toEqual([]);
    expect([...shrivelsAfter(none, "unknown", [chess], cleanChess, []).values()]).toEqual([]);
  });

  it("starts none when overgrowth drops but stays above 0", () => {
    const drawn = mossyIcons([chess], overgrownChess, null);

    expect(shrivelling(drawn, [chess], [growth("com.apple.Chess", 0.3, "fuzz")])).toEqual([]);
  });

  it("starts none for an item that was already clean", () => {
    const drawn = mossyIcons([chess, discord], cleanChess, null);

    expect(shrivelling(drawn, [chess, discord], cleanChess)).toEqual([]);
  });

  it("starts none for an item whose app the growth frame no longer lists", () => {
    const drawn = mossyIcons([chess, discord], overgrownChess, null);

    expect(shrivelling(drawn, [chess, discord], [growth("com.hnc.Discord", 0.6)])).toEqual([]);
  });

  it("starts none while the dev override pins overgrowth", () => {
    const drawn = mossyIcons([chess], overgrownChess, 0.6);
    const next = mossyIcons([chess], cleanChess, 0.6);

    expect([...shrivelsAfter(none, shown(drawn), [chess], cleanChess, next).values()]).toEqual([]);
  });

  describe("for a shrivel in progress", () => {
    const drawnChess = mossyIcons([chess], overgrownChess, null)[0];
    const running = new Map([[drawnChess.key, { icon: drawnChess, startMs: 1000 }]]);

    function after(items: DockItem[], frame: AppGrowth[]): Shrivel[] {
      const drawnWithoutChess = mossyIcons(items, cleanChess, null);
      return [...shrivelsAfter(running, shown(drawnWithoutChess), items, frame, mossyIcons(items, frame, null)).values()];
    }

    it("keeps its start time while the app stays clean, even before the canvas has redrawn", () => {
      const stillDrawn = [drawnChess];

      const kept = shrivelsAfter(running, shown(stillDrawn), [chess], cleanChess, []);

      expect([...kept.values()]).toEqual([{ icon: drawnChess, startMs: 1000 }]);
    });

    it("moves it to the item's tile now, so it follows magnification", () => {
      const magnified = { ...chess, rect: { x: 690, y: 900, width: 70, height: 82 } };

      expect(after([magnified], cleanChess)).toEqual([
        { icon: { ...drawnChess, tile: magnified.rect }, startMs: 1000 },
      ]);
    });

    it("ends it when the app has moss again", () => {
      expect(after([chess], [growth("com.apple.Chess", 0.2, "fuzz")])).toEqual([]);
    });

    it("ends it when the item leaves the Dock", () => {
      expect(after([discord], cleanChess)).toEqual([]);
    });

    it("ends it when the Dock list empties", () => {
      expect(after([], cleanChess)).toEqual([]);
    });
  });
});

describe("shrivelProgress", () => {
  it.each([
    [1000, 0],
    [1300, 0.5],
    [1000 + SHRIVEL_MS, 1],
  ])("is the share of SHRIVEL_MS since the first frame: at %d ms, %d", (time, progress) => {
    expect(shrivelProgress(1000, time)).toBe(progress);
  });

  it("is clamped to 0 before the start and to 1 after the end", () => {
    expect(shrivelProgress(1000, 900)).toBe(0);
    expect(shrivelProgress(1000, 5000)).toBe(1);
  });

  it("never goes back as time goes on", () => {
    const samples = Array.from({ length: 100 }, (_, index) => shrivelProgress(1000, 900 + index * 10));

    for (let index = 1; index < samples.length; index++) {
      expect(samples[index]).toBeGreaterThanOrEqual(samples[index - 1]);
    }
  });
});

describe("shrivelInstance", () => {
  const progresses = Array.from({ length: 101 }, (_, index) => index / 100);
  const overgrown = icon({ overgrowth: mossy(1), overgrown: true });

  function at(progress: number, from: MossyIcon = overgrown): MossInstance | null {
    return shrivelInstance(from, progress, 2, 1964);
  }

  it("starts from the icon as the canvas last drew it", () => {
    expect(at(0)).toEqual(mossInstances([overgrown], 2, 1964)[0]);
  });

  it("leaves the icon no instance at progress 1, so it is clean", () => {
    expect(at(1)).toBeNull();
  });

  // Derived by hand: the smoothstep at 0.25 is 3 * 0.25^2 - 2 * 0.25^3 = 0.15625,
  // so 0.84375 of the overgrowth remains, which is 216/256 and needs no rounding.
  it("eases the front down slowly at first: a quarter of the way, an overgrown icon keeps 0.84375", () => {
    expect(at(0.25)?.overgrowth).toBe(0.84375);
  });

  it("recedes the front from its start to the bottom edge, never rising, and at or below 0.55", () => {
    const fronts = progresses.flatMap((progress) => {
      const instance = at(progress);
      return instance === null ? [] : [worstCaseFront(0.5, instance.overgrowth)];
    });
    const start = worstCaseFront(0.5, 1);

    expect(fronts.length).toBeGreaterThan(80);
    for (const [index, front] of fronts.entries()) {
      expect(front).toBeLessThanOrEqual(start);
      expect(front).toBeLessThanOrEqual(0.55);
      expect(front).toBeLessThanOrEqual(fronts[index - 1] ?? start);
    }
    expect(fronts[fronts.length - 1]).toBeLessThan(start / 5);
  });

  it("recedes from a patchy start the same way, to no moss", () => {
    const patchy = icon({ overgrowth: mossy(0.5) });
    const overgrowths = progresses.map((progress) => at(progress, patchy)?.overgrowth ?? 0);

    for (let index = 1; index < overgrowths.length; index++) {
      expect(overgrowths[index]).toBeLessThanOrEqual(overgrowths[index - 1]);
    }
    expect(overgrowths[0]).toBe(0.5);
    expect(overgrowths[overgrowths.length - 1]).toBe(0);
  });

  it("withers the moss from none at the start to full before the end, never back", () => {
    const withers = progresses.map((progress) => at(progress)?.wither ?? 1);

    expect(withers[0]).toBe(0);
    expect(withers[50]).toBe(1);
    for (let index = 1; index < withers.length; index++) {
      expect(withers[index]).toBeGreaterThanOrEqual(withers[index - 1]);
      expect(withers[index]).toBeLessThanOrEqual(1);
    }
  });

  it("shrinks every sprite to nothing inside its own box, the clip and the 0.55 bound", () => {
    const seeds = Array.from({ length: 500 }, (_, index) => fnv1a32(`app.${index}#0`));
    for (const seed of seeds) {
      const from = icon({ seed, overgrowth: mossy(1), overgrown: true });
      const start = spritesFor(seed);
      let previous: readonly Sprite[] = start;
      for (const progress of progresses) {
        const sprites = at(progress, from)?.sprites ?? [];
        if (sprites.length === 0) {
          continue;
        }
        expect(sprites).toHaveLength(start.length);
        for (const [slot, sprite] of sprites.entries()) {
          const top = sprite.y + sprite.size / 2;
          expect(sprite.size, `seed ${seed}`).toBeLessThanOrEqual(previous[slot].size);
          expect(top, `seed ${seed}`).toBeLessThanOrEqual(start[slot].y + start[slot].size / 2);
          expect(top, `seed ${seed}`).toBeLessThanOrEqual(0.55);
          expect(sprite.y - sprite.size / 2, `seed ${seed}`).toBeCloseTo(start[slot].y - start[slot].size / 2, 12);
          for (const [cx, cy] of leanedCorners(sprite)) {
            expect(clipDistance(cx, cy), `seed ${seed}`).toBeLessThan(0);
          }
        }
        previous = sprites;
      }
      const drawnFrames = progresses.flatMap((progress) => at(progress, from) ?? []);
      const last = drawnFrames[drawnFrames.length - 1];
      expect(Math.max(...last.sprites.map(({ size }) => size)), `seed ${seed}`).toBeLessThan(0.02);
    }
  });

  it("keeps the icon's tile, seed, hue and sprite kinds", () => {
    const instance = at(0.5);
    const start = mossInstances([overgrown], 2, 1964)[0];

    expect(instance?.box).toEqual(start.box);
    expect(instance?.seed).toBe(start.seed);
    expect(instance?.hue).toBe(start.hue);
    expect(instance?.sprites.map(({ kind, x }) => ({ kind, x }))).toEqual(start.sprites.map(({ kind, x }) => ({ kind, x })));
  });
});

describe("shrivelFrame", () => {
  const chessKey = "com.apple.Chess#0" as ItemKey;
  const chess = icon({ key: chessKey, seed: fnv1a32(chessKey), overgrowth: mossy(1), overgrown: true });
  const discord = icon({ tile: { x: 760, ...TILE } });

  it("starts a shrivel's clock on its first frame and draws it there as last drawn", () => {
    const { instances, running } = shrivelFrame(new Map([[chessKey, { icon: chess, startMs: null }]]), 5000, 2, 1964);

    expect(instances).toEqual(mossInstances([chess], 2, 1964));
    expect([...running.values()]).toEqual([{ icon: chess, startMs: 5000 }]);
  });

  it("draws concurrent shrivels each at its own progress", () => {
    const shrivels = new Map([
      [chessKey, { icon: chess, startMs: 1000 }],
      [discord.key, { icon: discord, startMs: 1300 }],
    ]);

    const { instances } = shrivelFrame(shrivels, 1300, 2, 1964);

    expect(instances).toEqual([shrivelInstance(chess, 0.5, 2, 1964), shrivelInstance(discord, 0, 2, 1964)]);
  });

  it("drops a shrivel at its end and draws nothing for it", () => {
    const shrivels = new Map([
      [chessKey, { icon: chess, startMs: 1000 }],
      [discord.key, { icon: discord, startMs: 1300 }],
    ]);

    const { instances, running } = shrivelFrame(shrivels, 1000 + SHRIVEL_MS, 2, 1964);

    expect(instances).toEqual([shrivelInstance(discord, 0.5, 2, 1964)]);
    expect([...running.keys()]).toEqual([discord.key]);
  });
});

describe("WITHER_TINT", () => {
  // Withered moss is WITHER_TINT times the colour's luma. With weights
  // that sum to 1, the luma of a premultiplied colour is at most its
  // alpha, so a tint no channel of which is above 1 keeps it premultiplied.
  it("keeps a withered premultiplied colour premultiplied", () => {
    const weights = /const vec3 REC709_LUMA = vec3\(([^)]+)\);/.exec(fragmentSource);

    expect(weights).not.toBeNull();
    expect(weights?.[1].split(",").map(Number).reduce((sum, weight) => sum + weight, 0)).toBeCloseTo(1, 6);
    for (const channel of WITHER_TINT) {
      expect(channel).toBeGreaterThanOrEqual(0);
      expect(channel).toBeLessThanOrEqual(1);
    }
  });
});
