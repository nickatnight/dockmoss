import {
  type AppGrowth,
  type BundleId,
  type DockItem,
  isCommandError,
  watchDock,
  watchGrowth,
} from "./ipc";

interface Outline {
  box: HTMLDivElement;
  label: HTMLSpanElement;
}

export async function outlineDockItems(root: HTMLElement): Promise<void> {
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
      label.textContent = app
        ? `${app.overgrowth.toFixed(2)} ${app.stage}`
        : "";
    });
  };

  const requestDraw = (): void => {
    if (!frameRequested) {
      frameRequested = true;
      requestAnimationFrame(draw);
    }
  };

  const [dock, growthWatch] = await Promise.allSettled([
    watchDock((items) => {
      latest = items;
      requestDraw();
    }),
    watchGrowth((frame) => {
      growth = new Map(frame.map((app) => [app.bundleId, app]));
      requestDraw();
    }),
  ]);
  if (dock.status === "rejected") {
    warnNotWatched("dock items", dock.reason);
  }
  if (growthWatch.status === "rejected") {
    warnNotWatched("growth", growthWatch.reason);
  }
}

function warnNotWatched(what: string, error: unknown): void {
  const kind = isCommandError(error) ? error.kind : "unknown";
  console.warn(`${what} not watched`, kind, error);
}
