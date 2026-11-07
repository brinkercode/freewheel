import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// outDir must match build.frontendDist in src-tauri/tauri.conf.json.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    // IPv4 only, matching build.devUrl: on Windows "localhost" can resolve to ::1 while another (stale) dev
    // server holds 127.0.0.1:5173, and the app window then silently loads that old frontend.
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
    // Cargo output and Rust sources churn during builds; watching them crashes dev mode on Windows.
    watch: { ignored: ["**/target/**", "**/src-tauri/**", "**/crates/**"] },
  },
  build: { outDir: "dist" },
});
