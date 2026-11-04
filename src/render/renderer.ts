// The renderer's state and draw rule, without the DOM: index.ts wires the
// canvas, the window and requestAnimationFrame to it, and renderer.test.ts
// drives it with a fake MossGl and a manual scheduler.

import type { AppGrowth, DockItem } from "../ipc";
import type { MossGl } from "./gl";
import { type Scene, artBox, mostOvergrownItem, step, toDeviceBox } from "./model";

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
  let latest: Scene | null = null;
  let drawn: Scene | null = null;
  let frameRequested = false;
  let size = { width: 0, height: 0, pixelRatio: 1 };

  const frame = (): void => {
    frameRequested = false;
    const scene = latest;
    if (gl === null || scene === null) {
      return;
    }
    const next = step(drawn, scene);
    if (next.kind !== "draw") {
      return;
    }
    if (next.changes.geometry) {
      const canvasHeight = Math.round(size.height * size.pixelRatio);
      gl.setBox(toDeviceBox(artBox(scene.tile), size.pixelRatio, canvasHeight));
    }
    if (next.changes.moss) {
      gl.setMoss(scene.seed, scene.overgrowth);
    }
    gl.draw();
    drawn = scene;
  };

  const update = (): void => {
    latest = mostOvergrownItem(items, growth, options.overgrowthOverride);
    const next = step(drawn, latest);
    if (next.kind === "clear") {
      // At once: the window may be ordered out already, and WebKit runs no
      // frame callbacks there, so the old moss would show at the next order-in.
      gl?.clear();
      drawn = null;
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
