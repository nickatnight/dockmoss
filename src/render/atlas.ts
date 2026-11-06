import { type LevelStep, levelStep } from "./generator";
import type { ItemKey, MossyIcon } from "./model";

export const MAX_MAGNIFIED_TILE_POINTS = 128;

/** A software bake takes about 12 ms, so a frame makes at most this many; the rest wait for the next frames. */
export const BAKES_PER_FRAME = 2;

export function bakeSide(pixelRatio: number): number {
  return Math.round(MAX_MAGNIFIED_TILE_POINTS * pixelRatio);
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

/** A bake a frame wants, for the Dock item that draws it. */
export interface Want {
  item: ItemKey;
  request: BakeRequest;
}

export interface Atlas {
  side: number;
  capacity: number;
  layers: ReadonlyMap<BakeKey, number>;
  /** The bake each item draws now: its wanted one once made, its previous one until then. */
  current: ReadonlyMap<ItemKey, BakeKey>;
}

export const EMPTY_ATLAS: Atlas = { side: 0, capacity: 0, layers: new Map(), current: new Map() };

const MIN_LAYERS = 8;

export interface AtlasPlan {
  atlas: Atlas;
  bakes: { request: BakeRequest; layer: number }[];
  reallocate: boolean;
  /** Some wanted bakes wait for a later frame. */
  pending: boolean;
}

export function planAtlas(atlas: Atlas, wanted: readonly Want[], side: number, budget: number): AtlasPlan {
  const sameSide = atlas.side === side;
  const previous = sameSide ? atlas : EMPTY_ATLAS;
  const byItem = new Map<ItemKey, BakeRequest>();
  for (const { item, request } of wanted) {
    if (!byItem.has(item)) {
      byItem.set(item, request);
    }
  }
  const kept = new Map<BakeKey, number>();
  const current = new Map<ItemKey, BakeKey>();
  const fresh: Want[] = [];
  for (const [item, request] of byItem) {
    const layer = previous.layers.get(request.key);
    if (layer === undefined) {
      fresh.push({ item, request });
    } else {
      kept.set(request.key, layer);
      current.set(item, request.key);
    }
  }
  const now = fresh.slice(0, budget);
  const waiting = fresh.slice(budget);
  for (const { item } of waiting) {
    const old = previous.current.get(item);
    const layer = old === undefined ? undefined : previous.layers.get(old);
    if (old !== undefined && layer !== undefined) {
      kept.set(old, layer);
      current.set(item, old);
    }
  }
  let capacity = Math.max(previous.capacity, MIN_LAYERS);
  while (capacity < kept.size + now.length) {
    capacity *= 2;
  }
  const used = new Set(kept.values());
  const layers = new Map(kept);
  const bakes: AtlasPlan["bakes"] = [];
  let free = 0;
  for (const { item, request } of now) {
    while (used.has(free)) {
      free++;
    }
    used.add(free);
    layers.set(request.key, free);
    current.set(item, request.key);
    bakes.push({ request, layer: free });
  }
  return {
    atlas: { side, capacity, layers, current },
    bakes,
    reallocate: !sameSide || capacity !== atlas.capacity,
    pending: waiting.length > 0,
  };
}
