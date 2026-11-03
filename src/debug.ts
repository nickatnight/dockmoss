import { type DockItem, isCommandError, watchDock } from "./ipc";

/**
 * Outlines every Dock app item. Outlines are keyed by Dock position, since
 * two items can share a bundle ID. Each message requests at most one
 * animation frame, and nothing runs between messages.
 */
export async function outlineDockItems(root: HTMLElement): Promise<void> {
  const layer = document.createElement("div");
  layer.className = "debug-layer";
  layer.setAttribute("aria-hidden", "true");
  root.append(layer);

  const outlines: HTMLDivElement[] = [];
  let latest: DockItem[] = [];
  let frameRequested = false;

  const draw = (): void => {
    frameRequested = false;
    while (outlines.length > latest.length) {
      outlines.pop()?.remove();
    }
    while (outlines.length < latest.length) {
      const outline = document.createElement("div");
      outline.className = "debug-dock-item";
      layer.append(outline);
      outlines.push(outline);
    }
    outlines.forEach((outline, index) => {
      const item = latest[index];
      if (item === undefined) {
        return;
      }
      const { x, y, width, height } = item.rect;
      outline.style.transform = `translate(${x}px, ${y}px)`;
      outline.style.width = `${width}px`;
      outline.style.height = `${height}px`;
    });
  };

  try {
    await watchDock((items) => {
      latest = items;
      if (!frameRequested) {
        frameRequested = true;
        requestAnimationFrame(draw);
      }
    });
  } catch (error) {
    const kind = isCommandError(error) ? error.kind : "unknown";
    console.warn("dock items not watched", kind, error);
  }
}
