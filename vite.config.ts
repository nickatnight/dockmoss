import { defineConfig, type Plugin } from "vite";

function noDevOnlyModuleInBuild(): Plugin {
  return {
    name: "dockmoss:no-dev-only-module-in-build",
    apply: "build",
    generateBundle() {
      for (const id of this.getModuleIds()) {
        if (id.endsWith("/src/debug.ts")) {
          this.error(
            `${id} is in the release build. Load it only through import() under import.meta.env.DEV.`,
          );
        }
        if (id.includes("/src/icon/")) {
          this.error(`${id} is in the release build. src/icon/ writes the bundle icon master under vitest only.`);
        }
      }
    },
  };
}

export default defineConfig({
  clearScreen: false,
  plugins: [noDevOnlyModuleInBuild()],
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
