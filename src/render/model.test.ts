import { describe, expect, it } from "vitest";

import type { AppGrowth, BundleId, DockItem } from "../ipc";
import {
  INSTANCE_ATTRIBUTES,
  SHRIVEL_MS,
  WITHER_TINT,
  type MossInstance,
  type MossOvergrowth,
  type MossyIcon,
  type ItemKey,
  type Drawn,
  type Shrivel,
  type Shrivels,
  artBox,
  glslConstants,
  fnv1a32,
  itemKeys,
  mossInstances,
  mossyIcons,
  parseOvergrowthOverride,
  mossOvergrowth,
  quantize,
  shrivelFrame,
  shrivelInstance,
  shrivelProgress,
  shrivelsAfter,
  step,
  toDeviceBox,
} from "./model";
import * as model from "./model";
import fragmentSource from "./moss.frag.glsl?raw";
import vertexSource from "./moss.vert.glsl?raw";

const TILE = { y: 918, width: 56, height: 56 };

const LAYER_ZERO = (): number => 0;

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

  it("keeps an overgrown app's sprites when intensity scales its overgrowth below 1", () => {
    const items = [item("ru.keepcoder.Telegram", 280)];

    expect(mossyIcons(items, [growth("ru.keepcoder.Telegram", 0.3, "overgrown")], null)).toMatchObject([
      { overgrowth: quantize(0.3), overgrown: true },
    ]);
  });

  it("draws nothing for an overgrown app sent at overgrowth 0, as intensity 0 sends it", () => {
    const items = [item("ru.keepcoder.Telegram", 280)];

    expect(mossyIcons(items, [growth("ru.keepcoder.Telegram", 0, "overgrown")], null)).toEqual([]);
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
  it("draws static moss whole and unwithered", () => {
    const [instance] = mossInstances([icon({ overgrowth: mossy(1), overgrown: true })], 2, 1964, LAYER_ZERO);

    expect(instance.wither).toBe(0);
    expect(instance.remaining).toBe(1);
  });

  it("gives each icon the layer of its bake, and leaves out an icon without one", () => {
    const discord = icon();
    const chess = icon({ key: "com.apple.Chess#0" as ItemKey, seed: fnv1a32("com.apple.Chess#0") });
    const telegram = icon({ key: "ru.keepcoder.Telegram#0" as ItemKey, seed: fnv1a32("ru.keepcoder.Telegram#0") });
    const layers = new Map<ItemKey, number>([
      [discord.key, 3],
      [telegram.key, 0],
    ]);

    const instances = mossInstances([discord, chess, telegram], 2, 1964, ({ key }) => layers.get(key));

    expect(instances.map(({ key, layer }) => [key, layer])).toEqual([
      [discord.key, 3],
      [telegram.key, 0],
    ]);
  });

  it("puts each icon's bake over its tile's width, centred on the art, in canvas device pixels", () => {
    const tile = { x: 711, y: 918, width: 47, height: 59 };

    const [instance] = mossInstances([icon({ tile })], 2, 1964, LAYER_ZERO);

    const art = toDeviceBox(artBox(tile), 2, 1964);
    expect(instance.box.width).toBeCloseTo(47 * 2, 9);
    expect(instance.box.height).toBeCloseTo(47 * 2, 9);
    expect(instance.box.x + instance.box.width / 2).toBeCloseTo(art.x + art.width / 2, 9);
    expect(instance.box.y + instance.box.height / 2).toBeCloseTo(art.y + art.height / 2, 9);
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

const GLSL_SIZES: Record<string, number> = { float: 1, vec2: 2, vec3: 3, vec4: 4 };

const layoutInstance: MossInstance = {
  key: "com.hnc.Discord#0" as ItemKey,
  box: { x: 10, y: 20, width: 74, height: 74 },
  layer: 3,
  wither: 0.25,
  remaining: 0.5,
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

  it("writes exactly `size` values per attribute: the box, the layer, the wither and what remains", () => {
    const values = INSTANCE_ATTRIBUTES.map((attribute) => attribute.values(layoutInstance));

    expect(values.map((written) => written.length)).toEqual(INSTANCE_ATTRIBUTES.map(({ size }) => size));
    expect(values.flat()).toEqual([10, 20, 74, 74, 3, 0.25, 0.5]);
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

  it("starts one for a drawn item sent at overgrowth 0 that keeps its stage, as intensity 0 sends it", () => {
    const drawn = mossyIcons([chess, discord], overgrownChess, null);
    const intensityZero = [growth("com.apple.Chess", 0, "overgrown"), growth("com.hnc.Discord", 0)];

    expect(shrivelling(drawn, [chess, discord], intensityZero)).toEqual([
      { icon: drawn[0], startMs: null },
      { icon: drawn[1], startMs: null },
    ]);
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
    return shrivelInstance(from, progress, 2, 1964, 0);
  }

  it("starts from the icon as the canvas last drew it", () => {
    expect(at(0)).toEqual(mossInstances([overgrown], 2, 1964, LAYER_ZERO)[0]);
  });

  it("leaves the icon no instance at progress 1, so it is clean", () => {
    expect(at(1)).toBeNull();
  });

  it("eases the front down slowly at first: a quarter of the way, 0.84375 of the moss remains", () => {
    expect(at(0.25)?.remaining).toBe(0.84375);
  });

  it("recedes the front all the way to the bottom edge, never rising", () => {
    const remaining = progresses.map((progress) => at(progress)?.remaining ?? 0);

    expect(remaining[0]).toBe(1);
    for (let index = 1; index < remaining.length; index++) {
      expect(remaining[index]).toBeLessThanOrEqual(remaining[index - 1]);
    }
    expect(remaining[99]).toBeLessThan(0.001);
  });

  it("recedes a patchy icon's front the same way as an overgrown one's", () => {
    const patchy = icon({ overgrowth: mossy(0.5) });

    for (const progress of progresses) {
      expect(at(progress, patchy)?.remaining).toBe(at(progress)?.remaining);
    }
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

  it("keeps the icon's key, box and bake", () => {
    const instance = at(0.5);
    const start = mossInstances([overgrown], 2, 1964, LAYER_ZERO)[0];

    expect([instance?.key, instance?.box, instance?.layer]).toEqual([start.key, start.box, start.layer]);
  });
});

describe("shrivelFrame", () => {
  const chessKey = "com.apple.Chess#0" as ItemKey;
  const chess = icon({ key: chessKey, seed: fnv1a32(chessKey), overgrowth: mossy(1), overgrown: true });
  const discord = icon({ tile: { x: 760, ...TILE } });

  it("starts a shrivel's clock on its first frame and draws it there as last drawn", () => {
    const { instances, running } = shrivelFrame(new Map([[chessKey, { icon: chess, startMs: null }]]), 5000, 2, 1964, LAYER_ZERO);

    expect(instances).toEqual(mossInstances([chess], 2, 1964, LAYER_ZERO));
    expect([...running.values()]).toEqual([{ icon: chess, startMs: 5000 }]);
  });

  it("draws a shrivel from the bake of the icon as last drawn, and ends one with no bake", () => {
    const shrivels = new Map([
      [chessKey, { icon: chess, startMs: 1000 }],
      [discord.key, { icon: discord, startMs: 1000 }],
    ]);
    const layerOf = ({ key, overgrowth }: MossyIcon) => (key === chessKey && overgrowth === 1 ? 5 : undefined);

    const { instances, running } = shrivelFrame(shrivels, 1100, 2, 1964, layerOf);

    expect(instances.map(({ layer }) => layer)).toEqual([5]);
    expect([...running.keys()]).toEqual([chessKey]);
  });

  it("draws concurrent shrivels each at its own progress", () => {
    const shrivels = new Map([
      [chessKey, { icon: chess, startMs: 1000 }],
      [discord.key, { icon: discord, startMs: 1300 }],
    ]);

    const { instances } = shrivelFrame(shrivels, 1300, 2, 1964, LAYER_ZERO);

    expect(instances).toEqual([shrivelInstance(chess, 0.5, 2, 1964, 0), shrivelInstance(discord, 0, 2, 1964, 0)]);
  });

  it("drops a shrivel at its end and draws nothing for it", () => {
    const shrivels = new Map([
      [chessKey, { icon: chess, startMs: 1000 }],
      [discord.key, { icon: discord, startMs: 1300 }],
    ]);

    const { instances, running } = shrivelFrame(shrivels, 1000 + SHRIVEL_MS, 2, 1964, LAYER_ZERO);

    expect(instances).toEqual([shrivelInstance(discord, 0.5, 2, 1964, 0)]);
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
