import type { AppGrowth, DockItem } from "../ipc";
import { EMPTY_ATLAS, type BakeKey, type BakeRequest, bakeRequest, bakeSide, planAtlas } from "./atlas";
import type { MossGl } from "./gl";
import {
  type LayerOf,
  type Drawn,
  LOOP_MIN_FRAME_INTERVAL_MS,
  type MossyIcon,
  NOTHING_DRAWN,
  SWAY_CYCLE_SECONDS,
  type Shrivels,
  mossInstances,
  mossyIcons,
  shrivelFrame,
  shrivelsAfter,
  step,
  swaying,
} from "./model";

export interface FrameSink {
  dock(items: DockItem[]): void;
  growth(frame: AppGrowth[]): void;
}

export interface RendererOptions<Image> {
  /** Runs `callback` once, before the next repaint, with the time in ms. */
  schedule(callback: (time: number) => void): void;
  /** A MossGl for the canvas's current context, or null if it has none. */
  createGl(): MossGl<Image> | null;
  /** The moss for one bake, or null if it could not be drawn. */
  bake(request: BakeRequest): Image | null;
  overgrowthOverride: number | null;
  sway: boolean;
}

export interface MossRenderer extends FrameSink {
  /** The canvas's size in CSS px and its pixel ratio, which may be unchanged. */
  resize(width: number, height: number, pixelRatio: number): void;
  contextLost(): void;
  contextRestored(): void;
}

export function createRenderer<Image>(options: RendererOptions<Image>): MossRenderer {
  let gl = options.createGl();
  let atlas = EMPTY_ATLAS;
  /** The bakes the atlas holds, kept to upload again when the texture is made again. */
  const images = new Map<BakeKey, Image>();
  let items: DockItem[] = [];
  let growth: AppGrowth[] = [];
  let latest: MossyIcon[] = [];
  let drawn: Drawn = "unknown";
  let shrivels: Shrivels = new Map();
  let frameRequested = false;
  let size = { width: 0, height: 0, pixelRatio: 1 };
  let lastLoopFrameMs: number | null = null;
  let swaySeconds = 0;

  const swayRuns = (): boolean => options.sway && swaying(latest);
  const loopRuns = (): boolean => swayRuns() || shrivels.size > 0;

  const requestFrame = (): void => {
    if (gl !== null && !frameRequested) {
      frameRequested = true;
      options.schedule(frame);
    }
  };

  /**
   * Makes the atlas hold the bakes of `icons`: bakes what is new, uploads it,
   * and evicts the rest. Returns where each icon's bake is.
   */
  function prepareAtlas(target: MossGl<Image>, icons: readonly MossyIcon[]): LayerOf {
    const { pixelRatio } = size;
    const plan = planAtlas(
      atlas,
      icons.map((icon) => bakeRequest(icon, pixelRatio)),
      bakeSide(pixelRatio),
    );
    atlas = plan.atlas;
    for (const key of images.keys()) {
      if (!atlas.layers.has(key)) {
        images.delete(key);
      }
    }
    const baked = plan.bakes.flatMap(({ request, layer }) => {
      const image = options.bake(request);
      if (image === null) {
        return [];
      }
      images.set(request.key, image);
      return [{ layer, image }];
    });
    if (plan.reallocate) {
      target.allocate(atlas.capacity, atlas.side);
      target.upload(
        [...atlas.layers].flatMap(([key, layer]) => {
          const image = images.get(key);
          return image === undefined ? [] : [{ layer, image }];
        }),
      );
    } else if (baked.length > 0) {
      target.upload(baked);
    }
    return (icon) => {
      const { key } = bakeRequest(icon, pixelRatio);
      return images.has(key) ? atlas.layers.get(key) : undefined;
    };
  }

  function forgetBakes(): void {
    atlas = EMPTY_ATLAS;
    images.clear();
  }

  function frame(time: number): void {
    frameRequested = false;
    if (gl === null) {
      return;
    }
    const due =
      loopRuns() && (lastLoopFrameMs === null || time - lastLoopFrameMs >= LOOP_MIN_FRAME_INTERVAL_MS);
    const changed = step(drawn, latest, shrivels).kind !== "none";
    if (due) {
      lastLoopFrameMs = time;
      if (swayRuns()) {
        swaySeconds = (time / 1000) % SWAY_CYCLE_SECONDS;
      }
    }
    if (due || changed) {
      const canvasHeight = Math.round(size.height * size.pixelRatio);
      const layerOf = prepareAtlas(gl, [...latest, ...[...shrivels.values()].map(({ icon }) => icon)]);
      const shrivelling = shrivelFrame(shrivels, time, size.pixelRatio, canvasHeight, layerOf);
      shrivels = shrivelling.running;
      gl.draw(
        [...mossInstances(latest, size.pixelRatio, canvasHeight, layerOf), ...shrivelling.instances],
        swaySeconds,
      );
      drawn = { icons: latest, showsShrivel: shrivelling.instances.length > 0 };
    }
    if (loopRuns()) {
      requestFrame();
    } else {
      lastLoopFrameMs = null;
    }
  }

  const update = (): void => {
    latest = mossyIcons(items, growth, options.overgrowthOverride);
    shrivels = shrivelsAfter(shrivels, drawn, items, growth, latest);
    const next = step(drawn, latest, shrivels);
    if (next.kind === "clear") {
      // At once: the window may be ordered out already, and WebKit runs no
      // frame callbacks there, so the old moss would show at the next order-in.
      gl?.clear();
      drawn = NOTHING_DRAWN;
    } else if (next.kind === "draw" || shrivels.size > 0) {
      // A running shrivel always has a frame pending already; the size check states that it needs one.
      requestFrame();
    }
    if (!loopRuns()) {
      lastLoopFrameMs = null;
    }
  };

  const resize = (width: number, height: number, pixelRatio: number): void => {
    size = { width, height, pixelRatio };
    gl?.resize(Math.round(width * pixelRatio), Math.round(height * pixelRatio));
    drawn = "unknown";
    update();
  };

  return {
    dock(next) {
      items = next;
      update();
    },
    growth(next) {
      growth = next;
      update();
    },
    resize,
    contextLost() {
      gl = null;
      forgetBakes();
      drawn = "unknown";
      shrivels = new Map();
      lastLoopFrameMs = null;
    },
    contextRestored() {
      gl = options.createGl();
      resize(size.width, size.height, size.pixelRatio);
    },
  };
}
