import type { AppGrowth, BundleId, DockItem, LogicalRect } from "../ipc";

export type ItemKey = string & { readonly __brand: "ItemKey" };

export type QuantizedOvergrowth = number & { readonly __brand: "QuantizedOvergrowth" };

/** A quantized overgrowth above 0: the only kind that gets moss. */
export type MossOvergrowth = QuantizedOvergrowth & { readonly __mossy: true };

export interface MossyIcon {
  key: ItemKey;
  seed: number;
  tile: LogicalRect;
  overgrowth: MossOvergrowth;
  overgrown: boolean;
}

const OVERGROWTH_STEPS = 256;

export function quantize(overgrowth: number): QuantizedOvergrowth {
  return (Math.round(overgrowth * OVERGROWTH_STEPS) / OVERGROWTH_STEPS) as QuantizedOvergrowth;
}

export function mossOvergrowth(overgrowth: number): MossOvergrowth | null {
  const quantized = quantize(overgrowth);
  return quantized > 0 ? (quantized as MossOvergrowth) : null;
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
    const overgrowth = mossOvergrowth(overgrowthOverride ?? app?.overgrowth ?? 0);
    if (overgrowth === null) {
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
 * What the canvas shows: its static icons and whether its last frame drew a
 * shrivel, or "unknown" after a resize or a context restore.
 */
export type Drawn = { icons: readonly MossyIcon[]; showsShrivel: boolean } | "unknown";

export const NOTHING_DRAWN: Drawn = { icons: [], showsShrivel: false };

/**
 * What the canvas needs to show the static icons `next` while `shrivels` run.
 * A shrivel frame that no running shrivel will redraw needs a redraw, or a
 * clear when nothing is mossy. While shrivels run, their loop redraws them.
 */
export function step(drawn: Drawn, next: readonly MossyIcon[], shrivels: Shrivels): Step {
  const running = shrivels.size > 0;
  const shownIcons = drawn === "unknown" || (drawn.showsShrivel && !running) ? null : drawn.icons;
  if (next.length === 0 && !running) {
    return shownIcons !== null && shownIcons.length === 0 ? { kind: "none" } : { kind: "clear" };
  }
  return shownIcons !== null && sameIcons(shownIcons, next) ? { kind: "none" } : { kind: "draw" };
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
export const ART_SIDE_PER_TILE_WIDTH = 0.79;

export function artBox(tile: LogicalRect): LogicalRect {
  const side = tile.width * ART_SIDE_PER_TILE_WIDTH;
  return {
    x: tile.x + (tile.width - side) / 2,
    y: tile.y + (tile.height - side) / 2,
    width: side,
    height: side,
  };
}

/**
 * The square a bake covers, in the tile's points: the tile's width, centred on
 * the art box. The moss may overhang the art by half the gap to each
 * neighbouring art box, so it stays inside its own tile.
 */
export function bakeBox(tile: LogicalRect): LogicalRect {
  const art = artBox(tile);
  const overhang = (tile.width - art.width) / 2;
  return { x: art.x - overhang, y: art.y - overhang, width: tile.width, height: tile.width };
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

/** The icon art's corner radius, as a share of its side. */
export const CORNER_RADIUS_PER_SIDE = 0.225;

function glslFloat(value: number): string {
  return Number.isInteger(value) ? value.toFixed(1) : String(value);
}

/** The shader's constants, as GLSL `const` declarations, one per line. */
export function glslConstants(): string {
  const floats = { MIP_LOD_BIAS, SHRIVEL_FRONT_WIDTH };
  const vectors = { WITHER_TINT };
  return [
    ...Object.entries(floats).map(([name, value]) => `const float ${name} = ${glslFloat(value)};`),
    ...Object.entries(vectors).map(([name, components]) => {
      const type = `vec${components.length}`;
      return `const ${type} ${name} = ${type}(${components.map(glslFloat).join(", ")});`;
    }),
  ].join("\n");
}

export function mulberry32(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = Math.imul(state ^ (state >>> 15), state | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 0x100000000;
  };
}

export interface MossInstance {
  key: ItemKey;
  /** The bake's box: the art box and its overhang. */
  box: DeviceBox;
  /** The atlas layer that holds the icon's bake. */
  layer: number;
  /** 0 for living moss, 1 for moss browned all the way by a shrivel. */
  wither: number;
  /** How much of the bake a shrivel leaves, from 1 (all) down to 0, as a front easing down. */
  remaining: number;
}

/** The atlas layer holding an icon's bake, or undefined when it has none. */
export type LayerOf = (icon: MossyIcon) => number | undefined;

/** An instance for each icon that has a bake, in order. */
export function mossInstances(
  icons: readonly MossyIcon[],
  pixelRatio: number,
  canvasHeight: number,
  layerOf: LayerOf,
): MossInstance[] {
  return icons.flatMap((icon) => {
    const layer = layerOf(icon);
    if (layer === undefined) {
      return [];
    }
    const box = toDeviceBox(bakeBox(icon.tile), pixelRatio, canvasHeight);
    return [{ key: icon.key, box, layer, wither: 0, remaining: 1 }];
  });
}

export const SHRIVEL_MS = 600;
export const SHRIVEL_WITHER_END = 0.4;
export const WITHER_TINT = [1, 0.74, 0.42] as const;
/**
 * Samples the atlas half a mip level sharper than the GPU would pick. At 0 the
 * moss kept about 0.7 of the high-frequency detail of a bake made at the drawn
 * size, resting and magnified, and read soft beside the icon; at -0.5 about
 * 0.9; at -1 it went past the ideal at rest (aliasing). Job 017, tools/look/sharp.mjs.
 */
export const MIP_LOD_BIAS = -0.5;
/** How soft a shrivel's front is, as a share of the bake's height. */
export const SHRIVEL_FRONT_WIDTH = 0.06;

export interface Shrivel {
  /** The icon as the canvas last drew it, at its item's tile now. */
  icon: MossyIcon;
  /** The time of its first shrivel frame, or null until that frame. */
  startMs: number | null;
}

export type Shrivels = ReadonlyMap<ItemKey, Shrivel>;

/**
 * The shrivels after a scene update. One starts for each item that is mossy in
 * what the canvas last drew and whose app the next growth frame lists as clean,
 * while the item is still in the Dock. One in progress follows its item's tile,
 * and ends when the item has moss again or leaves the Dock. None starts after
 * a load or a restore ("unknown"), or while the dev override pins every item
 * mossy in `next`.
 */
export function shrivelsAfter(
  shrivels: Shrivels,
  drawn: Drawn,
  items: readonly DockItem[],
  growth: readonly AppGrowth[],
  next: readonly MossyIcon[],
): Shrivels {
  const inDock = new Map(itemKeys(items).map((key, index) => [key, items[index]]));
  const mossy = new Set(next.map(({ key }) => key));
  const cleanApps = new Set(
    growth.filter(({ overgrowth }) => quantize(overgrowth) === 0).map(({ bundleId }) => bundleId),
  );
  const after = new Map<ItemKey, Shrivel>();
  for (const [key, shrivel] of shrivels) {
    const item = inDock.get(key);
    if (item !== undefined && !mossy.has(key)) {
      after.set(key, { ...shrivel, icon: { ...shrivel.icon, tile: item.rect } });
    }
  }
  for (const icon of drawn === "unknown" ? [] : drawn.icons) {
    const item = inDock.get(icon.key);
    if (item !== undefined && cleanApps.has(item.bundleId) && !mossy.has(icon.key) && !after.has(icon.key)) {
      after.set(icon.key, { icon: { ...icon, tile: item.rect }, startMs: null });
    }
  }
  return after;
}

/** How far a shrivel is, from 0 on its first frame to 1 at SHRIVEL_MS. */
export function shrivelProgress(startMs: number, timeMs: number): number {
  return Math.min(1, Math.max(0, (timeMs - startMs) / SHRIVEL_MS));
}

export function smoothstep(edge0: number, edge1: number, x: number): number {
  const t = Math.min(1, Math.max(0, (x - edge0) / (edge1 - edge0)));
  return t * t * (3 - 2 * t);
}

/**
 * A shrivelling icon at `progress`, from its bake as last drawn: the moss
 * browns by SHRIVEL_WITHER_END while its front eases down to the bottom edge.
 * Null once nothing is left, at progress 1.
 */
export function shrivelInstance(
  icon: MossyIcon,
  progress: number,
  pixelRatio: number,
  canvasHeight: number,
  layer: number,
): MossInstance | null {
  const remaining = 1 - smoothstep(0, 1, progress);
  if (remaining <= 0) {
    return null;
  }
  const [instance] = mossInstances([icon], pixelRatio, canvasHeight, () => layer);
  return { ...instance, wither: smoothstep(0, SHRIVEL_WITHER_END, progress), remaining };
}

/**
 * One frame of every shrivel: each one's instance at `timeMs`, from the bake
 * of the icon as last drawn, and the shrivels still running after it. A
 * shrivel's clock starts on its first frame; one with nothing left to draw,
 * or no bake, has ended.
 */
export function shrivelFrame(
  shrivels: Shrivels,
  timeMs: number,
  pixelRatio: number,
  canvasHeight: number,
  layerOf: LayerOf,
): { instances: MossInstance[]; running: Shrivels } {
  const instances: MossInstance[] = [];
  const running = new Map<ItemKey, Shrivel>();
  for (const [key, { icon, startMs }] of shrivels) {
    const start = startMs ?? timeMs;
    const layer = layerOf(icon);
    const instance =
      layer === undefined
        ? null
        : shrivelInstance(icon, shrivelProgress(start, timeMs), pixelRatio, canvasHeight, layer);
    if (instance !== null) {
      instances.push(instance);
      running.set(key, { icon, startMs: start });
    }
  }
  return { instances, running };
}

export interface InstanceAttribute {
  name: string;
  size: number;
  values(instance: MossInstance): readonly number[];
}

/** One instance's attributes in buffer order, each an `in` of moss.vert.glsl. */
export const INSTANCE_ATTRIBUTES: readonly InstanceAttribute[] = [
  { name: "a_box", size: 4, values: ({ box }) => [box.x, box.y, box.width, box.height] },
  { name: "a_layer", size: 1, values: ({ layer }) => [layer] },
  { name: "a_wither", size: 1, values: ({ wither }) => [wither] },
  { name: "a_remaining", size: 1, values: ({ remaining }) => [remaining] },
];

/** The shrivel loop draws at most 30 frames a second. */
export const LOOP_MIN_FRAME_INTERVAL_MS = 33;

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
