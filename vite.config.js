import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// outDir must match build.frontendDist in src-tauri/tauri.conf.json.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { outDir: "dist" },
});
