import { dockItems, isCommandError } from "./ipc";

/** Outlines every Dock app icon with a 1 px rect, until the moss renderer
 * replaces this in slice 9. Draws nothing when the Dock cannot be read. */
export async function outlineDockItems(root: HTMLElement): Promise<void> {
  try {
    const items = await dockItems();
    root.append(
      ...items.map(({ rect }) => {
        const outline = document.createElement("div");
        outline.className = "debug-dock-item";
        outline.style.left = `${rect.x}px`;
        outline.style.top = `${rect.y}px`;
        outline.style.width = `${rect.width}px`;
        outline.style.height = `${rect.height}px`;
        return outline;
      }),
    );
  } catch (error) {
    const kind = isCommandError(error) ? error.kind : "unknown";
    console.warn("dock items not drawn", kind, error);
  }
}
