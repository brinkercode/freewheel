import { renderToString } from "react-dom/server";
import { expect, test, vi } from "vitest";
import General, { applyAppearance, changeSetting, switchLogin, updateButton, updateText } from "./General.jsx";
import { checkNow } from "./updater.js";

const settings = { enabled: true, start_at_login: false, appearance: "System", auto_update: true };
const config = { version: 1, settings };
const general = (props) => renderToString(<General settings={settings} onSetting={() => {}} {...props} />);

test("Appearance offers Match my computer, Light and Dark as preview cards, the choice pressed", () => {
  const html = general({ settings: { ...settings, appearance: "Dark" } });
  for (const [label, icon] of [["Match my computer", "contrast"], ["Light", "light_mode"], ["Dark", "dark_mode"]])
    expect(html).toMatch(new RegExp(`data-icon="${icon}"[^>]*><svg[^>]*>.*?</svg></span>${label}</span></button>`));
  expect(html.match(/class="preview"/g)).toHaveLength(3);
  expect(html).toMatch(/aria-pressed="true"[^>]*>.*?Dark<\/span><\/button>/);
  expect(html.match(/aria-pressed="true"/g)).toHaveLength(1);
});

test("choosing an appearance applies it at once and saves it in the config", async () => {
  const root = { style: {}, dataset: {} };
  const setConfig = vi.fn();
  const save = vi.fn(async () => {});
  await changeSetting({ config, key: "appearance", value: "Light", setConfig, save, root });
  expect(root.style.colorScheme).toBe("light");
  expect(setConfig).toHaveBeenCalledWith({ ...config, settings: { ...settings, appearance: "Light" } });
  expect(save).toHaveBeenCalledWith({ ...config, settings: { ...settings, appearance: "Light" } });
  applyAppearance("Dark", root);
  expect(root.style.colorScheme).toBe("dark");
  applyAppearance("System", root);
  expect(root.style.colorScheme).toBe("light dark");
});

test("a setting change saves over the config read fresh, keeping mappings changed since the window loaded", async () => {
  const fresh = { ...config, layers: [{ id: "base", name: "Base", bindings: { Cut: { kind: "Keystroke" } } }] };
  const setConfig = vi.fn();
  const save = vi.fn(async () => {});
  await changeSetting({ config, key: "auto_update", value: false, setConfig, save, load: async () => fresh });
  expect(save).toHaveBeenCalledWith({ ...fresh, settings: { ...settings, auto_update: false } });
  expect(setConfig).toHaveBeenCalledWith({ ...fresh, settings: { ...settings, auto_update: false } });
});

test("a failed save is logged", async () => {
  const log = vi.fn();
  await changeSetting({ config, key: "auto_update", value: false, setConfig: () => {}, save: async () => Promise.reject("disk full"), log });
  expect(log).toHaveBeenCalledWith("freewheel: saving", "auto_update", "failed:", "disk full");
});

test("'Start freewheel when I log in' shows the real autostart state and sets it", async () => {
  const login = (startAtLogin) => general({ startAtLogin, onStartAtLogin: () => {} });
  expect(login(true)).toMatch(/aria-label="Start freewheel when I log in" aria-checked="true"/);
  expect(login(false)).toMatch(/aria-label="Start freewheel when I log in" aria-checked="false"/);
  expect(login(null)).toMatch(/aria-label="Start freewheel when I log in" aria-checked="false" disabled/);
  const setLogin = vi.fn();
  let c = config;
  const setConfig = (f) => (c = f(c));
  const set = vi.fn(async (on) => on);
  await switchLogin({ on: true, set, get: async () => false, setLogin, setConfig });
  expect(set).toHaveBeenCalledWith(true);
  expect(setLogin).toHaveBeenLastCalledWith(true);
  expect(c.settings.start_at_login).toBe(true);
  const log = vi.fn();
  await switchLogin({ on: false, set: async () => Promise.reject("denied"), get: async () => true, setLogin, setConfig, log });
  expect(log).toHaveBeenCalledWith("freewheel: start at login failed:", "denied");
  expect(setLogin).toHaveBeenLastCalledWith(true);
});

test("the version reads 'freewheel <version>' from the app", () => {
  expect(general({ version: "1.4.2" })).toContain(">freewheel 1.4.2</div>");
});

