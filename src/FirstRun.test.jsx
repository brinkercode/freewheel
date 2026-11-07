// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import FirstRun from "./FirstRun.jsx";
import { PermissionsScreen, rowState } from "./Permissions.jsx";
import { fromStatusEvent } from "./shell.js";

afterEach(() => {
  cleanup();
  delete window.__TAURI__;
});

const absent = fromStatusEvent({ connection: "Absent" });
const usb = fromStatusEvent({ connection: "Connected", bus: "Usb" });
const bluetooth = fromStatusEvent({ connection: "Connected", bus: "Bluetooth" });
const mac = { Accessibility: "Missing", InputMonitoring: "Missing" };

function fakeTauri(permissions) {
  const calls = [];
  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        calls.push([cmd, args]);
        return cmd === "get_permissions" ? permissions() : undefined;
      },
    },
  };
  return calls;
}

test("Welcome shows first, and Get started moves to Plug in or pair", () => {
  render(<FirstRun state={absent} permissions={{}} onFinish={() => {}} />);
  expect(screen.getByRole("heading").textContent).toBe("Welcome to freewheel");
  fireEvent.click(screen.getByText("Get started"));
  expect(screen.getByRole("heading").textContent).toBe("Plug in or pair your Speed Editor");
});

test("Plug in or pair looks until a device connects, then live Found it! with the connection type", () => {
  const props = { permissions: {}, step: 1, onFinish: () => {} };
  const { rerender } = render(<FirstRun {...props} state={absent} />);
  expect(screen.getByText("Looking for your Speed Editor…")).toBeTruthy();
  expect(screen.queryByText("Found it!")).toBeNull();
  expect(screen.getByText("Continue").disabled).toBe(true);
  rerender(<FirstRun {...props} state={bluetooth} />);
  expect(screen.getByText("Found it!")).toBeTruthy();
  expect(screen.getByText(/^Connected by Bluetooth/)).toBeTruthy();
  expect(screen.queryByText("Looking for your Speed Editor…")).toBeNull();
  expect(screen.getByText("Continue").disabled).toBe(false);
  rerender(<FirstRun {...props} state={usb} />);
  expect(screen.getByText("Connected by USB")).toBeTruthy();
});

test("Open Bluetooth settings calls open_bluetooth_settings", () => {
  const calls = fakeTauri(() => ({}));
  render(<FirstRun state={absent} permissions={{}} step={1} onFinish={() => {}} />);
  fireEvent.click(screen.getByText("Open Bluetooth settings"));
  expect(calls).toEqual([["open_bluetooth_settings", undefined]]);
});

test("Windows (no permissions) skips the permissions screen: Continue finishes", () => {
  const onFinish = vi.fn();
  render(<FirstRun state={usb} permissions={{}} step={1} onFinish={onFinish} />);
  fireEvent.click(screen.getByText("Continue"));
  expect(onFinish).toHaveBeenCalledOnce();
});

test("on macOS Continue opens the permissions screen with Start using freewheel", () => {
  const onFinish = vi.fn();
  render(<FirstRun state={usb} permissions={mac} initialPermissions={mac} step={1} onFinish={onFinish} />);
  fireEvent.click(screen.getByText("Continue"));
  expect(onFinish).not.toHaveBeenCalled();
  expect(screen.getByText("Accessibility")).toBeTruthy();
  expect(screen.getByText("Input Monitoring")).toBeTruthy();
  expect(screen.getByText("Start using freewheel").disabled).toBe(true);
});

test("each row: Open System Settings, then Waiting…, then a live Allowed from polling; the button waits for both", async () => {
  vi.useFakeTimers();
  let current = { ...mac };
  const calls = fakeTauri(() => current);
  const onDone = vi.fn();
  render(<PermissionsScreen poll={1000} button="Start using freewheel" onDone={onDone} />);
  await act(() => vi.advanceTimersByTimeAsync(0));
  expect(screen.getAllByText("Open System Settings")).toHaveLength(2);
  fireEvent.click(screen.getAllByText("Open System Settings")[0]);
  expect(calls).toContainEqual(["open_permission_pane", { permission: "Accessibility" }]);
  expect(screen.getByText("Waiting…")).toBeTruthy();
  current = { ...current, Accessibility: "Granted" };
  await act(() => vi.advanceTimersByTimeAsync(1000));
  expect(screen.getByText("Allowed")).toBeTruthy();
  expect(screen.queryByText("Waiting…")).toBeNull();
  expect(screen.getByText("Start using freewheel").disabled).toBe(true);
  current = { ...current, InputMonitoring: "Granted" };
  await act(() => vi.advanceTimersByTimeAsync(1000));
  expect(screen.getAllByText("Allowed")).toHaveLength(2);
  fireEvent.click(screen.getByText("Start using freewheel"));
  expect(onDone).toHaveBeenCalledOnce();
  vi.useRealTimers();
});

test("a row is idle, waiting once asked, and allowed once granted", () => {
  expect(rowState("Missing", false)).toBe("idle");
  expect(rowState("Missing", true)).toBe("waiting");
  expect(rowState("Granted", true)).toBe("granted");
});

test("the tray status line asks for the permission fix by the event the settings window hears", () => {
  const tray = readFileSync("src-tauri/src/tray.rs", "utf8");
  expect(tray).toContain('const FIX_PERMISSIONS_EVENT: &str = "fix-permissions";');
  expect(tray).toMatch(/STATUS => \{\s*show_settings\(app\);\s*if let Err\(e\) = app\.emit\(FIX_PERMISSIONS_EVENT/);
  const api = readFileSync("src/settingsApi.js", "utf8");
  expect(api).toContain('FIX_PERMISSIONS_EVENT = "fix-permissions"');
});

test("the permissions screen shows the permissions that asked for it, live, without get_permissions (--simulate on Windows)", () => {
  const calls = fakeTauri(() => ({}));
  const onFinish = vi.fn();
  const granted = { Accessibility: "Granted", InputMonitoring: "Missing" };
  const { rerender } = render(<FirstRun state={usb} permissions={granted} step={1} onFinish={onFinish} />);
  fireEvent.click(screen.getByText("Continue"));
  const start = screen.getByText("Start using freewheel");
  expect(screen.getByText("Allow freewheel to press keys for you")).toBeTruthy();
  expect(start.disabled).toBe(true);
  rerender(<FirstRun state={usb} permissions={{ ...granted, InputMonitoring: "Granted" }} step={1} onFinish={onFinish} />);
  expect(screen.getByText("Start using freewheel").disabled).toBe(false);
  fireEvent.click(screen.getByText("Start using freewheel"));
  expect(onFinish).toHaveBeenCalledOnce();
  expect(calls.filter(([cmd]) => cmd === "get_permissions")).toEqual([]);
});
