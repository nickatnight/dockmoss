// The renderer's state and draw rule, without the DOM: index.ts wires the
// canvas, the window and requestAnimationFrame to it, and renderer.test.ts
// drives it with a fake MossGl and a manual scheduler.

import type { AppGrowth, DockItem } from "../ipc";
import type { MossGl } from "./gl";
import { type Icon, mossInstances, mossyIcons, step } from "./model";

/** What `overlay.ts` passes every Dock and growth frame to. */
export interface FrameSink {
  dock(items: DockItem[]): void;
  growth(frame: AppGrowth[]): void;
}

export interface RendererOptions {
  /** Runs `callback` once, before the next repaint, with the time in ms. */
  schedule(callback: (time: number) => void): void;
  /** A MossGl for the canvas's current context, or null if it has none. */
  createGl(): MossGl | null;
  overgrowthOverride: number | null;
}

export interface MossRenderer extends FrameSink {
  /** The canvas's size in CSS px and its pixel ratio, which may be unchanged. */
  resize(width: number, height: number, pixelRatio: number): void;
  contextLost(): void;
  contextRestored(): void;
}

export function createRenderer(options: RendererOptions): MossRenderer {
  let gl = options.createGl();
  let items: DockItem[] = [];
  let growth: AppGrowth[] = [];
  let latest: Icon[] = [];
  /** What the canvas shows, or null when that is unknown. */
  let drawn: Icon[] | null = null;
  let frameRequested = false;
  let size = { width: 0, height: 0, pixelRatio: 1 };

  const frame = (): void => {
    frameRequested = false;
    if (gl === null || step(drawn, latest).kind !== "draw") {
      return;
    }
    const canvasHeight = Math.round(size.height * size.pixelRatio);
    gl.draw(mossInstances(latest, size.pixelRatio, canvasHeight));
    drawn = latest;
  };

  const update = (): void => {
    latest = mossyIcons(items, growth, options.overgrowthOverride);
    const next = step(drawn, latest);
    if (next.kind === "clear") {
      // At once: the window may be ordered out already, and WebKit runs no
      // frame callbacks there, so the old moss would show at the next order-in.
      gl?.clear();
      drawn = [];
    } else if (next.kind === "draw" && gl !== null && !frameRequested) {
      frameRequested = true;
      options.schedule(frame);
    }
  };

  const resize = (width: number, height: number, pixelRatio: number): void => {
    size = { width, height, pixelRatio };
    gl?.resize(Math.round(width * pixelRatio), Math.round(height * pixelRatio));
    drawn = null;
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
      drawn = null;
    },
    contextRestored() {
      gl = options.createGl();
      resize(size.width, size.height, size.pixelRatio);
    },
  };
}