test("'Check for updates' shows idle, checking, up to date, downloading and ready", () => {
  const sub = (update) => general({ update, onCheck: () => {} });
  expect(sub(null)).toContain(updateText(null));
  const checking = sub({ status: "checking" });
  expect(checking).toContain("Checking…");
  expect(checking).toContain('class="spinner"');
  expect(sub({ status: "current" })).toContain("You’re up to date.");
  expect(sub({ status: "downloading", version: "0.2.0", progress: 40 })).toContain("Downloading the new version…");
  expect(sub({ status: "ready", version: "0.2.0" })).toContain("A new version is ready.");
  expect(sub(null)).toContain("freewheel checks for updates by itself.");
  expect(sub(null)).not.toContain("spinner");
});

const now = { minMs: 0 };

test("checking finds no update: checking then up to date; a failure is an error with Try again", async () => {
  const states = [];
  await checkNow({ ...now, check: async () => null, onState: (s) => states.push(s) });
  expect(states).toEqual([{ status: "checking" }, { status: "current" }]);
  const failed = [];
  const log = vi.fn();
  await checkNow({ ...now, check: async () => Promise.reject("offline"), onState: (s) => failed.push(s), log });
  expect(failed).toEqual([{ status: "checking" }, { status: "error", during: "check", message: "offline" }]);
  expect(log).toHaveBeenCalledWith("update check failed", "offline");
  const html = general({ update: failed[1], onCheck: () => {} });
  expect(html).toContain("Couldn’t check for updates.");
  expect(html).toContain(">Try again</button>");
});

test("a build without the updater says so instead of doing nothing", () => {
  const html = general({ update: { status: "error", during: "check", notConfigured: true, message: "plugin updater not found" }, onCheck: () => {} });
  expect(html).toContain("This test build can’t update itself.");
});

test("the version card's button follows the update: check, download, downloading, restart", () => {
  const on = { onCheck: () => "check", onDownload: () => "download", onRestart: () => "restart" };
  const b = (update) => {
    const x = updateButton(update, on);
    return [x.label, x.onClick?.() ?? null, !!x.spin, !!x.disabled];
  };
  expect(b(null)).toEqual(["Check for updates", "check", false, false]);
  expect(b({ status: "checking" })).toEqual(["Check for updates", null, true, true]);
  expect(b({ status: "current" })).toEqual(["Check for updates", "check", false, false]);
  expect(b({ status: "available", version: "0.2.0" })).toEqual(["Download and install", "download", false, false]);
  expect(b({ status: "downloading", version: "0.2.0", progress: 3 })).toEqual(["Downloading…", null, true, true]);
  expect(b({ status: "ready", version: "0.2.0" })).toEqual(["Restart to update", "restart", false, false]);
  expect(b({ status: "restarting", version: "0.2.0" })).toEqual(["Restarting…", null, true, true]);
  expect(b({ status: "error", during: "check" })).toEqual(["Try again", "check", false, false]);
  expect(b({ status: "error", during: "download", version: "0.2.0" })).toEqual(["Try again", "download", false, false]);
  expect(b({ status: "error", during: "install", version: "0.2.0" })).toEqual(["Try again", "restart", false, false]);
  const downloading = general({ update: { status: "downloading", version: "0.2.0", progress: 40 }, onCheck: () => {} });
  expect(downloading).toContain("Downloading the new version… 40%");
  expect(downloading).toMatch(/role="progressbar"[^>]*aria-valuenow="40"/);
});

test("checking finds an update: with Update automatically it downloads to ready, else it is available", async () => {
  const update = { version: "0.2.0", download: async () => {} };
  const states = [];
  await checkNow({ ...now, check: async () => update, onState: (s) => states.push(s?.status) });
  expect(states).toEqual(["checking", "downloading", "ready"]);
  const manual = [];
  await checkNow({ ...now, check: async () => update, autoUpdate: false, onState: (s) => manual.push(s?.status) });
  expect(manual).toEqual(["checking", "available"]);
});

test("'Update automatically' is on by default and its change is saved in the config", async () => {
  const html = general({ settings: undefined, onSetting: undefined });
  expect(html).toContain("New versions download by themselves. You choose when to restart.");
  expect(html).toMatch(/aria-label="Update automatically" aria-checked="true"/);
  expect(general({ settings: { ...settings, auto_update: false } })).toMatch(/aria-label="Update automatically" aria-checked="false"/);
  const save = vi.fn(async () => {});
  await changeSetting({ config, key: "auto_update", value: false, setConfig: () => {}, save });
  expect(save).toHaveBeenCalledWith({ ...config, settings: { ...settings, auto_update: false } });
});

test("Linux without a tray host says where the tray menu went", () => {
  expect(general({})).not.toContain("No tray icon");
  const html = general({ trayHint: "On GNOME, add the AppIndicator extension." });
  expect(html).toContain("No tray icon");
  expect(html).toContain("AppIndicator extension");
});
