import { defineConfig } from "vite";

// https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  clearScreen: false,
  server: {
    port: 5110,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
