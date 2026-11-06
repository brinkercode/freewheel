import { readdirSync, readFileSync } from "node:fs";
import { renderToString } from "react-dom/server";
import { expect, test } from "vitest";
import viteConfig from "../vite.config.js";
import App from "./App.jsx";
import Simulator from "./Simulator.jsx";

test("App renders the settings heading", () => {
  const done = { getItem: () => "1" };
  expect(renderToString(<App storage={done} />)).toContain("freewheel settings");
});

test("the UI uses the system font and loads no network fonts or icons", () => {
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  const sources = readdirSync(new URL(".", import.meta.url)).filter(
    (f) => /\.(jsx?|tsx?|css)$/.test(f) && !/\.test\./.test(f),
  );
  expect(sources).toContain("Status.jsx");
  expect(sources).toEqual(expect.arrayContaining(["FirstRun.jsx", "Permissions.jsx", "KeysTab.jsx"]));
  const ui = [html, ...sources.map((f) => readFileSync(new URL(`./${f}`, import.meta.url), "utf8"))].join("\n");
  expect(html).toMatch(/font-family:[^;]*system-ui/);
  expect(ui).not.toMatch(/fonts\.googleapis|fonts\.gstatic|Material\sSymbols|https?:\/\//);
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
        resolve_open: false,
        permissions: "Granted",
        update: { status: "ready", version: "99.0.0" },
        first_run: true,
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
    "Open Resolve",
    "Permissions missing",
    "Next update state",
    "99.0.0",
    "is ready.",
    "Restart to update",
    "Reset to first run",
    "· first run",
  ]) {
    expect(html).toContain(text);
  }
});

test("vite outDir is the frontendDist of the Tauri config", () => {
  const tauri = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url)));
  expect(tauri.build.frontendDist).toBe(`../${viteConfig.build.outDir}`);
});
