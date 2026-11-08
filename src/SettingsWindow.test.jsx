import { readFileSync } from "node:fs";
import { renderToString } from "react-dom/server";
import { expect, test, vi } from "vitest";
import { NoticeBar, Toast, Toolbar } from "./SettingsWindow.jsx";
import UpdateBanner from "./UpdateBanner.jsx";
import { setEnabled } from "./settingsApi.js";
import { TOAST_MS, fromStatusEvent, showToast, switchOn } from "./shell.js";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");

const ok = { enabled: true, permissionMissing: false, connected: true, bus: "Usb", battery: null, paused: false };

test("the settings window opens at 1000x640, min 820x520, without the OS title bar (macOS: traffic lights over the app's header), and closing it leaves freewheel in the tray", () => {
  const [win] = JSON.parse(read("../src-tauri/tauri.conf.json")).app.windows;
  expect(win).toMatchObject({ width: 1000, height: 640, minWidth: 820, minHeight: 520, resizable: true, decorations: false, shadow: true });
  const [mac] = JSON.parse(read("../src-tauri/tauri.macos.conf.json")).app.windows;
  const { shadow: _windowsOnly, ...same } = win;
  expect(mac).toMatchObject({ ...same, decorations: true, titleBarStyle: "Overlay", hiddenTitle: true });
  const sim = read("../src-tauri/src/simulate.rs");
  expect(sim).toContain(".title_bar_style(tauri::TitleBarStyle::Overlay)");
  expect(sim).toContain("builder = builder.decorations(false).shadow(true);");
  const main = read("../src-tauri/src/main.rs");
  // Closing destroys the webview (no WebView2 idles in the tray); the app stays in the tray.
  expect(main).toMatch(/CloseRequested \{ api, \.\. \} = event \{\s*app_windows::close_requested\(window, api\);/);
  expect(main).toMatch(/RunEvent::ExitRequested \{\s*code: None, api, \.\.\s*\} = &event\s*\{\s*api\.prevent_exit\(\);/);
});

const toolbar = (state) => renderToString(<Toolbar tab="Keys" onTab={() => {}} state={state} onToggle={() => {}} />);

test("the toolbar has the five tabs and the On/Off switch", () => {
  const html = toolbar(ok);
  for (const tab of ["Keys", "Wheel", "DaVinci Resolve", "General", "Help &amp; backup"]) expect(html).toContain(`>${tab}</button>`);
  expect(html).toMatch(/aria-pressed="true"[^>]*>Keys</);
  expect(html).toContain('role="switch" aria-label="Turn freewheel on or off" aria-checked="true"');
  // Text only: no dot next to "Connected · USB".
  expect(html).not.toMatch(/dot/);
  expect(html).toMatch(/<button class="status"[^>]*data-tone="green"><span class="status-text">Connected · USB<\/span><\/button>/);
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
  // low battery turns the status text the warning colour
  expect(toolbar({ ...ok, bus: "Bluetooth", battery: 12 })).toContain('data-tone="orange"');
  expect(read("./styles/base.css")).toContain('.status[data-tone="orange"] { color: var(--warn); }');
});

test("the header says each status short (the full sentence is its tooltip): it fits the default 1000px window whole", () => {
  const all = { enabled: false, permissionMissing: true, connected: false, bus: null, battery: null, paused: true };
  const shown = (state) => {
    const html = toolbar(state);
    const short = html.match(/<span class="status-text">([^<]*)<\/span>/)[1];
    const full = html.match(/ title="([^"]*)"/)[1];
    return [short, full];
  };
  expect(shown(all)).toEqual(["Off", "Off: Resolve can use it"]);
  expect(shown({ ...all, enabled: true })).toEqual(["Needs permission", "Needs permission (click to fix)"]);
  expect(shown({ ...all, enabled: true, permissionMissing: false })).toEqual(["Looking for device", "Looking for your Speed Editor…"]);
  expect(shown({ ...ok, paused: true })).toEqual(["Paused for Resolve", "Paused: DaVinci Resolve is open"]);
  expect(shown(ok)).toEqual(["Connected · USB", "Connected by USB"]);
  expect(shown({ ...ok, bus: "Bluetooth", battery: 64 })).toEqual(["Bluetooth · 64%", "Connected by Bluetooth · 64%"]);
  expect(shown({ ...ok, bus: "Bluetooth", battery: 100 })).toEqual(["Bluetooth · 100%", "Connected by Bluetooth · 100%"]);
  expect(shown({ ...ok, bus: "Bluetooth" })).toEqual(["Connected · Bluetooth", "Connected by Bluetooth"]);
  expect(shown({ ...ok, bus: null })).toEqual(["Connected", "Connected"]);
  // Short enough for the room the header leaves at 1000px (~134px of 12.5px text; the e2e measures it).
  for (const state of [all, { ...ok, paused: true }, ok, { ...ok, bus: "Bluetooth", battery: 100 }, { ...ok, bus: "Bluetooth" }])
    expect(shown(state)[0].length).toBeLessThanOrEqual(21);
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
  expect(banner("downloading")).toMatch(/role="progressbar"[^>]*aria-valuenow="10"/);
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

test("the header status can never overlap the tabs, the switch or the window controls: it shrinks into its own slot", () => {
  const html = toolbar({ ...ok, connected: false });
  // The text sits in the status slot, between the tabs and the switch; the full text is the tooltip.
  expect(html).toMatch(/<div class="toolbar-right"><div class="status-slot"><button class="status"[^>]*title="Looking for your Speed Editor…"[^>]*aria-label="Looking for your Speed Editor…"/);
  expect(html).toContain('<span class="status-text">Looking for device</span>');
  const css = read("./styles/base.css");
  const rule = (selector) => {
    const at = css.indexOf(`\n${selector} {`);
    expect(at, selector).toBeGreaterThan(-1);
    return css.slice(at, css.indexOf("}", at));
  };
  // Flex all the way: the tabs keep their width, the slot takes what is left and clips, the text ends in an ellipsis.
  expect(rule(".tabs")).toMatch(/flex: 0 1 auto; min-width: 0; overflow: clip/);
  expect(rule(".tab")).toContain("flex: none");
  expect(rule(".toolbar-right")).toContain("flex: 1 1 auto");
  expect(rule(".toolbar-right > :not(.status-slot)")).toContain("flex: none");
  expect(rule(".status-slot")).toMatch(/flex: 1 1 0; min-width: 0;.*overflow: hidden; container-type: inline-size/);
  expect(rule(".status")).toMatch(/min-width: 0; max-width: 100%/);
  expect(rule(".status-text")).toMatch(/min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis/);
  // No dot to fall back on: the text never hides, it ends in an ellipsis.
  expect(css).not.toMatch(/status-dot|dot-green|\.status-text \{ display: none/);
  // Nothing in the header row is positioned out of the flow.
  for (const s of [".toolbar", ".tabs", ".toolbar-right", ".status-slot", ".status", ".status-text", ".window-controls"]) expect(rule(s)).not.toMatch(/position: (absolute|fixed)/);
});
