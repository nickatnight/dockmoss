import { type LevelStep, levelStep } from "./generator";
import type { MossyIcon } from "./model";

export const BAKE_POINTS = 128;

export function bakeSide(pixelRatio: number): number {
  return Math.round(BAKE_POINTS * pixelRatio);
}

export type BakeKey = string & { readonly __brand: "BakeKey" };

export interface BakeRequest {
  key: BakeKey;
  seed: number;
  step: LevelStep;
  overgrown: boolean;
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

export interface Atlas {
  side: number;
  capacity: number;
  layers: ReadonlyMap<BakeKey, number>;
}

export const EMPTY_ATLAS: Atlas = { side: 0, capacity: 0, layers: new Map() };

const MIN_LAYERS = 8;

export interface AtlasPlan {
  atlas: Atlas;
  bakes: { request: BakeRequest; layer: number }[];
  reallocate: boolean;
}

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
