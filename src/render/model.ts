// Pure: which Dock item gets moss, its key and seed, and when to redraw.
// No DOM, no WebGL and no clock, so `model.test.ts` runs it in Node.

import type { AppGrowth, BundleId, DockItem, LogicalRect } from "../ipc";

export type ItemKey = string & { readonly __brand: "ItemKey" };

export interface Scene {
  key: ItemKey;
  seed: number;
  rect: LogicalRect;
  overgrowth: number;
}

export const OVERGROWTH_STEPS = 256;

export function quantize(overgrowth: number): number {
  return Math.round(overgrowth * OVERGROWTH_STEPS) / OVERGROWTH_STEPS;
}

// Noise coordinates to move each item's pattern by, at most, on each axis.
const SEED_OFFSET_RANGE = 64;

export function seedOffset(seed: number): [number, number] {
  return [
    ((seed & 0xffff) / 0x10000) * SEED_OFFSET_RANGE,
    ((seed >>> 16) / 0x10000) * SEED_OFFSET_RANGE,
  ];
}

export function itemKeys(items: readonly DockItem[]): ItemKey[] {
  const seen = new Map<BundleId, number>();
  return items.map(({ bundleId }) => {
    const occurrence = seen.get(bundleId) ?? 0;
    seen.set(bundleId, occurrence + 1);
    return `${bundleId}#${occurrence}` as ItemKey;
  });
}

export function fnv1a32(text: string): number {
  let hash = 0x811c9dc5;
  for (const byte of new TextEncoder().encode(text)) {
    hash = Math.imul(hash ^ byte, 0x01000193);
  }
  return hash >>> 0;
}

export function mostOvergrownItem(
  items: readonly DockItem[],
  growth: readonly AppGrowth[],
  overgrowthOverride: number | null,
): Scene | null {
  const byBundleId = new Map(
    growth.map(({ bundleId, overgrowth }) => [bundleId, quantize(overgrowth)]),
  );
  const keys = itemKeys(items);
  let best: Scene | null = null;
  for (const [index, { bundleId, rect }] of items.entries()) {
    const overgrowth = byBundleId.get(bundleId) ?? 0;
    if (best === null || overgrowth > best.overgrowth) {
      const key = keys[index];
      best = { key, seed: fnv1a32(key), rect, overgrowth };
    }
  }
  if (best === null || overgrowthOverride === null) {
    return best;
  }
  return { ...best, overgrowth: quantize(overgrowthOverride) };
}

export interface Changes {
  geometry: boolean;
  moss: boolean;
}

export type Step =
  | { kind: "none" }
  | { kind: "clear" }
  | { kind: "draw"; changes: Changes };

export function step(drawn: Scene | null, next: Scene | null): Step {
  if (next === null) {
    return drawn === null ? { kind: "none" } : { kind: "clear" };
  }
  if (drawn === null || drawn.key !== next.key) {
    return { kind: "draw", changes: { geometry: true, moss: true } };
  }
  const geometry = !sameRect(drawn.rect, next.rect);
  const moss = drawn.overgrowth !== next.overgrowth;
  if (!geometry && !moss) {
    return { kind: "none" };
  }
  return { kind: "draw", changes: { geometry, moss } };
}

function sameRect(a: LogicalRect, b: LogicalRect): boolean {
  return a.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height;
}

/**
 * The AX rect is the whole Dock tile, taller than the icon art and with the
 * running dot below it. The art is a square, centred in the tile, whose side
 * measured 0.785 to 0.808 of the tile's width, normal and magnified.
 */
const ART_SIDE_PER_TILE_WIDTH = 0.79;

export function artBox(tile: LogicalRect): LogicalRect {
  const side = tile.width * ART_SIDE_PER_TILE_WIDTH;
  return {
    x: tile.x + (tile.width - side) / 2,
    y: tile.y + (tile.height - side) / 2,
    width: side,
    height: side,
  };
}

/** A rect in canvas device pixels, measured from the bottom-left as GL does. */
export interface DeviceBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

export function toDeviceBox(
  rect: LogicalRect,
  pixelRatio: number,
  canvasHeight: number,
): DeviceBox {
  return {
    x: rect.x * pixelRatio,
    y: canvasHeight - (rect.y + rect.height) * pixelRatio,
    width: rect.width * pixelRatio,
    height: rect.height * pixelRatio,
  };
}

export type OvergrowthOverride =
  | { kind: "unset" }
  | { kind: "set"; overgrowth: number }
  | { kind: "invalid"; raw: string };

export function parseOvergrowthOverride(raw: string | undefined): OvergrowthOverride {
  if (raw === undefined || raw === "") {
    return { kind: "unset" };
  }
  const overgrowth = Number(raw);
  if (!/^(\d+(\.\d*)?|\.\d+)$/.test(raw) || overgrowth > 1) {
    return { kind: "invalid", raw };
  }
  return { kind: "set", overgrowth };
}
