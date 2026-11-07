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

const reported = (invoke) => invoke.mock.calls.filter(([cmd]) => cmd === "tray_update_state").map(([, a]) => a.state?.status ?? null);

test("the tray's update item drives the flow: the window reports each state and runs check / restart from the tray", async () => {
  const invoke = fakeTauri({ simulate: true, autoUpdate: true });
  render(<App storage={memoryStorage({ "freewheel.first-run-done": "1" })} />);
  await openGeneral();
  await waitFor(() => expect(listeners["tray-update"]).toBeTruthy());
  // Idle is reported, so the tray says Check for updates.
  await waitFor(() => expect(reported(invoke)).toContain(null));
  // Tray: Check for updates (nothing published): checking, then up to date.
  listeners["tray-update"].forEach((f) => f({ payload: "check" }));
  await waitFor(() => expect(sub().dataset.update).toBe("checking"));
  await waitFor(() => expect(sub().dataset.update).toBe("current"));
  // A new version: auto update downloads it; the tray hears ready.
  listeners[SIM_UPDATE_EVENT].forEach((f) => f({ payload: null }));
  await waitFor(() => expect(reported(invoke)).toContain("ready"), { timeout: 10000 });
  expect(reported(invoke)).toEqual(expect.arrayContaining(["checking", "current", "downloading", "ready"]));
  // Tray: Restart to update.
  listeners["tray-update"].forEach((f) => f({ payload: "restart" }));
  await waitFor(() => expect(reported(invoke)).toContain("restarting"));
  await waitFor(() => expect(screen.getByText("freewheel is updated to 0.2.0")).toBeTruthy(), { timeout: 8000 });
}, 30000);

test("a build without the updater reports notConfigured to the tray", async () => {
  const invoke = fakeTauri({ check: async () => Promise.reject("plugin updater not found") });
  vi.spyOn(console, "error").mockImplementation(() => {});
  render(<App storage={memoryStorage({ "freewheel.first-run-done": "1" })} />);
  await openGeneral();
  await waitFor(() => expect(listeners["tray-update"]).toBeTruthy());
  listeners["tray-update"].forEach((f) => f({ payload: "check" }));
  await waitFor(() =>
    expect(invoke.mock.calls.some(([cmd, a]) => cmd === "tray_update_state" && a.state?.notConfigured === true)).toBe(true),
  );
});

test("the tray's Download logs opens Help & backup with the saved file (or why not)", async () => {
  fakeTauri();
  render(<App storage={memoryStorage({ "freewheel.first-run-done": "1" })} />);
  await waitFor(() => expect(listeners["logs-saved"]).toBeTruthy());
  listeners["logs-saved"].forEach((f) => f({ payload: { path: "C:\\Users\\a\\Downloads\\freewheel-logs-1.txt" } }));
  await waitFor(() => expect(screen.getByText("Downloaded “freewheel-logs-1.txt” to your Downloads folder.")).toBeTruthy());
  expect(screen.getByText("Show file")).toBeTruthy();
  listeners["logs-saved"].forEach((f) => f({ payload: { error: "the disk is full" } }));
  await waitFor(() => expect(screen.getByRole("alert").textContent).toBe("The logs could not be saved: the disk is full"));
});
