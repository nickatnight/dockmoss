import "./debug.css";
import type { AppGrowth, BundleId, DockItem } from "./ipc";
import { parseOvergrowthOverride } from "./render/model";

interface Outline {
  box: HTMLDivElement;
  label: HTMLSpanElement;
}

export function outlineDockItems(root: HTMLElement) {
  const layer = document.createElement("div");
  layer.className = "debug-layer";
  layer.setAttribute("aria-hidden", "true");
  root.append(layer);

  const outlines: Outline[] = [];
  let latest: DockItem[] = [];
  let growth = new Map<BundleId, AppGrowth>();
  let frameRequested = false;

  const addOutline = (): Outline => {
    const box = document.createElement("div");
    box.className = "debug-dock-item";
    const label = document.createElement("span");
    label.className = "debug-dock-item-label";
    box.append(label);
    layer.append(box);
    const outline = { box, label };
    outlines.push(outline);
    return outline;
  };

  const draw = (): void => {
    frameRequested = false;
    while (outlines.length > latest.length) {
      outlines.pop()?.box.remove();
    }
    latest.forEach(({ bundleId, rect }, index) => {
      const { box, label } = outlines[index] ?? addOutline();
      box.style.transform = `translate(${rect.x}px, ${rect.y}px)`;
      box.style.width = `${rect.width}px`;
      box.style.height = `${rect.height}px`;
      const app = growth.get(bundleId);
      const text = app ? `${app.overgrowth.toFixed(2)}\n${app.stage}` : "";
      if (label.textContent !== text) {
        label.textContent = text;
      }
    });
  };

  const requestDraw = (): void => {
    if (!frameRequested) {
      frameRequested = true;
      requestAnimationFrame(draw);
    }
  };

  return {
    dock(items: DockItem[]): void {
      latest = items;
      requestDraw();
    },
    growth(frame: AppGrowth[]): void {
      growth = new Map(frame.map((app) => [app.bundleId, app]));
      requestDraw();
    },
  };
}

export function markNotWatched(root: HTMLElement): void {
  if (root.querySelector(".debug-not-watched") !== null) {
    return;
  }
  const bar = document.createElement("div");
  bar.className = "debug-not-watched";
  bar.setAttribute("aria-hidden", "true");
  root.append(bar);
}

export function overgrowthOverride(): number | null {
  const override = parseOvergrowthOverride(import.meta.env.VITE_DOCKMOSS_OVERGROWTH);
  if (override.kind === "invalid") {
    console.warn("VITE_DOCKMOSS_OVERGROWTH ignored: not a number from 0 to 1", override.raw);
  }
  return override.kind === "set" ? override.overgrowth : null;
}
