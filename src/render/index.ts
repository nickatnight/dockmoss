import type { AppGrowth, DockItem } from "../ipc";
import { type MossGl, createMossGl } from "./gl";
import { type Scene, drawnIcon, step, toDeviceBox } from "./model";

/**
 * Draws moss on one Dock icon. Feed it every Dock and growth frame; it
 * draws one frame per visible change and runs nothing while idle.
 * `override` (dev builds only) sets the drawn icon's overgrowth.
 */
export function startMossRenderer(root: HTMLElement, override: number | null) {
  const canvas = document.createElement("canvas");
  canvas.className = "moss";
  canvas.setAttribute("aria-hidden", "true");
  root.append(canvas);

  let moss: MossGl | null = createMossGl(canvas);
  let items: DockItem[] = [];
  let growth: AppGrowth[] = [];
  let latest: Scene | null = null;
  let drawn: Scene | null = null;
  let frameRequested = false;

  const frame = (): void => {
    frameRequested = false;
    const scene = latest;
    if (moss === null || scene === null) {
      return;
    }
    const next = step(drawn, scene);
    if (next.kind !== "draw") {
      return;
    }
    if (next.changes.geometry) {
      moss.setBox(toDeviceBox(scene.rect, devicePixelRatio, canvas.height));
    }
    if (next.changes.moss) {
      moss.setMoss(scene.seed, scene.overgrowth);
    }
    moss.draw();
    drawn = scene;
  };

  const update = (): void => {
    latest = drawnIcon(items, growth, override);
    const next = step(drawn, latest);
    if (next.kind === "clear") {
      // At once: the window may be ordered out already, and WebKit runs no
      // frame callbacks there, so the old moss would show at the next order-in.
      moss?.clear();
      drawn = null;
    } else if (next.kind === "draw" && moss !== null && !frameRequested) {
      frameRequested = true;
      requestAnimationFrame(frame);
    }
  };

  const resize = (): void => {
    moss?.resize(Math.round(innerWidth * devicePixelRatio), Math.round(innerHeight * devicePixelRatio));
    drawn = null;
    update();
  };

  const watchPixelRatio = (): void => {
    matchMedia(`(resolution: ${devicePixelRatio}dppx)`).addEventListener(
      "change",
      () => {
        resize();
        watchPixelRatio();
      },
      { once: true },
    );
  };

  addEventListener("resize", resize);
  watchPixelRatio();
  canvas.addEventListener("webglcontextlost", (event) => {
    event.preventDefault();
    moss = null;
    drawn = null;
  });
  canvas.addEventListener("webglcontextrestored", () => {
    moss = createMossGl(canvas);
    resize();
  });
  resize();

  return {
    dock(next: DockItem[]): void {
      items = next;
      update();
    },
    growth(next: AppGrowth[]): void {
      growth = next;
      update();
    },
  };
}
