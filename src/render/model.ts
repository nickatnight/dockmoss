import type { AppGrowth, BundleId, DockItem, LogicalRect } from "../ipc";

export type ItemKey = string & { readonly __brand: "ItemKey" };

export type QuantizedOvergrowth = number & { readonly __brand: "QuantizedOvergrowth" };

export interface MossyIcon {
  key: ItemKey;
  seed: number;
  tile: LogicalRect;
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
): MossyIcon[] {
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
export function step(drawn: readonly MossyIcon[] | null, next: readonly MossyIcon[]): Step {
  if (next.length === 0) {
    return drawn !== null && drawn.length === 0 ? { kind: "none" } : { kind: "clear" };
  }
  if (drawn !== null && sameIcons(drawn, next)) {
    return { kind: "none" };
  }
  return { kind: "draw" };
}

function sameIcons(a: readonly MossyIcon[], b: readonly MossyIcon[]): boolean {
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

export const CORNER_RADIUS_PER_SIDE = 0.225;
export const LEFT_CORNER = [0.066, 0.066] as const;
export const RIGHT_CORNER = [0.934, 0.066] as const;
export const BOTTOM_DISTANCE_SCALE = 1.7;
export const CORNER_DISTANCE_SCALE = 1.25;
export const MAX_REACH = 0.62;
export const REACH_EXPONENT = 0.8;
export const RAGGED_CAP = 0.24;
export const RAGGED_PER_REACH = 1.2;
export const TUFT_CAP = 0.05;
export const TUFT_PER_REACH = 0.3;
export const FBM_OCTAVES = 5;
export const FRONT_FUZZ_WIDTH = 0.04;
export const CONTACT_SHADOW_WIDTH = 0.03;
export const SWAY_REACH = 0.012;

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
 * The highest point on column `x` where moss or its contact shadow can touch
 * any pixel, as a fraction of the art height. Mirrors `mossFront`, `mossCover`
 * and `contactShadow` with every noise term at its peak (fbm at the sum of its
 * octave amplitudes, the tuft noise at 1) and sway at its highest. Rounded up
 * to the next 1/1000.
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
    SWAY_REACH +
    Math.max(FRONT_FUZZ_WIDTH, CONTACT_SHADOW_WIDTH);
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
    CONTACT_SHADOW_WIDTH,
    SWAY_REACH,
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

/** A mushroom or flower on an overgrown icon. MossyIcon space; `size` is its box's side. */
export interface Sprite {
  kind: "mushroom" | "flower";
  x: number;
  y: number;
  size: number;
}

export const MAX_SPRITES = 3;
const SPRITE_X = [0.2, 0.8] as const;
const SPRITE_Y = [0.16, 0.3] as const;
const SPRITE_SIZE = [0.13, 0.19] as const;
const SPRITE_TRIES = 12;

function mulberry32(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = Math.imul(state ^ (state >>> 15), state | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 0x100000000;
  };
}

function between([low, high]: readonly [number, number], random: () => number): number {
  return low + (high - low) * random();
}

/** 1 to 3 sprites that do not overlap, chosen and placed from the icon's seed. */
export function spritesFor(seed: number): Sprite[] {
  const random = mulberry32(seed);
  const wanted = 1 + Math.floor(random() * MAX_SPRITES);
  const sprites: Sprite[] = [];
  for (let attempt = 0; attempt < SPRITE_TRIES && sprites.length < wanted; attempt++) {
    const candidate: Sprite = {
      kind: random() < 0.5 ? "mushroom" : "flower",
      x: between(SPRITE_X, random),
      y: between(SPRITE_Y, random),
      size: between(SPRITE_SIZE, random),
    };
    const clear = sprites.every(
      (other) =>
        Math.max(Math.abs(other.x - candidate.x), Math.abs(other.y - candidate.y)) >=
        (other.size + candidate.size) / 2,
    );
    if (clear) {
      sprites.push(candidate);
    }
  }
  return sprites;
}

/**
 * Where an icon's moss sits between warmer (-1, olive) and cooler (1, blue
 * green), from bits of its seed that seedOffset does not use alone.
 */
export function mossHue(seed: number): number {
  return ((Math.imul(seed, 0x9e3779b1) >>> 24) / 0xff) * 2 - 1;
}

export interface MossInstance {
  box: DeviceBox;
  seed: number;
  overgrowth: QuantizedOvergrowth;
  hue: number;
  sprites: readonly Sprite[];
}

export function mossInstances(
  icons: readonly MossyIcon[],
  pixelRatio: number,
  canvasHeight: number,
): MossInstance[] {
  return icons.map(({ tile, seed, overgrowth, overgrown }) => ({
    box: toDeviceBox(artBox(tile), pixelRatio, canvasHeight),
    seed,
    overgrowth,
    hue: mossHue(seed),
    sprites: overgrown ? spritesFor(seed) : [],
  }));
}

export const SWAY_MIN_OVERGROWTH = 0.5;
export const SWAY_MIN_FRAME_INTERVAL_MS = 33;
export const SWAY_CYCLE_SECONDS = 60;

export function swaying(icons: readonly MossyIcon[]): boolean {
  return icons.some(({ overgrowth }) => overgrowth >= SWAY_MIN_OVERGROWTH);
}

export type SwayFlag = { kind: "set"; sway: boolean } | { kind: "invalid"; raw: string };

/** The dev-only VITE_DOCKMOSS_SWAY: "1" turns sway on; unset, "" or "0" leaves it off. */
export function parseSwayFlag(raw: string | undefined): SwayFlag {
  if (raw === undefined || raw === "" || raw === "0") {
    return { kind: "set", sway: false };
  }
  return raw === "1" ? { kind: "set", sway: true } : { kind: "invalid", raw };
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
