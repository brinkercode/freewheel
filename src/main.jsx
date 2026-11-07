import { createRoot } from "react-dom/client";
import "./styles/tokens.css";
import "./styles/base.css";
import App from "./App.jsx";

const mount = () => createRoot(document.getElementById("root")).render(<App />);

// `npm run dev` in a plain browser: fake the Tauri bridge (src/dev/mockTauri.js).
// MODE is "production" in every `vite build` (DEV follows NODE_ENV, which a
// shell may set), so the mock is never bundled (src/BuildOutput.test.js).
if (import.meta.env.MODE === "development" && !window.__TAURI_INTERNALS__) import("./dev/mockTauri.js").then((m) => (m.installMockTauri(), mount()));
else mount();
