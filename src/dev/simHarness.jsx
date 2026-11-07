// Dev only: mounts the Simulator window on the fake backend (simFake.js).
// ?solo=1 hides the settings window iframe.
import { createRoot } from "react-dom/client";
import "../styles/tokens.css";
import "../styles/base.css";
import Simulator from "../Simulator.jsx";
import { createSimFake } from "./simFake.js";

const fake = createSimFake();
// As simulate.rs does: "Record a key" and "New version" reach the settings window (the iframe).
const settings = () => document.getElementById("settings")?.contentWindow;
const emit = (event, payload) => settings()?.__fwEmit?.(event, payload);
const invoke = async (cmd, args) => {
  const out = await fake.invoke(cmd, args);
  if (cmd === "sim_jump" && args.to === "RecordKey") emit("sim-record-key", "Cut");
  if (cmd === "sim_jump" && args.to === "Reload") settings()?.location.reload();
  if (cmd === "simulate" && args.control.type === "StepUpdate") emit("sim-update", out.update);
  return out;
};
window.__TAURI__ = { core: { invoke } };
window.__simFake = fake;
if (new URLSearchParams(location.search).get("solo") === "1") document.body.classList.add("solo");

createRoot(document.getElementById("root")).render(<Simulator />);
