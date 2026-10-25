import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Vite configuration tuned for Tauri development.
// See https://vitejs.dev/config/ and https://tauri.app/start/frontend/vite/
export default defineConfig({
  plugins: [react()],
  // Tauri expects a fixed port; fail if it's taken.
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2021",
    minify: "esbuild",
    sourcemap: false,
  },
});
