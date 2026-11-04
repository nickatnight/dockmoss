import { createMossGl } from "./gl";
import { type FrameSink, createRenderer } from "./renderer";

export function startMossRenderer(root: HTMLElement, overgrowthOverride: number | null): FrameSink {
  const canvas = document.createElement("canvas");
  canvas.className = "moss";
  canvas.setAttribute("aria-hidden", "true");
  root.append(canvas);

  const renderer = createRenderer({
    schedule: (callback) => requestAnimationFrame(callback),
    createGl: () => createMossGl(canvas),
    overgrowthOverride,
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
