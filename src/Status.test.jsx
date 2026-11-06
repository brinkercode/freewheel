// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { renderToString } from "react-dom/server";
import { expect, test } from "vitest";
import Status, { overrideErrorLines } from "./Status.jsx";

const status = {
  connection: "Connected",
  battery: { level: 64, charging: true },
  layer: "edit",
  override_errors: [{ mode: "Brightness", error: "DELL U2720Q: no DDC reply" }],
  permissions: { Accessibility: "Granted" },
};

test("the status shows connection, battery, page and override errors", () => {
  const html = renderToString(<Status initial={status} />);
  for (const text of [
    "Speed Editor: Connected",
    "battery 64% (charging)",
    "Page: edit",
    "Brightness: DELL U2720Q: no DDC reply",
  ]) {
    expect(html).toContain(text);
  }
});

test("each display's brightness error shows on its own line", () => {
  const two = { mode: "Brightness", error: "DELL U2720Q: no DDC reply; Built-in Display: unavailable" };
  const html = renderToString(<Status initial={{ ...status, override_errors: [two] }} />);
  expect(html).toContain('<p role="alert">Brightness: DELL U2720Q: no DDC reply</p>');
  expect(html).toContain('<p role="alert">Brightness: Built-in Display: unavailable</p>');
});

test("per-app volume reads 'permission needed' or 'not available on macOS yet'", () => {
  expect(overrideErrorLines({ mode: "AppVolume", error: "permission needed" })).toEqual(["App volume: permission needed"]);
  expect(overrideErrorLines({ mode: "AppVolume", error: "not available on this Mac yet" })).toEqual([
    "App volume: not available on macOS yet",
  ]);
  expect(overrideErrorLines({ mode: "SystemVolume", error: "no device" })).toEqual(["System volume: no device"]);
});

test("connection and battery update live from the status event", async () => {
  const listeners = {};
  window.__TAURI__ = {
    core: { invoke: async () => ({ ...status, connection: "Absent", battery: null, override_errors: [] }) },
    event: { listen: async (name, handler) => ((listeners[name] = handler), () => {}) },
  };
  try {
    render(<Status />);
    expect(await screen.findByText("Speed Editor: Not connected")).toBeTruthy();
    await waitFor(() => expect(listeners.status).toBeTypeOf("function"));
    act(() => listeners.status({ payload: { ...status, battery: { level: 40, charging: false }, override_errors: [] } }));
    expect(screen.getByText("Speed Editor: Connected · battery 40%")).toBeTruthy();
  } finally {
    cleanup();
    delete window.__TAURI__;
  }
});

test("a lost connection reads Not connected without a battery", () => {
  const html = renderToString(
    <Status initial={{ ...status, connection: "Absent", battery: null, override_errors: [] }} />,
  );
  expect(html).toContain("Speed Editor: Not connected");
  expect(html).not.toContain("battery");
});
