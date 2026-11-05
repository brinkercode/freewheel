import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// outDir must match build.frontendDist in src-tauri/tauri.conf.json.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    // Cargo output and Rust sources churn during builds; watching them crashes dev mode on Windows.
    watch: { ignored: ["**/target/**", "**/src-tauri/**", "**/crates/**"] },
  },
  build: { outDir: "dist" },
});
