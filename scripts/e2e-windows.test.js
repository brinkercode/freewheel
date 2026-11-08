import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { checkWorkflow } from "./check-workflow.mjs";
import { DEVTOOLS_PORT_ENV, DIALOG_DIR_ENV, RESPONSIVE_MS, appLog, flowLines, parseArgs, sessionBody, shotName, traceLines } from "./e2e-windows.mjs";
import { LINUX_APP, PLATFORMS, installLinuxApp } from "./e2e.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const wf = read("../.github/workflows/e2e-windows.yml");
const script = read("./e2e.mjs");

test("the e2e session starts the real exe with --simulate through tauri-driver", () => {
  expect(sessionBody("C:/a/freewheel.exe")).toEqual({
    capabilities: { alwaysMatch: { "tauri:options": { application: "C:/a/freewheel.exe", args: ["--simulate"] } } },
  });
  expect(sessionBody("C:/a/freewheel.exe", "9222").capabilities.alwaysMatch).toEqual({
    browserName: "webview2",
    "ms:edgeOptions": { debuggerAddress: "127.0.0.1:9222" },
  });
  expect(parseArgs(["--app", "x.exe", "--out", "o"])).toEqual({ driver: "http://127.0.0.1:4444", app: "x.exe", out: "o", port: null, "tauri-driver": null, "native-driver": null, "next-notes": null });
  expect(parseArgs(["--port", "9222", "--app", "x.exe"]).port).toBe("9222");
  expect(() => parseArgs([])).toThrow(/--app/);
  expect(() => parseArgs(["--nope", "1"])).toThrow(/unknown/);
});

test("the DevTools port env var matches the app", () => {
  expect(read("../src-tauri/src/simulate.rs")).toContain(`"${DEVTOOLS_PORT_ENV}"`);
});

test("the dialog stubs' env var matches the app, and only the e2e build has them", () => {
  expect(read("../src-tauri/src/e2e.rs")).toContain(`"${DIALOG_DIR_ENV}"`);
  expect(wf).toContain("--features tauri/custom-protocol,e2e");
});

test("every user-facing control is clicked and its result checked", () => {
  for (const control of [
    "Download logs", "a.show-file", "Back up", "Restore from a backup", "Turn on detailed logs for troubleshooting",
    "Let restored keys open apps", "Start freewheel when I log in", "Match my computer", "Update automatically",
    "Step aside automatically when DaVinci Resolve opens", "input.program-name", "Reset", "Change the volume",
    "Change screen brightness", '"Nothing"', "Press shortcuts", "Scroll up and down", "Sensitivity", "data-view=List",
    "Press a shortcut", "Play/Pause", "Mute", "Hold a key while I hold this one", "Turn a key on and off",
    "Turn the wheel into", "System volume", "Back to normal", "Open an app or website", "Search your apps",
    "button.choose-file", "Switch to another set of keys", "button.pill-add", "Rename", "Delete page", "Do nothing",
    "New profile", "Apps for this profile", 'aria-label="Colour"', 'aria-label="Icon"', "Program name", "Rename profile", "Delete profile", "front-window.ps1", "wmctrl", "Bring to front", "Another app",
    "Set up the wheel", "Show the ", "aria-label=\"Close\"", "Turn freewheel on or off", "Turn on", "Computer appearance",
    "Bluetooth 12%", "Empty setup", "Several set", "macOS permission", "Record a key", "New version", "Reload settings",
    "Settings window", "First run", 'aria-label="Do actions for real"', "marker-", "export_logs", 'tray("toggle")', 'tray("open-settings")', 'tray("start-at-login")',
    'tray("download-logs")', 'tray("status")', 'tray("update")', 'tray("quit")', "img.app-icon",
    "Notification: Speed Editor battery low", "Notification: freewheel — DaVinci Resolve started", "Restart to update",
  ])
    expect(script, control).toContain(control);
});

test("screenshots are numbered per step and window", () => {
  expect(shotName(3, "settings", "Keys: click CUT in the drawing")).toBe("03-settings-keys-click-cut-in-the-drawing.png");
  expect(shotName(12, "simulator", "simulator Speed Editor: Bluetooth 64%")).toBe("12-simulator-simulator-speed-editor-bluetooth-64.png");
});

