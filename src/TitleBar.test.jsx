// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { Toolbar } from "./SettingsWindow.jsx";
import Simulator from "./Simulator.jsx";
import { TitleBar, WindowControls, isMac } from "./TitleBar.jsx";

afterEach(cleanup);

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");

function fakeWindow() {
  let max = false;
  let resized = null;
  return {
    calls: [],
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {
      max = !max;
      resized?.();
    }),
    close: vi.fn(async () => {}),
    isMaximized: vi.fn(async () => max),
    onResized: vi.fn(async (fn) => {
      resized = fn;
      return () => (resized = null);
    }),
  };
}

const ok = { enabled: true, permissionMissing: false, connected: true, bus: "Usb", battery: null, paused: false };

test("macOS is told apart from Windows and Linux", () => {
  expect(isMac({ platform: "MacIntel" })).toBe(true);
  expect(isMac({ platform: "Win32" })).toBe(false);
  expect(isMac({ platform: "", userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64)" })).toBe(false);
});

test("the toolbar is the window's header: draggable, with minimise, maximise and close at its right end", () => {
  const { container } = render(<Toolbar tab="Keys" onTab={() => {}} state={ok} />);
  const nav = container.querySelector("nav.toolbar");
  expect(nav.getAttribute("data-tauri-drag-region")).toBe("deep");
  expect(nav.lastElementChild.className).toBe("window-controls");
  expect([...nav.querySelectorAll(".window-control")].map((b) => b.getAttribute("aria-label"))).toEqual(["Minimise", "Maximise", "Close"]);
  // no title text, no app icon
  expect(nav.textContent).not.toContain("freewheel");
});

test("the controls minimise, maximise then restore, and close the window", async () => {
  const win = fakeWindow();
  render(<WindowControls win={win} mac={false} />);
  await act(async () => {});
  fireEvent.click(screen.getByLabelText("Minimise"));
  expect(win.minimize).toHaveBeenCalledOnce();
  await act(async () => fireEvent.click(screen.getByLabelText("Maximise")));
  expect(win.toggleMaximize).toHaveBeenCalledOnce();
  expect(screen.getByLabelText("Restore").getAttribute("data-state")).toBe("maximized");
  await act(async () => fireEvent.click(screen.getByLabelText("Restore")));
  expect(screen.getByLabelText("Maximise")).toBeTruthy();
  fireEvent.click(screen.getByLabelText("Close"));
  expect(win.close).toHaveBeenCalledOnce();
});

test("on macOS the native traffic lights stay: the app draws no controls", () => {
  const { container } = render(<WindowControls win={fakeWindow()} mac />);
  expect(container.innerHTML).toBe("");
});

test("screens without the toolbar keep a draggable header with the controls", () => {
  const { container } = render(<TitleBar />);
  expect(container.querySelector("header.title-bar").getAttribute("data-tauri-drag-region")).toBe("deep");
  expect(container.querySelectorAll(".window-control")).toHaveLength(3);
});

test("the Simulator window's header drags it and has its own controls", () => {
  const { container } = render(<Simulator />);
  const head = container.querySelector(".fw-sim-head");
  expect(head.getAttribute("data-tauri-drag-region")).toBe("deep");
  expect(head.querySelectorAll(".window-control")).toHaveLength(3);
});

test("both windows may minimise, maximise, close and drag themselves", () => {
  for (const file of ["settings.json", "simulator.json"]) {
    const { permissions } = JSON.parse(read(`../src-tauri/capabilities/${file}`));
    for (const p of ["minimize", "toggle-maximize", "internal-toggle-maximize", "is-maximized", "start-dragging", "close"])
      expect(permissions, file).toContain(`core:window:allow-${p}`);
  }
});

test("the controls use the prototype's Windows caption look: 46px buttons, red close", () => {
  const css = read("./styles/base.css");
  expect(css).toMatch(/\.window-control \{ width: 46px; height: 32px;/);
  expect(css).toContain(".window-control.window-close:hover:not(:disabled) { background: #c42b1c; color: #fff; }");
});
