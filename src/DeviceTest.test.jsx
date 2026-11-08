// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import Backup from "./Backup.jsx";
import DeviceTest, { busText, copyText, diagnosticsText, secondsText, statusTone } from "./DeviceTest.jsx";

afterEach(() => {
  cleanup();
  delete globalThis.window.__TAURI__;
});

/** A DeviceDiagnostics (src/bindings.ts) as the device loop sends it. */
const diag = (over = {}) => ({
  simulated: false,
  status: "Connected",
  device: {
    bus: "Bluetooth",
    vendor_id: 0x1edb,
    product_id: 0xda0e,
    usage_page: 0xff00,
    usage: 1,
    interface: -1,
    product: "DaVinci Resolve Speed Editor",
    release: 0x0100,
    candidates: 2,
    report_descriptor: "06 00 ff",
  },
  auth: { state: "Passed", passed: 3, failed: 1, session_s: 600, next_in_s: 4, last_error: null, last_passed_ms: 1200 },
  battery: { level: 87, charging: true },
  held: ["CUT"],
  presses: ["CUT", "IN"],
  wheel: { device_mode: 0, last_value: -3, total: 12, reports: 9 },
  counts: { "0x03": 9, "0x04": 4, "0x07": 1 },
  undecoded: 0,
  open_attempts: 2,
  connects: 1,
  disconnects: 0,
  last_open_error: null,
  reports: [
    { seq: 15, at_ms: 5000, direction: "In", id: 3, hex: "03 00 fd ff ff ff 00", decoded: "wheel mode 0 (relative) value -3", failed: false },
    { seq: 14, at_ms: 4000, direction: "Out", id: 3, hex: "03 00 00 00 00 00 ff", decoded: "set wheel mode 0 (relative)", failed: true },
  ],
  uptime_ms: 6000,
  ...over,
});

test("the view shows connection, auth, battery, keys, wheel and the raw reports", () => {
  render(<DeviceTest initial={diag()} />);
  for (const text of [
    "Device test",
    "Connected",
    "Bluetooth",
    "0x1edb:0xda0e · usage page 0xff00 · interface -1 · 2 listed",
    "Passed",
    "3 passed · 1 failed",
    "10 min 0 s",
    "4 s",
    "87% (charging)",
    "CUT, IN",
    "0 (relative)",
    "-3",
    "03 00 fd ff ff ff 00",
    "wheel mode 0 (relative) value -3",
    "set wheel mode 0 (relative) — write failed",
  ]) {
    expect(screen.getAllByText(text, { exact: true }).length, text).toBeGreaterThan(0);
  }
  expect(screen.queryByText(/Simulated Speed Editor/)).toBeNull();
  expect(document.querySelector(".diag-status").dataset.tone).toBe("green");
  expect(document.querySelector("[class*=dot]")).toBeNull();
  // Read-only: no switches, inputs or selects.
  expect(document.querySelectorAll("input, select, textarea, [role=switch]").length).toBe(0);
});

test("simulated data says so, and an empty device says what to do", () => {
  render(
    <DeviceTest
      initial={diag({ simulated: true, status: "Absent", device: null, battery: null, held: [], presses: [], reports: [], wheel: { device_mode: null, last_value: null, total: 0, reports: 0 }, last_open_error: "HID I/O error: no Speed Editor" })}
    />,
  );
  for (const text of [
    "Simulated Speed Editor: these reports come from the Simulator window.",
    "Not connected",
    "None opened yet",
    "No battery report yet",
    "Press a key on the Speed Editor",
    "Turn the wheel",
    "No reports yet.",
    "HID I/O error: no Speed Editor",
  ]) {
    expect(screen.getByText(text), text).toBeTruthy();
  }
});

test("it updates live from the device-diagnostics event", async () => {
  globalThis.window.__TAURI__ = {};
  let push;
  const stop = vi.fn();
  const get = vi.fn(async () => diag({ held: [] }));
  const listen = vi.fn(async (f) => ((push = f), stop));
  const { unmount } = render(<DeviceTest get={get} listen={listen} />);
  await act(async () => {});
  expect(get).toHaveBeenCalledOnce();
  expect(screen.getByText("None")).toBeTruthy();
  await act(async () => push(diag({ held: ["STOP/PLAY"], status: "AuthFailed" })));
  expect(screen.getByText("STOP/PLAY")).toBeTruthy();
  expect(screen.getByText("Found, but it did not unlock (auth failed)")).toBeTruthy();
  expect(document.querySelector(".diag-status").dataset.tone).toBe("orange");
  unmount();
  await act(async () => {});
  expect(stop).toHaveBeenCalledOnce();
});