test("every action is followed by a 5 s round trip through the app in both windows", () => {
  expect(RESPONSIVE_MS).toBe(5000);
  expect(script).toContain('["settings", "get_status", {}]');
  expect(script).toContain('["simulator", "simulate", { control: { type: "Status" } }]');
  for (const flow of ["button.get-started", "Continue", '[data-binding="Cut"]', "Open an app or website", "panel-try.is-saved", "Help & backup",
    "DaVinci Resolve", "appearance-option", '"Bluetooth 64%"', '"Not found"', '"Open"', '"Closed"', '"Off"', '"On"', "Turn wheel left", "aria-label=Maximise", "aria-label=Restore", "aria-label=Minimise", "isDecorated", '"macOS permission", "Missing"', "Open System Settings", "would open System Settings", '"macOS permission", "Allowed"'])
    expect(script).toContain(flow);
});

test("the e2e workflow is a Windows PR check that builds the embedded UI and uploads screenshots", () => {
  expect(checkWorkflow(wf)).toEqual([]);
  expect(wf).toMatch(/^on:\n {2}pull_request:/m);
  expect(wf).toMatch(/^ {2}push:\n {4}branches: \[main\]/m);
  expect(wf.match(/runs-on: \S+/g)).toEqual(["runs-on: windows-2025", "runs-on: windows-2025", "runs-on: windows-2025"]);
  expect(wf).toContain("cargo build --release --locked -p freewheel --features tauri/custom-protocol");
  expect(wf).toContain("node scripts/e2e-windows.mjs --port 9222 --app target/release/freewheel.exe --out e2e-shots");
  expect(wf).toMatch(/name: e2e-screenshots\n\s+path: e2e-shots/);
  // The clickable simulate build: the exe and the .cmd, each unzipped.
  expect(wf).toMatch(/- name: Upload freewheel\.exe \(simulate build\)\n(.*\n)\s+uses: actions\/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9 # v7\.0\.2\n\s+with:\n\s+path: target\/release\/freewheel\.exe\n\s+archive: false\n/);
  expect(wf).toMatch(/- name: Upload freewheel-simulate\.cmd\n(.*\n)\s+uses: actions\/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9 # v7\.0\.2\n\s+with:\n\s+path: src-tauri\/windows\/freewheel-simulate\.cmd\n\s+archive: false\n/);
});

test("one click-through for both OSes: only how the app starts and what is OS-specific differ", () => {
  expect(Object.keys(PLATFORMS)).toEqual(["windows", "linux"]);
  expect(PLATFORMS.windows.installedApp({ path: "explorer.exe", args: ["C:/x/Spotify.lnk"] })).toBe(true);
  expect(PLATFORMS.linux.installedApp({ path: "/usr/share/applications/org.gnome.Calculator.desktop", args: [] })).toBe(true);
  expect(PLATFORMS.linux.installedApp({ path: "explorer.exe", args: ["x.lnk"] })).toBe(false);
  expect([PLATFORMS.windows.resolve, PLATFORMS.linux.resolve]).toEqual(["Resolve.exe", "resolve"]);
  // The stubbed 'Choose file…' answers with these (src-tauri/src/e2e.rs).
  const stub = read("../src-tauri/src/e2e.rs");
  for (const p of Object.values(PLATFORMS)) expect(stub).toContain(`"${p.chosen}"`);
  expect(read("./e2e-linux.mjs")).toContain('main("linux")');
  expect(read("./e2e-windows.mjs")).toContain('main("windows")');
});

test("the Linux e2e installs its own desktop entry with a themed icon", async () => {
  const { mkdtempSync, readFileSync: rf, existsSync } = await import("node:fs");
  const { join } = await import("node:path");
  const { tmpdir } = await import("node:os");
  const home = mkdtempSync(join(tmpdir(), "fw-e2e-"));
  const desktop = installLinuxApp(home, fileURLToPath(new URL("../src-tauri/icons/32x32.png", import.meta.url)), "");
  expect(desktop).toBe(join(home, ".local/share/applications", `${LINUX_APP.id}.desktop`));
  expect(rf(desktop, "utf8")).toContain(`Name=${LINUX_APP.name}\nExec=/usr/bin/true %F\nIcon=${LINUX_APP.id}`);
  expect(existsSync(join(home, ".local/share/icons/hicolor/32x32/apps", `${LINUX_APP.id}.png`))).toBe(true);
});

test("the e2e checks the header never overlaps at 820px and that the tray and the window share one On/Off", async () => {
  const { HEADER_JS } = await import("./e2e-windows.mjs");
  for (const part of [".toolbar .tab", ".toolbar .switch", ".toolbar .window-control", "tabs clipped", "header overflows", "text.scrollWidth > text.clientWidth"]) expect(HEADER_JS).toContain(part);
  // Every status, short and whole at 1000px; the window closed and built again twice.
  for (const short of ["Connected · USB", "Bluetooth · 64%", "Bluetooth · 100%", "Looking for device", "Paused for Resolve", "Needs permission", '"Off"']) expect(script).toContain(short);
  expect(script).toContain("for (const round of [1, 2])");
  for (const step of [
    "header: Looking for your Speed Editor… at 820px never overlaps",
    "header: every status at 820px and 1000px, light and dark",
    "On/Off: the window's switch moves the tray's check",
    "On/Off: tray Off, window On, window Off, tray On",
    "On/Off: a settings window opened after the tray's Off reads Off",
    "reopen ${round}: tray Check for updates acts in the new window",
    "reopen ${round}: tray Download logs opens Help & backup in the new window",
    "reopen ${round}: tray Needs permission opens the fix in the new window",
    "reopen ${round}: Record a key opens CUT recording in the new window",
    "reopen ${round}: tray Open settings brings the minimised new window back",
    "closed: tray Check for updates opens the window and checks there",
    "closed: tray Download logs opens the window on Help & backup",
    "closed: tray Needs permission opens the window on the fix",
    "closed: Record a key opens the window on CUT recording",
  ])
    expect(script).toContain(step);
  expect(script).toContain('invoke("get_status")).enabled');
  for (const shot of ["820-${appearance.toLowerCase()}", "1000-${appearance.toLowerCase()}"]) expect(script).toContain(shot);
  // Only the --simulate window may resize the settings window.
  expect(read("../src-tauri/capabilities/simulator.json")).toContain('"core:window:allow-set-size"');
  expect(read("../src-tauri/capabilities/settings.json")).not.toContain("set-size");
});

test("a failed step dumps the update flow: the settings page's trace and the app's log lines about updates, windows, the tray and freezes", () => {
  expect(appLog("L")).toBe(join("L", "freewheel", "data", "logs", "freewheel.log"));
  const log = [
    "t1 INFO freewheel_core::connection: Speed Editor opened",
    "t2 INFO freewheel::tray: tray: update what=\"restart\"",
    "t3 INFO freewheel::app_windows: settings window: delivered event=\"tray-update\"",
    "t4 WARN freewheel::simulate: main thread blocked: no answer ms=2000",
    "t5 INFO freewheel::selfupdate: update: installing version=0.5.0",
  ].join("\r\n");
  expect(flowLines(log)).toEqual(log.split("\r\n").slice(1));
  expect(flowLines(log, 1)).toEqual([log.split("\r\n")[4]]);
  expect(traceLines([{ at: "A", what: "update", status: "ready" }])).toEqual(['A update {"status":"ready"}']);
  expect(traceLines(undefined)).toEqual([]);
  const script = read("./e2e.mjs");
  expect(script).toContain('ask("settings", "return window.__freewheelTrace ?? []")');
  // The names the dump keys on are the ones the app logs.
  expect(read("../src-tauri/src/simulate.rs")).toContain('"main thread blocked: no answer"');
  expect(read("../src-tauri/src/app_windows.rs")).toContain('"settings window: delivered"');
  expect(read("../src-tauri/src/selfupdate.rs")).toContain('"update: installing"');
  expect(read("../src-tauri/src/main.rs")).toContain("simulate::watch_main_thread(app.handle().clone());");
});
