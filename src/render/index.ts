import { bake } from "./bake";
import { createMossGl } from "./gl";
import { mossScene } from "./generator";
import { type FrameSink, createRenderer } from "./renderer";

export interface MossOptions {
  overgrowthOverride: number | null;
  sway: boolean;
}

export function startMossRenderer(root: HTMLElement, { overgrowthOverride, sway }: MossOptions): FrameSink {
  const canvas = document.createElement("canvas");
  canvas.className = "moss";
  canvas.setAttribute("aria-hidden", "true");
  root.append(canvas);

  const renderer = createRenderer({
    schedule: (callback) => requestAnimationFrame(callback),
    createGl: () => createMossGl(canvas),
    bake: (request) => {
      const baked = bake(mossScene(request), request.side);
      if (baked.kind !== "baked") {
        console.error(`moss renderer: no bake (${baked.kind})`);
        return null;
      }
      return baked.canvas;
    },
    overgrowthOverride,
    sway,
  });
  const resize = (): void => renderer.resize(innerWidth, innerHeight, devicePixelRatio);

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
    renderer.contextLost();
  });
  canvas.addEventListener("webglcontextrestored", () => renderer.contextRestored());
  resize();

  return renderer;
}
