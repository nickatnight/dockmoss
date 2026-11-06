import { type LevelStep, levelStep } from "./generator";
import type { MossyIcon } from "./model";

/**
 * The largest magnified Dock tile, in points. A bake covers it at the backing
 * scale and is sampled with mipmaps, so a magnification never re-bakes.
 */
export const BAKE_POINTS = 128;

export function bakeSide(pixelRatio: number): number {
  return Math.round(BAKE_POINTS * pixelRatio);
}

/** One bake's cache key: the item key, level step, stage and size. The seed comes from the item key. */
export type BakeKey = string & { readonly __brand: "BakeKey" };

export interface BakeRequest {
  key: BakeKey;
  seed: number;
  step: LevelStep;
  /** Props grow only on an Overgrown item, so the stage is part of the key. */
  overgrown: boolean;
  /** The bake's side in device pixels. */
  side: number;
}

export function bakeRequest(icon: MossyIcon, pixelRatio: number): BakeRequest {
  const step = levelStep(icon.overgrowth);
  const side = bakeSide(pixelRatio);
  const stage = icon.overgrown ? "overgrown" : "growing";
  return {
    key: `${icon.key}|${step}|${stage}|${side}` as BakeKey,
    seed: icon.seed,
    step,
    overgrown: icon.overgrown,
    side,
  };
}

/** The bakes held in the texture: one layer each, of `side` device pixels square. */
export interface Atlas {
  side: number;
  capacity: number;
  layers: ReadonlyMap<BakeKey, number>;
}

export const EMPTY_ATLAS: Atlas = { side: 0, capacity: 0, layers: new Map() };

/** The texture's first size, in layers; it doubles when they run out. */
const MIN_LAYERS = 8;

export interface AtlasPlan {
  atlas: Atlas;
  /** The bakes to make now, each with the layer it goes in. */
  bakes: { request: BakeRequest; layer: number }[];
  /** The texture must be made again at the atlas's side and capacity, and every bake uploaded to it. */
  reallocate: boolean;
}

/**
 * The atlas that holds exactly the bakes `wanted`. A bake already held keeps
 * its layer; one no longer wanted is evicted, so a key that left the Dock or
 * moved to another step frees its layer. New bakes take the lowest free
 * layers. A new `side` (a backing scale change) starts again from empty.
 */
export function planAtlas(atlas: Atlas, wanted: readonly BakeRequest[], side: number): AtlasPlan {
  const unique = [...new Map(wanted.map((request) => [request.key, request])).values()];
  const sameSide = atlas.side === side;
  const kept = new Map<BakeKey, number>();
  for (const { key } of unique) {
    const layer = sameSide ? atlas.layers.get(key) : undefined;
    if (layer !== undefined) {
      kept.set(key, layer);
    }
  }
  let capacity = Math.max(sameSide ? atlas.capacity : 0, MIN_LAYERS);
  while (capacity < unique.length) {
    capacity *= 2;
  }
  const used = new Set(kept.values());
  const layers = new Map<BakeKey, number>();
  const bakes: AtlasPlan["bakes"] = [];
  let free = 0;
  for (const request of unique) {
    const held = kept.get(request.key);
    if (held !== undefined) {
      layers.set(request.key, held);
      continue;
    }
    while (used.has(free)) {
      free++;
    }
    used.add(free);
    layers.set(request.key, free);
    bakes.push({ request, layer: free });
  }
  return {
    atlas: { side, capacity, layers },
    bakes,
    reallocate: !sameSide || capacity !== atlas.capacity,
  };
}
