import { isCommandError, watchDock, watchGrowth } from "./ipc";
import { startMossRenderer } from "./render";

// One subscription per channel: a second watch call would replace the first.
async function start(root: HTMLElement): Promise<void> {
  const dev = import.meta.env.DEV ? await import("./debug") : null;
  const sinks = [startMossRenderer(root, dev?.overgrowthOverride() ?? null)];
  if (dev !== null) {
    sinks.push(dev.outlineDockItems(root));
  }
  const [dock, growth] = await Promise.allSettled([
    watchDock((items) => {
      for (const sink of sinks) sink.dock(items);
    }),
    watchGrowth((frame) => {
      for (const sink of sinks) sink.growth(frame);
    }),
  ]);
  const notWatched = (what: string, error: unknown): void => {
    const kind = isCommandError(error) ? error.kind : "unknown";
    console.warn(`${what} not watched`, kind, error);
    dev?.markNotWatched(root);
  };
  if (dock.status === "rejected") {
    notWatched("dock items", dock.reason);
  }
  if (growth.status === "rejected") {
    notWatched("growth", growth.reason);
  }
}

void start(document.body);
