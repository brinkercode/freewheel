// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, expect, test } from "vitest";
import Simulator, { QUICK_KEYS, deviceOption, trayState } from "./Simulator.jsx";
import { createSimFake } from "./dev/simFake.js";
import { LABELS } from "./keys.js";

// jsdom gives import.meta.url an http scheme; vitest runs from the repo root.
const SRC = join(process.cwd(), "src");

let fake;
beforeEach(() => {
  fake = createSimFake();
  window.__TAURI__ = { core: { invoke: fake.invoke } };
});
afterEach(() => {
  cleanup();
  delete window.__TAURI__;
});

const mount = async () => {
  render(<Simulator />);
  await waitFor(() => expect(screen.getByText(/^Page base/)).toBeTruthy());
};
const group = (name) => within(screen.getByRole("group", { name }));
const pressed = (name) =>
  group(name)
    .getAllByRole("button")
    .filter((b) => b.getAttribute("aria-pressed") === "true")
    .map((b) => b.textContent);
const click = (el) => act(async () => fireEvent.click(el));
const log = () => screen.getByRole("list", { name: "What the Speed Editor did" }).textContent;

test("it has every group of the prototype's controls column", async () => {
  await mount();
  for (const text of [
    "Simulator controls",
    "Pretend to be the computer and the Speed Editor.",
    "Press a key on the Speed Editor",
    "↺ Turn wheel left",
    "Turn right ↻",
    "What the Speed Editor did shows up here.",
    "Jump to",
    "Tray icon states",
    "Send keys for real",
  ]) {
    expect(screen.getByText(text)).toBeTruthy();
  }
  for (const key of QUICK_KEYS) expect(screen.getAllByText(LABELS[key]).length).toBeGreaterThan(0);
  expect(pressed("Speed Editor")).toEqual(["Not found"]);
  expect(pressed("DaVinci Resolve")).toEqual(["Closed"]);
  expect(pressed("freewheel")).toEqual(["On"]);
  expect(pressed("Keys")).toEqual(["Empty setup"]);
  expect(pressed("macOS permission")).toEqual(["Allowed"]);
  expect(screen.getByRole("listitem", { current: true }).textContent).toBe("Not connected");
});

test("the Speed Editor row connects over USB or Bluetooth with its battery", async () => {
  await mount();
  await click(group("Speed Editor").getByText("Bluetooth 12%"));
  expect(pressed("Speed Editor")).toEqual(["Bluetooth 12%"]);
  expect(screen.getByRole("listitem", { current: true }).textContent).toBe("Needs attention");
  await click(group("Speed Editor").getByText("USB"));
  expect(pressed("Speed Editor")).toEqual(["USB"]);
  expect(screen.getByRole("listitem", { current: true }).textContent).toBe("On");
  await click(group("Speed Editor").getByText("Not found"));
  expect(pressed("Speed Editor")).toEqual(["Not found"]);
});

test("keys and the wheel go to the device and the log says what it did", async () => {
  await mount();
  await click(group("Speed Editor").getByText("USB"));
  await click(group("Keys").getByText("Several set (2 pages)"));
  expect(pressed("Keys")).toEqual(["Several set (2 pages)"]);
  const cut = screen.getByRole("button", { name: "CUT" });
  await act(async () => fireEvent.pointerDown(cut));
  expect(cut.getAttribute("aria-pressed")).toBe("true");
  await act(async () => fireEvent.pointerUp(cut));
  expect(cut.getAttribute("aria-pressed")).toBe("false");
  expect(log()).toContain("You pressed CUT");
  expect(log()).toContain("would press Ctrl+C");

  fireEvent.change(screen.getByRole("combobox", { name: "Key to press" }), { target: { value: "Esc" } });
  await click(screen.getByRole("button", { name: "Press" }));
  await waitFor(() => expect(log()).toContain("would press Ctrl+Z"));

  const right = screen.getByRole("button", { name: "Turn right ↻" });
  await act(async () => fireEvent.pointerDown(right));
  await act(async () => fireEvent.pointerUp(right));
  expect(log()).toContain("would scroll 1");
});

test("Resolve, the switch and the permission drive the app's state", async () => {
  await mount();
  await click(group("Speed Editor").getByText("USB"));
  await click(group("DaVinci Resolve").getByText("Open"));
  expect(pressed("DaVinci Resolve")).toEqual(["Open"]);
  expect(pressed("freewheel")).toEqual(["Off"]);
  expect(screen.getByRole("listitem", { current: true }).textContent).toBe("Off / paused");
  // While Off the device stays as picked.
  await click(group("Speed Editor").getByText("Bluetooth 64%"));
  expect(pressed("Speed Editor")).toEqual(["Bluetooth 64%"]);
  await click(group("DaVinci Resolve").getByText("Closed"));
  expect(pressed("freewheel")).toEqual(["On"]);
  await click(group("macOS permission").getByText("Missing"));
  expect(screen.getByRole("listitem", { current: true }).textContent).toBe("Needs attention");
});

test("Jump to opens the settings window's screens", async () => {
  await mount();
  for (const name of ["First run", "Settings window", "Record a key", "Reload settings", "Quit app"]) {
    await click(screen.getByRole("button", { name }));
  }
  await click(group("Computer appearance").getByText("Dark"));
  expect(fake.jumps).toEqual(["FirstRun", "Settings", "RecordKey", "Reload", "Quit", "Dark"]);
  expect(pressed("Computer appearance")).toEqual(["Dark"]);
  await click(screen.getByRole("button", { name: "New version" }));
  expect(screen.getByText("New version: freewheel 99.0.0 is available.")).toBeTruthy();
});

test("send keys for real is a switch", async () => {
  await mount();
  const real = screen.getByRole("switch");
  expect(real.checked).toBe(false);
  await click(real);
  expect(real.checked).toBe(true);
  expect(screen.getByText("Shortcuts reach the apps you have open.")).toBeTruthy();
});

test("option and tray state helpers", () => {
  expect(deviceOption(null)).toBe("none");
  expect(deviceOption({ connection: "Off" })).toBe(null);
  expect(deviceOption({ connection: "Connected", bus: "Bluetooth", battery: 64 })).toBe("bt");
  expect(trayState({ enabled: false, connection: "Off" })).toBe("off");
  expect(trayState({ enabled: true, connection: "Connected", low_battery: false, permissions: "Granted" })).toBe("on");
});

test("the simulator's styles live in their own stylesheet, scoped to its window", () => {
  const css = readFileSync(join(SRC, "styles/simulator.css"), "utf8");
  const selectors = css
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/@supports[^{]*\{/g, "")
    .match(/[^{}]+(?=\{)/g)
    .flatMap((s) => s.split(","))
    .map((s) => s.trim())
    .filter(Boolean);
  for (const s of selectors) expect(s).toMatch(/fw-sim|fw-tray-icon/);
  expect(readFileSync(join(SRC, "Simulator.jsx"), "utf8")).toContain('import "./styles/simulator.css"');
});
