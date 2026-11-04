// Pure: which Dock items get moss, their keys and seeds, and when to redraw.
// No DOM, no WebGL and no clock, so `model.test.ts` runs it in Node.

import type { AppGrowth, BundleId, DockItem, LogicalRect } from "../ipc";

export type ItemKey = string & { readonly __brand: "ItemKey" };

export type QuantizedOvergrowth = number & { readonly __brand: "QuantizedOvergrowth" };

/** A Dock item with moss on it. */
export interface Icon {
  key: ItemKey;
  seed: number;
  tile: LogicalRect;
  /** Above 0: clean items get no icon. */
  overgrowth: QuantizedOvergrowth;
  overgrown: boolean;
}

const OVERGROWTH_STEPS = 256;

export function quantize(overgrowth: number): QuantizedOvergrowth {
  return (Math.round(overgrowth * OVERGROWTH_STEPS) / OVERGROWTH_STEPS) as QuantizedOvergrowth;
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

const utf8 = new TextEncoder();

export function fnv1a32(text: string): number {
  let hash = 0x811c9dc5;
  for (const byte of utf8.encode(text)) {
    hash = Math.imul(hash ^ byte, 0x01000193);
  }
  return hash >>> 0;
}

/**
 * Every Dock item with visible moss, in Dock order. The dev override gives
 * every item its overgrowth, and makes every item overgrown at 1.
 */
export function mossyIcons(
  items: readonly DockItem[],
  growth: readonly AppGrowth[],
  overgrowthOverride: number | null,
): Icon[] {
  const byBundleId = new Map(growth.map((app) => [app.bundleId, app]));
  const keys = itemKeys(items);
  return items.flatMap(({ bundleId, rect }, index) => {
    const app = byBundleId.get(bundleId);
    const overgrowth = quantize(overgrowthOverride ?? app?.overgrowth ?? 0);
    if (overgrowth <= 0) {
      return [];
    }
    const overgrown =
      overgrowthOverride === null ? app?.stage === "overgrown" : overgrowthOverride === 1;
    const key = keys[index];
    return [{ key, seed: fnv1a32(key), tile: rect, overgrowth, overgrown }];
  });
}

export type Step = { kind: "none" } | { kind: "clear" } | { kind: "draw" };

/**
 * `drawn` is what the canvas shows: null when that is unknown (after a
 * resize or a context restore), and empty when it shows nothing.
 */
export function step(drawn: readonly Icon[] | null, next: readonly Icon[]): Step {
  if (next.length === 0) {
    return drawn !== null && drawn.length === 0 ? { kind: "none" } : { kind: "clear" };
  }
  if (drawn !== null && sameIcons(drawn, next)) {
    return { kind: "none" };
  }
  return { kind: "draw" };
}

function sameIcons(a: readonly Icon[], b: readonly Icon[]): boolean {
  return (
    a.length === b.length &&
    a.every((icon, index) => {
      const other = b[index];
      return (
        icon.key === other.key &&
        icon.overgrowth === other.overgrowth &&
        icon.overgrown === other.overgrown &&
        sameRect(icon.tile, other.tile)
      );
    })
  );
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

// The moss shape, in icon space: 0 to 1 across the art box, 0 at its bottom.
// moss.frag.glsl gets these as generated `const` lines (glslConstants), so
// worstCaseFront and the shader share one source.

/** The macOS icon shape, approximated as a rounded rect. */
export const CORNER_RADIUS_PER_SIDE = 0.225;
// Growth starts at the bottom edge and at the two bottom corners, where the
// corner arcs sit. Distance from the bottom edge counts for more, so the
// corners lead.
export const LEFT_CORNER = [0.066, 0.066] as const;
export const RIGHT_CORNER = [0.934, 0.066] as const;
export const BOTTOM_DISTANCE_SCALE = 1.7;
export const CORNER_DISTANCE_SCALE = 1.25;
/** How far growth reaches at overgrowth 1, and the curve up to it. */
export const MAX_REACH = 0.62;
export const REACH_EXPONENT = 0.8;
/** The front's noise: at most this far, and at most this much per unit of reach. */
export const RAGGED_CAP = 0.24;
export const RAGGED_PER_REACH = 1.2;
export const TUFT_CAP = 0.05;
export const TUFT_PER_REACH = 0.3;
export const FBM_OCTAVES = 5;
/** Grain can cover pixels this far beyond the front. */
export const FRONT_FUZZ_WIDTH = 0.04;

/** Mirrors `growthDistance` in moss.frag.glsl. */
function growthDistance(x: number, y: number): number {
  const fromBottom = y * BOTTOM_DISTANCE_SCALE;
  const fromCorner =
    CORNER_DISTANCE_SCALE *
    Math.min(
      Math.hypot(x - LEFT_CORNER[0], y - LEFT_CORNER[1]),
      Math.hypot(x - RIGHT_CORNER[0], y - RIGHT_CORNER[1]),
    );
  return Math.min(fromBottom, fromCorner);
}

const FRONT_SCAN_STEPS = 1000;

/**
 * The highest point on column `x` where moss can cover any pixel, as a
 * fraction of the art height. Mirrors `mossFront` and `mossCover` with every
 * noise term at its peak: fbm at the sum of its octave amplitudes, the tuft
 * noise at 1. Rounded up to the next 1/1000.
 */
export function worstCaseFront(x: number, overgrowth: number): number {
  if (overgrowth <= 0) {
    return 0;
  }
  const reach = MAX_REACH * overgrowth ** REACH_EXPONENT;
  const raggedPeak = 0.5 - 0.5 ** FBM_OCTAVES;
  const tuftPeak = 0.5;
  const front =
    reach +
    raggedPeak * Math.min(RAGGED_CAP, reach * RAGGED_PER_REACH) +
    tuftPeak * Math.min(TUFT_CAP, reach * TUFT_PER_REACH) +
    FRONT_FUZZ_WIDTH;
  for (let step = FRONT_SCAN_STEPS; step >= 0; step--) {
    if (growthDistance(x, step / FRONT_SCAN_STEPS) <= front) {
      return Math.min(1, (step + 1) / FRONT_SCAN_STEPS);
    }
  }
  return 0;
}

function glslFloat(value: number): string {
  return Number.isInteger(value) ? value.toFixed(1) : String(value);
}

/** The shape constants above as GLSL `const` declarations, one per line. */
export function glslConstants(): string {
  const floats = {
    CORNER_RADIUS_PER_SIDE,
    BOTTOM_DISTANCE_SCALE,
    CORNER_DISTANCE_SCALE,
    MAX_REACH,
    REACH_EXPONENT,
    RAGGED_CAP,
    RAGGED_PER_REACH,
    TUFT_CAP,
    TUFT_PER_REACH,
    FRONT_FUZZ_WIDTH,
  };
  const vec2s = { LEFT_CORNER, RIGHT_CORNER };
  return [
    ...Object.entries(floats).map(([name, value]) => `const float ${name} = ${glslFloat(value)};`),
    ...Object.entries(vec2s).map(
      ([name, [x, y]]) => `const vec2 ${name} = vec2(${glslFloat(x)}, ${glslFloat(y)});`,
    ),
    `const int FBM_OCTAVES = ${FBM_OCTAVES};`,
  ].join("\n");
}

/** One icon's moss, ready for the GPU: one instance of the draw. */
export interface MossInstance {
  box: DeviceBox;
  seed: number;
  overgrowth: QuantizedOvergrowth;
}

export function mossInstances(
  icons: readonly Icon[],
  pixelRatio: number,
  canvasHeight: number,
): MossInstance[] {
  return icons.map(({ tile, seed, overgrowth }) => ({
    box: toDeviceBox(artBox(tile), pixelRatio, canvasHeight),
    seed,
    overgrowth,
  }));
}

export type OvergrowthOverride =
  | { kind: "unset" }
  | { kind: "set"; overgrowth: number }
  | { kind: "invalid"; raw: string };

// Digits with an optional fraction, or a bare fraction: no sign, exponent or hex.
const PLAIN_DECIMAL = /^(\d+(\.\d*)?|\.\d+)$/;

export function parseOvergrowthOverride(raw: string | undefined): OvergrowthOverride {
  if (raw === undefined || raw === "") {
    return { kind: "unset" };
  }
  const overgrowth = Number(raw);
  if (!PLAIN_DECIMAL.test(raw) || overgrowth > 1) {
    return { kind: "invalid", raw };
  }
  return { kind: "set", overgrowth };
}
