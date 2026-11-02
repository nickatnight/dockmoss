import { defineConfig } from "vite";

export default defineConfig({
  clearScreen: false,
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
  },
});
