import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { checkWorkflow } from "./check-workflow.mjs";
import { DEVTOOLS_PORT_ENV, DIALOG_DIR_ENV, RESPONSIVE_MS, parseArgs, sessionBody, shotName } from "./e2e-windows.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const wf = read("../.github/workflows/e2e-windows.yml");
const script = read("./e2e-windows.mjs");

test("the e2e session starts the real exe with --simulate through tauri-driver", () => {
  expect(sessionBody("C:/a/freewheel.exe")).toEqual({
    capabilities: { alwaysMatch: { "tauri:options": { application: "C:/a/freewheel.exe", args: ["--simulate"] } } },
  });
  expect(sessionBody("C:/a/freewheel.exe", "9222").capabilities.alwaysMatch).toEqual({
    browserName: "webview2",
    "ms:edgeOptions": { debuggerAddress: "127.0.0.1:9222" },
  });
  expect(parseArgs(["--app", "x.exe", "--out", "o"])).toEqual({ driver: "http://127.0.0.1:4444", app: "x.exe", out: "o", port: null, "next-notes": null });
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
    "Set up the wheel", "Show the ", "aria-label=\"Close\"", "Turn freewheel on or off", "Turn on", "Computer appearance",
    "Bluetooth 12%", "Empty setup", "Several set", "macOS permission", "Record a key", "New version", "Reload settings",
    "Settings window", "First run", "fw-sim-switch", 'tray("toggle")', 'tray("open-settings")', 'tray("start-at-login")',
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
  expect(wf).toMatch(/name: freewheel-windows-simulate\n\s+path: simulate-build/);
  expect(wf).toContain("cp target/release/freewheel.exe src-tauri/windows/freewheel-simulate.cmd simulate-build/");
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
