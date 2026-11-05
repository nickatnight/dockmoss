import { defineConfig, type Plugin } from "vite";

/**
 * Fails the build when `src/debug.ts` is in the module graph at all, so a
 * static import is caught even when nothing of it reaches the bundle's text.
 */
function noDebugModuleInBuild(): Plugin {
  return {
    name: "dockmoss:no-debug-module-in-build",
    apply: "build",
    generateBundle() {
      for (const id of this.getModuleIds()) {
        if (id.endsWith("/src/debug.ts")) {
          this.error(
            `${id} is in the release build. Load it only through import() under import.meta.env.DEV.`,
          );
        }
      }
    },
  };
}

export default defineConfig({
  clearScreen: false,
  plugins: [noDebugModuleInBuild()],
  server: {
    port: 5110,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    // Safari 18 is the WebKit of macOS 15, the bundle's minimumSystemVersion.
    target: "safari18",
    modulePreload: { polyfill: false },
    rolldownOptions: {
      input: { overlay: "overlay.html", settings: "settings.html" },
    },
  },
});
