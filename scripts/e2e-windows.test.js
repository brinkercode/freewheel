import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { checkWorkflow } from "./check-workflow.mjs";
import { DEVTOOLS_PORT_ENV, RESPONSIVE_MS, parseArgs, sessionBody, shotName } from "./e2e-windows.mjs";

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
  expect(parseArgs(["--app", "x.exe", "--out", "o"])).toEqual({ driver: "http://127.0.0.1:4444", app: "x.exe", out: "o", port: null });
  expect(parseArgs(["--port", "9222", "--app", "x.exe"]).port).toBe("9222");
  expect(() => parseArgs([])).toThrow(/--app/);
  expect(() => parseArgs(["--nope", "1"])).toThrow(/unknown/);
});

test("the DevTools port env var matches the app", () => {
  expect(read("../src-tauri/src/simulate.rs")).toContain(`"${DEVTOOLS_PORT_ENV}"`);
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
  expect(wf.match(/runs-on: \S+/g)).toEqual(["runs-on: windows-2025", "runs-on: windows-2025"]);
  expect(wf).toContain("cargo build --release --locked -p freewheel --features tauri/custom-protocol");
  expect(wf).toContain("node scripts/e2e-windows.mjs --port 9222 --app target/release/freewheel.exe --out e2e-shots");
  expect(wf).toMatch(/name: e2e-screenshots\n\s+path: e2e-shots/);
  expect(wf).toMatch(/name: freewheel-windows-simulate\n\s+path: simulate-build/);
  expect(wf).toContain("cp target/release/freewheel.exe src-tauri/windows/freewheel-simulate.cmd simulate-build/");
});
