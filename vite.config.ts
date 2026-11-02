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
});
