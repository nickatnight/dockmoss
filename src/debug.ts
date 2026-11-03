import { type DockItem, isCommandError, watchDock } from "./ipc";

export async function outlineDockItems(root: HTMLElement): Promise<void> {
  const layer = document.createElement("div");
  layer.className = "debug-layer";
  layer.setAttribute("aria-hidden", "true");
  root.append(layer);

  const outlines: HTMLDivElement[] = [];
  let latest: DockItem[] = [];
  let frameRequested = false;

  const addOutline = (): HTMLDivElement => {
    const outline = document.createElement("div");
    outline.className = "debug-dock-item";
    layer.append(outline);
    outlines.push(outline);
    return outline;
  };

  const draw = (): void => {
    frameRequested = false;
    while (outlines.length > latest.length) {
      outlines.pop()?.remove();
    }
    latest.forEach(({ rect }, index) => {
      const outline = outlines[index] ?? addOutline();
      outline.style.transform = `translate(${rect.x}px, ${rect.y}px)`;
      outline.style.width = `${rect.width}px`;
      outline.style.height = `${rect.height}px`;
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
