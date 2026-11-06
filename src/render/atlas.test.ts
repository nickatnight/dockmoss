import { describe, expect, it } from "vitest";

import { type Atlas, type BakeRequest, EMPTY_ATLAS, bakeRequest, planAtlas } from "./atlas";
import { type ItemKey, type MossOvergrowth, type MossyIcon, fnv1a32, mossOvergrowth } from "./model";

function icon(key: string, overgrowth: number, overrides: Partial<MossyIcon> = {}): MossyIcon {
  return {
    key: key as ItemKey,
    seed: fnv1a32(key),
    tile: { x: 617, y: 918, width: 47, height: 59 },
    overgrowth: mossOvergrowth(overgrowth) as MossOvergrowth,
    overgrown: false,
    ...overrides,
  };
}

const discord = icon("com.hnc.Discord#0", 0.6);
const chess = icon("com.apple.Chess#0", 1, { overgrown: true });
const telegram = icon("ru.keepcoder.Telegram#0", 0.3);

function plan(atlas: Atlas, icons: readonly MossyIcon[], pixelRatio = 2) {
  return planAtlas(
    atlas,
    icons.map((each) => bakeRequest(each, pixelRatio)),
    Math.round(128 * pixelRatio),
  );
}

const baked = (requests: readonly { request: BakeRequest }[]) => requests.map(({ request }) => request.key);

describe("bakeRequest", () => {
  it("bakes at the largest magnified tile, 128 pt, times the backing scale", () => {
    expect(bakeRequest(discord, 2).side).toBe(256);
    expect(bakeRequest(discord, 1).side).toBe(128);
  });

  it("asks for the same bake when only the tile moves, or the overgrowth moves within its step", () => {
    const moved = { ...discord, tile: { x: 600, y: 905, width: 66, height: 78 } };
    const sameStep = icon("com.hnc.Discord#0", 0.59);

    expect(bakeRequest(moved, 2).key).toBe(bakeRequest(discord, 2).key);
    expect(bakeRequest(sameStep, 2).key).toBe(bakeRequest(discord, 2).key);
  });

  it.each([
    ["another level step", icon("com.hnc.Discord#0", 0.7)],
    ["the Overgrown stage, which adds props", { ...discord, overgrown: true }],
    ["another item", icon("com.hnc.Discord#1", 0.6)],
  ])("asks for another bake for %s", (_, other) => {
    expect(bakeRequest(other, 2).key).not.toBe(bakeRequest(discord, 2).key);
  });

  it("asks for another bake at another backing scale", () => {
    expect(bakeRequest(discord, 1).key).not.toBe(bakeRequest(discord, 2).key);
  });
});

describe("planAtlas", () => {
  it("bakes every icon into its own layer on the first frame, and makes the texture", () => {
    const first = plan(EMPTY_ATLAS, [discord, chess]);

    expect(baked(first.bakes)).toEqual([bakeRequest(discord, 2).key, bakeRequest(chess, 2).key]);
    expect(first.bakes.map(({ layer }) => layer)).toEqual([0, 1]);
    expect(first.reallocate).toBe(true);
    expect(first.atlas.side).toBe(256);
  });

  it("bakes nothing, and keeps the texture, when the same bakes are wanted again", () => {
    const first = plan(EMPTY_ATLAS, [discord, chess]);
    const moved = { ...discord, tile: { ...discord.tile, x: 640 } };

    const next = plan(first.atlas, [moved, chess]);

    expect(next.bakes).toEqual([]);
    expect(next.reallocate).toBe(false);
    expect(next.atlas).toEqual(first.atlas);
  });

  it("evicts a bake no longer wanted and gives its layer to the next new one", () => {
    const first = plan(EMPTY_ATLAS, [discord, chess]);

    const next = plan(first.atlas, [chess, telegram]);

    expect([...next.atlas.layers.keys()]).toEqual([bakeRequest(chess, 2).key, bakeRequest(telegram, 2).key]);
    expect(next.bakes).toEqual([{ request: bakeRequest(telegram, 2), layer: 0 }]);
    expect(next.atlas.layers.get(bakeRequest(chess, 2).key)).toBe(1);
    expect(next.reallocate).toBe(false);
  });

  it("re-bakes an icon whose level step changed, and evicts its old bake", () => {
    const first = plan(EMPTY_ATLAS, [discord]);
    const grown = icon("com.hnc.Discord#0", 0.8);

    const next = plan(first.atlas, [grown]);

    expect(baked(next.bakes)).toEqual([bakeRequest(grown, 2).key]);
    expect([...next.atlas.layers.keys()]).toEqual([bakeRequest(grown, 2).key]);
  });

  it("re-bakes everything, and makes the texture again, when the backing scale changes", () => {
    const first = plan(EMPTY_ATLAS, [discord, chess]);

    const next = plan(first.atlas, [discord, chess], 1);

    expect(baked(next.bakes)).toEqual([bakeRequest(discord, 1).key, bakeRequest(chess, 1).key]);
    expect(next.reallocate).toBe(true);
    expect(next.atlas.side).toBe(128);
  });

  it("grows the texture when the layers run out, keeping every kept bake in its layer", () => {
    const many = Array.from({ length: 8 }, (_, index) => icon(`app.${index}#0`, 0.5));
    const first = plan(EMPTY_ATLAS, many);

    const next = plan(first.atlas, [...many, telegram]);

    expect(first.atlas.capacity).toBe(8);
    expect(next.atlas.capacity).toBe(16);
    expect(next.reallocate).toBe(true);
    expect(next.bakes).toEqual([{ request: bakeRequest(telegram, 2), layer: 8 }]);
    for (const [key, layer] of first.atlas.layers) {
      expect(next.atlas.layers.get(key)).toBe(layer);
    }
  });

  it("bakes an icon wanted twice in a frame only once", () => {
    const next = plan(EMPTY_ATLAS, [discord, discord]);

    expect(next.bakes).toHaveLength(1);
  });

  it("keeps the texture, empty, when nothing is wanted", () => {
    const first = plan(EMPTY_ATLAS, [discord]);

    const next = plan(first.atlas, []);

    expect(next.bakes).toEqual([]);
    expect(next.reallocate).toBe(false);
    expect(next.atlas.layers.size).toBe(0);
  });
});
