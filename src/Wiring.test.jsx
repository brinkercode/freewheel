// @vitest-environment jsdom
// The controls wired up by the audit: Show file, Back to normal, the media
// keys, the Simulator's tray menu and its log of what the app did.
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { KeyPanel } from "./ActionEditor.jsx";
import Backup from "./Backup.jsx";
import KeysTab from "./KeysTab.jsx";
import Simulator, { TRAY_ITEMS } from "./Simulator.jsx";
import WheelTab, { overrideNotice } from "./WheelTab.jsx";
import { createSimFake } from "./dev/simFake.js";
import { comboError, comboFromEvent, longLabel, shortLabel } from "./keys.js";
import { SIM_LOG_EVENT } from "./simulateEvents.js";

afterEach(() => {
  cleanup();
  delete window.__TAURI__;
});

const scroll = { left: { Scroll: { amount: -1 } }, right: { Scroll: { amount: 1 } }, threshold: 1, shuttle_max_rate_hz: null };
const config = (bindings = {}) => ({
  version: 1,
  settings: { enabled: true, start_at_login: false, resolve_process_name: "Resolve", low_battery_threshold: 15, step_aside_for_resolve: true },
  layers: [{ id: "base", name: "Main", bindings }],
  base_layer: "base",
  jog: { default_mode: "Scroll", per_mode: { Scroll: scroll } },
  overrides: { system_volume_step: 0.02, app_volume_step: 0.02, brightness_step: 5 },
});

test("Download logs' confirmation has Show file, which shows the saved file", async () => {
  const showFile = vi.fn(() => Promise.resolve());
  render(<Backup initialLogs="Downloaded “freewheel-logs.zip” to your Downloads folder." showFile={showFile} />);
  fireEvent.click(screen.getByText("Show file"));
  expect(showFile).toHaveBeenCalledTimes(1);
  cleanup();
  render(<Backup initialLogs="Downloaded." showFile={() => Promise.reject("The log file is gone. Download the logs again.")} />);
  fireEvent.click(screen.getByText("Show file"));
  await waitFor(() => expect(screen.getByRole("alert").textContent).toBe("The log file is gone. Download the logs again."));
});

test("the Wheel tab says which key turned the wheel into a knob; Back to normal leaves it", async () => {
  const c = config({ FullView: { OverrideToggle: { mode: "SystemVolume" } } });
  expect(overrideNotice(c, { wheel_override: null, layer: "base" })).toBeNull();
  expect(overrideNotice(c, { wheel_override: "SystemVolume", layer: "base" })).toBe(
    "Right now FULL VIEW has turned the wheel into a volume knob. Press FULL VIEW again to go back.",
  );
  expect(overrideNotice(c, { wheel_override: "Brightness", layer: "base" })).toContain("a brightness knob");
  const exitOverride = vi.fn(() => Promise.resolve());
  render(<WheelTab initial={c} status={{ wheel_override: "SystemVolume", layer: "base" }} exitOverride={exitOverride} />);
  fireEvent.click(screen.getByText("Back to normal"));
  expect(exitOverride).toHaveBeenCalledTimes(1);
  cleanup();
  render(<WheelTab initial={c} status={{ wheel_override: null, layer: "base" }} />);
  expect(screen.queryByText("Back to normal")).toBeNull();
});

test("the media keys record, save and read as the prototype names them", () => {
  expect(comboFromEvent({ key: "MediaPlayPause" })).toEqual({ modifiers: [], key: "PlayPause" });
  expect(comboFromEvent({ key: "AudioVolumeMute" })).toEqual({ modifiers: [], key: "Mute" });
  expect(comboFromEvent({ key: "MediaTrackNext" }).key).toBe("NextTrack");
  expect(comboError({ modifiers: [], key: "PlayPause" })).toBeNull();
  expect(comboError({ modifiers: [], key: "Mute" })).toBeNull();
  const c = config();
  expect(shortLabel({ Hotkey: { combo: { modifiers: [], key: "PlayPause" } } }, c)).toBe("Play/Pause");
  expect(longLabel({ Hotkey: { combo: { modifiers: [], key: "Mute" } } }, c)).toBe("Mute");
  expect(longLabel({ Hotkey: { combo: { modifiers: [], key: "PrevTrack" } } }, c)).toBe("Previous track");
  // A Start Menu shortcut or a .app is named for what it opens.
  expect(longLabel({ LaunchApp: { path: "explorer.exe", args: ["C:/Start Menu/Spotify.lnk"], enabled: true } }, c)).toBe("Opens Spotify");
  expect(longLabel({ LaunchApp: { path: "/usr/bin/open", args: ["/Applications/Notes.app"], enabled: true } }, c)).toBe("Opens Notes");
});

