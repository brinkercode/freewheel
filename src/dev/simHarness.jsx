// Dev only: mounts the Simulator window on the fake backend (simFake.js).
// ?solo=1 hides the settings window iframe.
import { createRoot } from "react-dom/client";
import "../styles/tokens.css";
import "../styles/base.css";
import Simulator from "../Simulator.jsx";
import { createSimFake } from "./simFake.js";

const fake = createSimFake();
window.__TAURI__ = { core: { invoke: fake.invoke } };
window.__simFake = fake;
if (new URLSearchParams(location.search).get("solo") === "1") document.body.classList.add("solo");

createRoot(document.getElementById("root")).render(<Simulator />);
