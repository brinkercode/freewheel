import { readFileSync } from "node:fs";
import { renderToString } from "react-dom/server";
import { expect, test } from "vitest";
import viteConfig from "../vite.config.js";
import App from "./App.jsx";
import Simulator from "./Simulator.jsx";

test("App renders the settings heading", () => {
  expect(renderToString(<App />)).toContain("freewheel settings");
});

test("Simulator window renders the status and every control", () => {
  const html = renderToString(
    <Simulator
      initial={{
        connection: "Connected",
        bus: "Bluetooth",
        battery: 12,
        charging: false,
        low_battery: true,
        page: "base",
        jog_mode: "Scroll",
        lit_leds: ["Scrl"],
        held_keys: [],
        enabled: true,
        send_for_real: false,
        toasts: [],
        keys: ["Cut", "Cam1"],
      }}
    />,
  );
  for (const text of [
    "freewheel Simulator",
    "Connected over Bluetooth",
    "battery 12%",
    "low battery",
    "page base",
    "Connect USB",
    "Disconnect",
    "Turn Off",
    "Load sample mappings",
    "Wheel left",
    "Shuttle",
    "Cam1",
    "Send for real",
  ]) {
    expect(html).toContain(text);
  }
});

test("vite outDir is the frontendDist of the Tauri config", () => {
  const tauri = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url)));
  expect(tauri.build.frontendDist).toBe(`../${viteConfig.build.outDir}`);
});
