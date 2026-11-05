import { getCurrentWindow } from "@tauri-apps/api/window";

// An Accessory app has no menu bar to carry ⌘W, so the page handles it.
document.addEventListener("keydown", (event) => {
  const commandOnly =
    event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey;
  if (commandOnly && event.key.toLowerCase() === "w") {
    event.preventDefault();
    getCurrentWindow()
      .close()
      .catch((error: unknown) => {
        console.error("settings window did not close", error);
      });
  }
});