test("Copy diagnostics copies the whole text and confirms", async () => {
  const copy = vi.fn(async () => {});
  render(<DeviceTest initial={diag()} version="0.4.0" copy={copy} />);
  await act(async () => fireEvent.click(screen.getByText("Copy diagnostics")));
  const text = copy.mock.calls[0][0];
  for (const needle of [
    "freewheel 0.4.0 device test",
    "status: Connected",
    "device: bus Bluetooth, vid 0x1edb pid 0xda0e, usage page 0xff00 usage 0x0001, interface -1, release 0x0100, product DaVinci Resolve Speed Editor, 2 interface(s) listed",
    "report descriptor: 06 00 ff",
    "auth: Passed, passed 3, failed 1, session 600 s, next in 4 s, last error none",
    "battery: 87% charging",
    "held: CUT",
    "wheel: mode 0, last -3, total 12, reports 9",
    "reports by id: 0x03=9 0x04=4 0x07=1; undecoded 0",
    "  #15 +5000ms in  03 00 fd ff ff ff 00  (wheel mode 0 (relative) value -3)",
    "  #14 +4000ms out 03 00 00 00 00 00 ff  (set wheel mode 0 (relative)) FAILED",
  ]) {
    expect(text).toContain(needle);
  }
  expect(screen.getByText("Diagnostics copied. Paste them into your reply.")).toBeTruthy();
});

test("a failed copy says so", async () => {
  render(<DeviceTest initial={diag()} copy={async () => Promise.reject(new Error("denied"))} />);
  await act(async () => fireEvent.click(screen.getByText("Copy diagnostics")));
  expect(screen.getByText("Could not copy: denied")).toBeTruthy();
});

test("copyText uses the clipboard, else a selected textarea", async () => {
  const writeText = vi.fn(async () => {});
  await copyText("a", document, { clipboard: { writeText } });
  expect(writeText).toHaveBeenCalledWith("a");
  document.execCommand = vi.fn(() => true);
  await copyText("b", document, {});
  expect(document.execCommand).toHaveBeenCalledWith("copy");
  expect(document.querySelector("textarea")).toBeNull();
  document.execCommand = vi.fn(() => false);
  await expect(copyText("c", document, {})).rejects.toThrow("the clipboard is not available");
});

test("helpers", () => {
  expect([busText("Usb"), busText("Bluetooth"), busText(null)]).toEqual(["USB", "Bluetooth", "—"]);
  expect([statusTone("Connected"), statusTone("AuthFailed"), statusTone("Absent")]).toEqual(["green", "orange", "gray"]);
  expect([secondsText(null), secondsText(42), secondsText(590)]).toEqual(["—", "42 s", "9 min 50 s"]);
  expect(diagnosticsText(null, { version: "1" })).toBe("freewheel 1 device test: no data yet");
  expect(diagnosticsText(diag({ simulated: true }))).toContain("SIMULATED device (--simulate)");
});

test("Help & backup > More opens the Device test view and Back returns", () => {
  render(<Backup initialMore />);
  fireEvent.click(screen.getByText("Open device test"));
  expect(document.querySelector('[data-screen-label="Device test"]')).toBeTruthy();
  fireEvent.click(screen.getByText("Help & backup"));
  expect(document.querySelector('[data-screen-label="Help and backup"]')).toBeTruthy();
});

test("on Linux a refused open offers the device rule, and says what installing it did", async () => {
  const install = vi.fn((name, say) => say("Installed freewheel's device rule."));
  render(
    <DeviceTest
      initial={diag({ status: "Absent", last_open_error: "HID I/O error: permission denied: freewheel's device rule is not installed (Permissions: Install the device rule)" })}
      get={() => new Promise(() => {})}
      listen={() => Promise.resolve(() => {})}
      install={install}
    />,
  );
  expect(screen.getByText(/needs freewheel’s device rule/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Install the device rule" }));
  expect(install.mock.calls[0][0]).toBe("DeviceAccess");
  expect(screen.getByText("Installed freewheel's device rule.")).toBeTruthy();
});

test("no device rule hint for other open errors", () => {
  render(<DeviceTest initial={diag({ last_open_error: "no Speed Editor (1edb:da0e) listed by the OS" })} get={() => new Promise(() => {})} listen={() => Promise.resolve(() => {})} />);
  expect(screen.queryByRole("button", { name: "Install the device rule" })).toBeNull();
});
