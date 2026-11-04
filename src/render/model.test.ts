import { describe, expect, it } from "vitest";

import type { AppGrowth, BundleId, DockItem } from "../ipc";
import {
  CORNER_RADIUS_PER_SIDE,
  INSTANCE_ATTRIBUTES,
  SPRITE_TIP_LEAN,
  type MossInstance,
  type MossOvergrowth,
  type MossyIcon,
  type ItemKey,
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
  spritesFor,
  step,
  toDeviceBox,
  worstCaseFront,
} from "./model";
import * as model from "./model";
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
    expect(step("unknown", [icon()])).toEqual({ kind: "draw" });
  });

  it("does nothing when the scene is unchanged", () => {
    expect(step([icon()], [icon()])).toEqual({ kind: "none" });
  });

  it("redraws when another app's icon takes a tile with the same overgrowth", () => {
    const key = "com.apple.Safari#0" as ItemKey;

    expect(step([icon()], [icon({ key, seed: fnv1a32(key) })])).toEqual({ kind: "draw" });
  });

  it("redraws when two saturated neighbours trade places", () => {
    const saturated = { overgrowth: mossy(1), overgrown: true };
    const safariKey = "com.apple.Safari#0" as ItemKey;
    const left = { x: 700, ...TILE };
    const right = { x: 760, ...TILE };
    const discordAt = (tile: typeof left) => icon({ ...saturated, tile });
    const safariAt = (tile: typeof left) => icon({ ...saturated, key: safariKey, seed: fnv1a32(safariKey), tile });

    expect(step([discordAt(left), safariAt(right)], [safariAt(left), discordAt(right)])).toEqual({ kind: "draw" });
  });

  it.each([{ x: 704 }, { y: 912 }, { width: 64 }, { height: 64 }])(
    "redraws when one field of a mossy icon's tile moves: %j",
    (moved) => {
      const next = icon({ tile: { ...icon().tile, ...moved } });

      expect(step([icon()], [next])).toEqual({ kind: "draw" });
    },
  );

  it("redraws when a mossy icon's overgrowth moves by a visible step", () => {
    expect(step([icon()], [icon({ overgrowth: mossy(0.5 + 1 / 256) })])).toEqual({ kind: "draw" });
  });

  it("redraws when a mossy icon becomes overgrown", () => {
    expect(step([icon({ overgrowth: mossy(1) })], [icon({ overgrowth: mossy(1), overgrown: true })])).toEqual({
      kind: "draw",
    });
  });

  it("does nothing for an overgrowth change smaller than one visible step", () => {
    const items = [item("com.hnc.Discord", 700)];
    const before = mossyIcons(items, [growth("com.hnc.Discord", 0.5)], null);
    const after = mossyIcons(items, [growth("com.hnc.Discord", 0.5 + 1 / 1024)], null);

    expect(step(before, after)).toEqual({ kind: "none" });
  });

  it("redraws when an icon gains moss or loses it", () => {
    const safari = icon({ key: "com.apple.Safari#0" as ItemKey, seed: 1184891789, tile: { x: 760, ...TILE } });

    expect(step([icon()], [icon(), safari])).toEqual({ kind: "draw" });
    expect(step([icon(), safari], [safari])).toEqual({ kind: "draw" });
  });

  it("draws nothing for a clean Dock whose tiles move", () => {
    const frame = [growth("com.apple.finder", 0, "clean"), growth("com.hnc.Discord", 0, "clean")];
    const before = mossyIcons([item("com.apple.finder", 100), item("com.hnc.Discord", 160)], frame, null);
    const after = mossyIcons([item("com.apple.finder", 96), item("com.hnc.Discord", 152)], frame, null);

    expect(step(before, after)).toEqual({ kind: "none" });
  });

  it("clears at once when the last mossy icon goes", () => {
    expect(step([icon()], [])).toEqual({ kind: "clear" });
  });

  it("clears a canvas in an unknown state, after a resize or a restore, when nothing is mossy", () => {
    expect(step("unknown", [])).toEqual({ kind: "clear" });
  });

  it("does nothing while nothing is mossy", () => {
    expect(step([], [])).toEqual({ kind: "none" });
  });
});

describe("mossInstances", () => {
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
    const declared = new Map(
      [...glslConstants().matchAll(/^const (float|int|vec2) (\w+) = (.+);$/gm)].map(
        ([, , name, value]) => [name, value],
      ),
    );
    const values = model as unknown as Record<string, number | readonly number[]>;

    expect(declared.size).toBeGreaterThan(0);
    for (const [name, literal] of declared) {
      const expected = values[name];
      const parsed = literal.startsWith("vec2(")
        ? literal.slice(5, -1).split(",").map(Number)
        : Number(literal);
      expect(parsed, name).toEqual(expected);
    }
  });

  it("writes every float with a decimal point, as GLSL requires", () => {
    for (const [, value] of glslConstants().matchAll(/^const float \w+ = (.+);$/gm)) {
      expect(value).toMatch(/\./);
    }
  });
});

describe("spritesFor", () => {
  const seeds = Array.from({ length: 2000 }, (_, index) => fnv1a32(`app.${index}#0`));

  function clipDistance(x: number, y: number): number {
    const r = CORNER_RADIUS_PER_SIDE;
    const qx = Math.abs(x - 0.5) - 0.5 + r;
    const qy = Math.abs(y - 0.5) - 0.5 + r;
    return Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - r;
  }

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
      for (const { x, y, size } of spritesFor(seed)) {
        const half = size / 2;
        // The tip leans by up to SPRITE_TIP_LEAN of the box at the top, and not at the foot.
        const lean = SPRITE_TIP_LEAN * size;
        const corners = [
          [x - half, y - half],
          [x + half, y - half],
          [x - half - lean, y + half],
          [x + half + lean, y + half],
        ];
        for (const [cx, cy] of corners) {
          expect(clipDistance(cx, cy), `seed ${seed}`).toBeLessThan(0);
        }
        expect(y + half, `seed ${seed}`).toBeLessThanOrEqual(0.55);
      }
    }
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

  it("writes exactly `size` values per attribute, zeros for a missing sprite", () => {
    const values = INSTANCE_ATTRIBUTES.map((attribute) => attribute.values(layoutInstance));

    expect(values.map((written) => written.length)).toEqual(INSTANCE_ATTRIBUTES.map(({ size }) => size));
    expect(values.flat()).toEqual([
      10, 20, 74, 74,
      ((0x5678 / 0x10000) * 64), ((0x1234 / 0x10000) * 64),
      quantize(0.6), -0.5,
      0.4, 0.2, 0.15, 2,
      0, 0, 0, 0,
      0, 0, 0, 0,
    ]);
  });
});
