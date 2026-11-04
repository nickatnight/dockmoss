import { outlineDockItems } from "./debug";
import { isCommandError, watchDock, watchGrowth } from "./ipc";
import { startMossRenderer } from "./render";

// One subscription per channel: a second watch call would replace the first.
async function start(root: HTMLElement): Promise<void> {
  const sinks = [startMossRenderer(root, null), outlineDockItems(root)];
  const [dock, growth] = await Promise.allSettled([
    watchDock((items) => {
      for (const sink of sinks) sink.dock(items);
    }),
    watchGrowth((frame) => {
      for (const sink of sinks) sink.growth(frame);
    }),
  ]);
  if (dock.status === "rejected") {
    warnNotWatched("dock items", dock.reason);
  }
  if (growth.status === "rejected") {
    warnNotWatched("growth", growth.reason);
  }
}

function warnNotWatched(what: string, error: unknown): void {
  const kind = isCommandError(error) ? error.kind : "unknown";
  console.warn(`${what} not watched`, kind, error);
}

void start(document.body);