test("the Simulator's tray menu clicks each tray item and shows the checks", async () => {
  const fake = createSimFake();
  const listeners = {};
  window.__TAURI__ = {
    core: { invoke: fake.invoke },
    event: { listen: async (name, f) => ((listeners[name] = f), () => {}) },
  };
  render(<Simulator />);
  await waitFor(() => expect(screen.getByText(/^Page base/)).toBeTruthy());
  const menu = within(screen.getByRole("group", { name: "Tray menu" }));
  expect(menu.getAllByRole("button").map((b) => b.textContent)).toEqual(TRAY_ITEMS.map(([, text]) => text({ on: true })));
  await act(async () => fireEvent.click(menu.getByText("Start at login")));
  await waitFor(() => expect(menu.getByText("Start at login").getAttribute("aria-pressed")).toBe("true"));
  await act(async () => fireEvent.click(menu.getByText("freewheel is On")));
  await waitFor(() => expect(menu.getByText("freewheel is Off")).toBeTruthy());
  for (const text of ["Open settings…", "Download logs for support…", "Quit freewheel"]) await act(async () => fireEvent.click(menu.getByText(text)));
  expect(fake.trayClicks).toEqual(["start-at-login", "toggle", "open-settings", "download-logs", "quit"]);
  // The permission line shows only while a permission is missing.
  expect(menu.queryByText(/Needs permission/)).toBeNull();
  await act(async () => fireEvent.click(within(screen.getByRole("group", { name: "macOS permission" })).getByText("Missing")));
  await waitFor(() => expect(menu.getByText("Needs permission (click to fix)")).toBeTruthy());
  // What the app did shows in the log as it happens.
  await act(async () => listeners[SIM_LOG_EVENT]({ payload: ["Saved the logs to C:/Downloads/freewheel-logs.zip"] }));
  expect(screen.getByRole("list", { name: "What the Speed Editor did" }).textContent).toContain("Saved the logs to");
});

test("a Quick pick saves while the record box listens (it keeps focus, so nothing moves under the click)", async () => {
  const onSave = vi.fn(() => Promise.resolve({ config: config() }));
  render(<KeyPanel keyName="In" config={config()} layerId="base" initialMode="Held" initialListen onSave={onSave} />);
  const shift = screen.getByText("Shift").closest("button");
  // mousedown is cancelled: the record box is not blurred before the click.
  expect(fireEvent.mouseDown(shift)).toBe(false);
  fireEvent.click(shift);
  expect(onSave).toHaveBeenCalledWith("Held", { modifiers: ["Shift"], key: "" });
});

test("a key restored without 'Let restored keys open apps' reads as off, and says how to turn it on", () => {
  const off = { LaunchApp: { path: "C:/Their/Tool.exe", args: [], enabled: false } };
  const c = config({ Cam1: off });
  expect(shortLabel(off, c)).toBe("Opens Tool (off)");
  expect(longLabel(off, c)).toBe("Opens Tool (off: restored from a file)");
  render(<KeyPanel keyName="Cam1" action={off} config={c} layerId="base" initialMode="LaunchApp" />);
  expect(screen.getByRole("note").textContent).toContain("so it is off and opens nothing");
});

test("a page key on the Speed Editor switches the Keys tab to that page", () => {
  const c = config({ Cam5: { LayerPush: { target: "studio" } } });
  c.layers.push({ id: "studio", name: "Edit Bay", bindings: {} });
  const active = () => document.querySelector(".pill.is-active button[role=tab]").textContent;
  const { rerender } = render(<KeysTab initial={c} status={{ connection: "Connected", layer: "base" }} />);
  expect(active()).toBe("Base".replace("Base", c.layers[0].name));
  rerender(<KeysTab initial={c} status={{ connection: "Connected", layer: "studio" }} />);
  expect(active()).toBe("Edit Bay");
  rerender(<KeysTab initial={c} status={{ connection: "Connected", layer: "base" }} />);
  expect(active()).toBe("Main");
});
