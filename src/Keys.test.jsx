// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import App from "./App.jsx";
import { KEYS } from "./Keys.jsx";

afterEach(() => {
  cleanup();
  delete window.__TAURI__;
});

const defaults = { version: 1, layers: [{ id: "base", name: "Base", bindings: {} }], base_layer: "base" };

function memoryStorage() {
  const items = {};
  return { getItem: (k) => items[k] ?? null, setItem: (k, v) => (items[k] = String(v)) };
}

function fakeTauri(status) {
  const calls = [];
  const listeners = {};
  const answers = { get_status: status, get_config: defaults, get_permissions: status.permissions };
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => (calls.push(cmd), answers[cmd]) },
    event: {
      listen: async (name, handler) => {
        listeners[name] = handler;
        return () => listeners[name] === handler && delete listeners[name];
      },
    },
  };
  return { calls, listeners };
}

const connected = { connection: "Connected", bus: "Usb", battery: null, layer: "base", override_errors: [], permissions: {}, paused: false };

test("KEYS names every Speed Editor key by its printed label, as status.rs does", () => {
  const rust = readFileSync("crates/freewheel-core/src/status.rs", "utf8");
  const printed = [...rust.matchAll(/Key::(\w+) => "([^"]+)"/g)].map((m) => [m[1], m[2]]);
  expect(KEYS).toEqual(printed);
});

test("first launch shows Welcome in place of the content; once finished the Keys tab shows every key Not set, and the flow does not show again", async () => {
  const { calls } = fakeTauri(connected);
  const storage = memoryStorage();
  render(<App autoUpdate={false} storage={storage} />);
  expect(screen.getByText("Welcome to freewheel")).toBeTruthy();
  expect(screen.queryByText("freewheel settings")).toBeNull();
  fireEvent.click(screen.getByText("Get started"));
  await screen.findByText("Found it!");
  fireEvent.click(screen.getByText("Continue"));
  await waitFor(() => expect(screen.getAllByText("Not set")).toHaveLength(KEYS.length));
  expect(screen.getByRole("button", { name: "Keys", pressed: true })).toBeTruthy();
  // The first run leaves the default config (no bindings, the wheel scrolls) as it is.
  expect(calls).not.toContain("save_config");
  cleanup();
  render(<App autoUpdate={false} storage={storage} />);
  expect(screen.queryByText("Welcome to freewheel")).toBeNull();
  expect(screen.getByText("freewheel settings")).toBeTruthy();
});

test("the permission fix from the notice bar or the tray status line reuses the permissions screen with Done", async () => {
  const missing = { ...connected, permissions: { Accessibility: "Missing", InputMonitoring: "Granted" } };
  const { listeners } = fakeTauri(missing);
  const done = memoryStorage();
  done.setItem("freewheel.first-run-done", "1");
  render(<App autoUpdate={false} storage={done} />);
  fireEvent.click(await screen.findByText("Fix this"));
  expect(screen.getByText("Allow freewheel on your Mac")).toBeTruthy();
  expect(screen.getByText("Done").disabled).toBe(true);
  expect(await screen.findByText("Open System Settings")).toBeTruthy();
  expect(screen.queryByText("Start using freewheel")).toBeNull();
  cleanup();
  render(<App autoUpdate={false} storage={done} />);
  await waitFor(() => expect(listeners["fix-permissions"]).toBeTypeOf("function"));
  act(() => listeners["fix-permissions"]());
  expect(await screen.findByText("Done")).toBeTruthy();
});
