import { readFileSync } from "node:fs";
import { renderToString } from "react-dom/server";
import { expect, test, vi } from "vitest";
import { NoticeBar, Toast, Toolbar } from "./SettingsWindow.jsx";
import UpdateBanner from "./UpdateBanner.jsx";
import { setEnabled } from "./settingsApi.js";
import { TOAST_MS, fromStatusEvent, showToast, switchOn } from "./shell.js";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");

const ok = { enabled: true, permissionMissing: false, connected: true, bus: "Usb", battery: null, paused: false };

test("the settings window opens at 1000x640, min 820x520, without the OS title bar (macOS: traffic lights over the app's header), and closing hides it to the tray", () => {
  const [win] = JSON.parse(read("../src-tauri/tauri.conf.json")).app.windows;
  expect(win).toMatchObject({ width: 1000, height: 640, minWidth: 820, minHeight: 520, resizable: true, decorations: false, shadow: true });
  const [mac] = JSON.parse(read("../src-tauri/tauri.macos.conf.json")).app.windows;
  const { shadow: _windowsOnly, ...same } = win;
  expect(mac).toMatchObject({ ...same, decorations: true, titleBarStyle: "Overlay", hiddenTitle: true });
  const sim = read("../src-tauri/src/simulate.rs");
  expect(sim).toContain(".title_bar_style(tauri::TitleBarStyle::Overlay)");
  expect(sim).toContain("builder = builder.decorations(false).shadow(true);");
  const main = read("../src-tauri/src/main.rs");
  expect(main).toMatch(/CloseRequested \{ api, \.\. \} = event \{\s*api\.prevent_close\(\);\s*let _ = window\.hide\(\);/);
});

const toolbar = (state) => renderToString(<Toolbar tab="Keys" onTab={() => {}} state={state} onToggle={() => {}} />);

test("the toolbar has the five tabs and the On/Off switch", () => {
  const html = toolbar(ok);
  for (const tab of ["Keys", "Wheel", "DaVinci Resolve", "General", "Help &amp; backup"]) expect(html).toContain(`>${tab}</button>`);
  expect(html).toMatch(/aria-pressed="true"[^>]*>Keys</);
  expect(html).toContain('role="switch" aria-label="Turn freewheel on or off" aria-checked="true"');
  expect(html).toContain("status-dot");
  expect(toolbar({ ...ok, enabled: false })).toContain('aria-checked="false"');
});

test("the status reads the prototype's first match: Off, Needs permission, Looking, Paused, Connected", () => {
  const all = { enabled: false, permissionMissing: true, connected: false, bus: null, battery: null, paused: true };
  expect(toolbar(all)).toContain("Off: Resolve can use it");
  expect(toolbar({ ...all, enabled: true })).toContain("Needs permission (click to fix)");
  expect(toolbar({ ...all, enabled: true, permissionMissing: false })).toContain("Looking for your Speed Editor…");
  expect(toolbar({ ...ok, paused: true })).toContain("Paused: DaVinci Resolve is open");
  expect(toolbar(ok)).toContain("Connected by USB");
  expect(toolbar({ ...ok, bus: "Bluetooth", battery: 64 })).toContain("Connected by Bluetooth · 64%");
  // low battery turns the dot amber
  expect(toolbar({ ...ok, bus: "Bluetooth", battery: 12 })).toContain("dot-orange");
});

test("a StatusEvent maps to the shell state", () => {
  expect(
    fromStatusEvent({ connection: "Connected", battery: { level: 9, charging: false }, permissions: { Accessibility: "Missing" } }),
  ).toEqual({ enabled: true, permissionMissing: true, connected: true, bus: null, battery: 9, paused: false });
  expect(fromStatusEvent({ connection: "Off", battery: null, permissions: {} })).toMatchObject({ enabled: false, connected: false });
  const usb = { connection: "Connected", battery: null, permissions: {}, bus: "Usb", paused: false };
  expect(toolbar(fromStatusEvent(usb))).toContain("Connected by USB");
  const bt = { ...usb, bus: "Bluetooth", battery: { level: 42, charging: false } };
  expect(toolbar(fromStatusEvent(bt))).toContain("Connected by Bluetooth · 42%");
});

test("stepped aside for Resolve the status says Paused, never Off or Turn on", () => {
  const event = { connection: "Off", battery: null, permissions: {}, bus: null, paused: true };
  const state = fromStatusEvent(event);
  expect(state).toMatchObject({ enabled: true, connected: true, paused: true });
  expect(toolbar(state)).toContain("Paused: DaVinci Resolve is open");
  expect(toolbar(state)).toContain('aria-checked="true"');
  const html = notice(state);
  expect(html).toContain("Paused while DaVinci Resolve is open.");
  expect(html).not.toContain("Turn on");
});

test("the update banner shows available, downloading, ready and restarting", () => {
  const banner = (status) => renderToString(<UpdateBanner update={{ status, version: "0.2.0", progress: 10 }} />);
  expect(banner("available")).toMatch(/Download and install.*Later/);
  expect(banner("downloading")).toContain("<progress");
  expect(banner("ready")).toMatch(/Restart to update.*Later/);
  expect(banner("restarting")).toContain("Restarting");
});

const notice = (state) => renderToString(<NoticeBar state={state} />);

test("the notice bar shows one notice, in priority: permission, Off, looking, Resolve, battery low", () => {
  const all = { enabled: false, permissionMissing: true, connected: false, bus: null, battery: 5, paused: true };
  const permission = notice(all);
  expect(permission).toContain("freewheel needs your permission.");
  expect(permission).toContain("Without it, your Speed Editor can’t press keys for you.");
  expect(permission).toContain(">Fix this</button>");
  expect(permission.match(/role="alert"/g)).toHaveLength(1);
  const off = notice({ ...all, permissionMissing: false });
  expect(off).toContain("freewheel is Off.");
  expect(off).toContain("Your Speed Editor is free for DaVinci Resolve.");
  expect(off).toContain(">Turn on</button>");
  expect(notice({ ...all, permissionMissing: false, enabled: true })).toContain("Looking for your Speed Editor…");
  expect(notice({ ...ok, battery: 5, paused: true })).toContain("Paused while DaVinci Resolve is open.");
  expect(notice({ ...ok, battery: 14 })).toContain("Battery is low (14%).");
  expect(notice({ ...ok, battery: 15 })).toBe("");
  expect(notice(ok)).toBe("");
});

test("the On/Off switch and Turn on invoke set_enabled on the running app and confirm it", async () => {
  const invoke = vi.fn(async () => {});
  vi.stubGlobal("window", { __TAURI__: { core: { invoke } } });
  try {
    expect(await switchOn(setEnabled, false)).toBe("freewheel is Off. DaVinci Resolve can use your Speed Editor.");
    expect(invoke).toHaveBeenLastCalledWith("set_enabled", { on: false });
    expect(await switchOn(setEnabled, true)).toBe("freewheel is On. Your Speed Editor runs your shortcuts.");
    expect(invoke).toHaveBeenLastCalledWith("set_enabled", { on: true });
    invoke.mockRejectedValueOnce("freewheel's device loop is not running");
    await expect(switchOn(setEnabled, true)).rejects.toBe("freewheel's device loop is not running");
  } finally {
    vi.unstubAllGlobals();
  }
});

test("a toast shows bottom-centre and clears after 2.8 s", () => {
  expect(TOAST_MS).toBe(2800);
  expect(renderToString(<Toast text={null} />)).toBe("");
  const html = renderToString(<Toast text="Saved" />);
  expect(html).toContain(">Saved</div>");
  // The prototype's toast: bottom-centre over the window, dark in both themes.
  expect(html).toContain('class="toast"');
  expect(readFileSync(new URL("./styles/base.css", import.meta.url), "utf8")).toMatch(
    /\.toast \{\s*position: absolute;\s*bottom: 24px;\s*left: 50%;\s*z-index: 40;\s*transform: translateX\(-50%\);[^}]*background: #1f1f22;/,
  );
  vi.useFakeTimers();
  const setToast = vi.fn();
  showToast(setToast, "Saved");
  expect(setToast).toHaveBeenLastCalledWith("Saved");
  vi.advanceTimersByTime(TOAST_MS - 1);
  expect(setToast).toHaveBeenCalledTimes(1);
  vi.advanceTimersByTime(1);
  expect(setToast).toHaveBeenLastCalledWith(null);
  vi.useRealTimers();
});
