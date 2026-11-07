// @vitest-environment jsdom
// The settings window's whole update flow: Check for updates through the
// updater plugin (every state visible), and in --simulate the simulated
// update source through the same screens, What's new after the restart.
import { cleanup, configure, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import App from "./App.jsx";
import { sampleConfig, sampleStatus } from "./dev/mockTauri.js";
import { SIM_UPDATE_EVENT } from "./simulateEvents.js";

configure({ asyncUtilTimeout: 10000 });

function memoryStorage(items = {}) {
  const map = new Map(Object.entries(items));
  return { getItem: (k) => (map.has(k) ? map.get(k) : null), setItem: (k, v) => map.set(k, String(v)), removeItem: (k) => map.delete(k) };
}

let listeners;
function fakeTauri({ simulate = false, check = async () => null, autoUpdate = true } = {}) {
  listeners = {};
  const config = sampleConfig();
  config.settings.auto_update = autoUpdate;
  const invoke = vi.fn(async (cmd, args) => {
    switch (cmd) {
      case "app_version":
        return "0.1.0";
      case "get_config":
        return structuredClone(config);
      case "save_config":
        Object.assign(config, args.config);
        return null;
      case "get_status":
        return sampleStatus();
      case "plugin:updater|check":
        return check();
      default:
        return null;
    }
  });
  const listen = async (event, handler) => {
    (listeners[event] ??= []).push(handler);
    return () => {};
  };
  window.__TAURI__ = { core: { invoke }, event: { listen } };
  window.__TAURI_INTERNALS__ = { invoke, transformCallback: () => 1, metadata: {} };
  if (simulate) window.__FREEWHEEL_SIMULATE__ = true;
  return invoke;
}

afterEach(() => {
  cleanup();
  delete window.__TAURI__;
  delete window.__TAURI_INTERNALS__;
  delete window.__FREEWHEEL_SIMULATE__;
});

const sub = () => document.querySelector(".version-card [data-update]");
const cardButton = () => document.querySelector(".version-card .update-button");
const openGeneral = async () => {
  await waitFor(() => expect(screen.getByRole("button", { name: "General" })).toBeTruthy());
  fireEvent.click(screen.getByRole("button", { name: "General" }));
  await waitFor(() => expect(screen.getByText("freewheel 0.1.0")).toBeTruthy());
};

test("Check for updates shows Checking… with a spinner, then You're up to date (and the toast)", async () => {
  fakeTauri();
  render(<App storage={memoryStorage({ "freewheel.first-run-done": "1" })} />);
  await openGeneral();
  fireEvent.click(cardButton());
  expect(sub().dataset.update).toBe("checking");
  expect(cardButton().querySelector(".spinner")).not.toBeNull();
  expect(cardButton().disabled).toBe(true);
  await waitFor(() => expect(sub().dataset.update).toBe("current"));
  expect(sub().textContent).toBe("You’re up to date.");
  expect(document.querySelector(".toast")?.textContent ?? document.body.textContent).toContain("You’re up to date.");
});

test("a build without the updater shows why instead of doing nothing", async () => {
  fakeTauri({ check: async () => Promise.reject("plugin updater not found") });
  vi.spyOn(console, "error").mockImplementation(() => {});
  render(<App storage={memoryStorage({ "freewheel.first-run-done": "1" })} />);
  await openGeneral();
  fireEvent.click(cardButton());
  await waitFor(() => expect(sub().dataset.update).toBe("error"));
  expect(sub().textContent).toContain("This test build can’t update itself.");
  expect(cardButton().textContent).toBe("Try again");
});

test("outside --simulate the Simulator's New version does nothing", async () => {
  fakeTauri();
  render(<App storage={memoryStorage({ "freewheel.first-run-done": "1" })} />);
  await openGeneral();
  await waitFor(() => expect(listeners[SIM_UPDATE_EVENT]).toBeTruthy());
  listeners[SIM_UPDATE_EVENT].forEach((f) => f({ payload: { status: "available", version: "99.0.0" } }));
  await new Promise((r) => setTimeout(r, 1200));
  expect(document.querySelector(".update-banner")).toBeNull();
  expect(sub().dataset.update).toBe("idle");
});

test("--simulate: New version, Download, progress, ready, Restart, then the new version and What's new", async () => {
  const invoke = fakeTauri({ simulate: true, autoUpdate: false });
  render(<App storage={memoryStorage({ "freewheel.first-run-done": "1" })} />);
  await openGeneral();
  // Nothing published yet: up to date.
  fireEvent.click(cardButton());
  await waitFor(() => expect(sub().dataset.update).toBe("current"));
  // The Simulator's New version: freewheel finds 0.2.0 by itself.
  listeners[SIM_UPDATE_EVENT].forEach((f) => f({ payload: { status: "available", version: "99.0.0" } }));
  await waitFor(() => expect(sub().dataset.update).toBe("available"));
  expect(sub().textContent).toBe("freewheel 0.2.0 is available.");
  expect(document.querySelector('.update-banner[data-update="available"]')).not.toBeNull();
  fireEvent.click(cardButton());
  await waitFor(() => expect(Number(document.querySelector(".version-card [role=progressbar]")?.getAttribute("aria-valuenow"))).toBeGreaterThan(0));
  expect(document.querySelector('.update-banner[data-update="downloading"] [role=progressbar]')).not.toBeNull();
  await waitFor(() => expect(sub().dataset.update).toBe("ready"), { timeout: 8000 });
  fireEvent.click(screen.getAllByRole("button", { name: "Restart to update" })[0]);
  expect(sub().dataset.update).toBe("restarting");
  await waitFor(() => expect(screen.getByText("freewheel is updated to 0.2.0")).toBeTruthy(), { timeout: 8000 });
  expect(screen.getByText("freewheel 0.2.0")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "What’s new in v0.2.0" }));
  expect(screen.getByText("smoother scrolling when you turn the wheel fast")).toBeTruthy();
  // Never the real updater.
  expect(invoke.mock.calls.filter(([cmd]) => String(cmd).startsWith("plugin:updater"))).toEqual([]);
}, 30000);
