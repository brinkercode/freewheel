import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { checkWorkflow } from "./check-workflow.mjs";
import { WEBKIT, parseProc, places, windowsOf } from "./footprint-linux.mjs";
import { memory, overBudget } from "./footprint-windows.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const wf = read("../.github/workflows/e2e-linux.yml");
const budgets = JSON.parse(read("./footprint-budgets-linux.json"));

test("a /proc entry gives the pid, parent, name, RSS, private RSS and CPU time", () => {
  const stat = "4242 (WebKitWebProces) S 4200 4242 4200 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 9 0 1 2 3";
  const status = "Name:\tWebKitWebProces\nVmRSS:\t  204800 kB\nRssAnon:\t   51200 kB\nRssFile:\t  153600 kB\n";
  const p = parseProc(4242, stat, status);
  expect(p).toEqual({ pid: 4242, parent: 4200, name: "WebKitWebProces", workingSet: 200 * 1024 * 1024, privateBytes: 50 * 1024 * 1024, cpu100ns: 3 * 1e7 });
  // A name with spaces and parentheses ends at the last ')'.
  expect(parseProc(1, "1 (a (b) c) S 0 1", "").name).toBe("a (b) c");
});

test("WebKitGTK's processes count as the webview", () => {
  const procs = [
    { pid: 1, parent: 0, name: "freewheel", workingSet: 10, privateBytes: 5 },
    { pid: 2, parent: 1, name: "WebKitNetworkPr", workingSet: 3, privateBytes: 1 },
    { pid: 3, parent: 1, name: "WebKitWebProces", workingSet: 7, privateBytes: 2 },
    { pid: 4, parent: 1, name: "bwrap", workingSet: 1, privateBytes: 0 },
  ];
  expect(memory(procs, WEBKIT)).toMatchObject({ processes: 4, webviewProcesses: 3, workingSet: 21, webviewWorkingSet: 11 });
});

test("wmctrl -lp lines become the app's windows", () => {
  const list = "0x00400004  0 1234   host freewheel settings\n0x00600001  0 999    host Other\n0x00400023 -1 1234   host freewheel (Simulator)\n";
  expect(windowsOf(list, [1234])).toEqual([
    { id: "0x00400004", pid: 1234, title: "freewheel settings" },
    { id: "0x00400023", pid: 1234, title: "freewheel (Simulator)" },
  ]);
});

test("freewheel writes to the XDG dirs on Linux", () => {
  const p = places("/home/a", {});
  expect(p.config).toBe("/home/a/.config/freewheel");
  expect(p.logs).toBe("/home/a/.local/share/freewheel/logs");
  expect(p.iconCache).toBe("/home/a/.cache/freewheel/app-icons");
  expect(p.webviewData).toMatch(new RegExp(JSON.parse(read("../src-tauri/tauri.conf.json")).identifier + "$"));
  expect(places("/home/a", { XDG_CONFIG_HOME: "/c" }).config).toBe("/c/freewheel");
});

test("the Linux footprint job measures the release build under Xvfb and enforces the budgets", () => {
  expect(checkWorkflow(wf)).toEqual([]);
  expect(wf).toContain("name: footprint (ubuntu-24.04)");
  expect(wf).toContain('node "$TAURI_CLI" build --ci --no-bundle');
  expect(wf).toContain("node scripts/footprint-linux.mjs --app target/release/freewheel --budgets scripts/footprint-budgets-linux.json");
  // Closed to the tray, only WebKit's network process stays.
  for (const mode of ["normal", "simulate"]) expect(budgets.modes[mode].tray.webviewProcesses).toBe(1);
  expect(budgets.idleCpuPercentOfOneCore).toBeLessThanOrEqual(3);
  expect(overBudget({ exe: 40 * 1024 * 1024, exeLabel: "exe", modes: {} }, budgets)).toEqual(["exe MB: 40.00 > budget 32"]);
});

test("the Linux e2e job builds the embedded UI with the dialog stubs and drives it through tauri-driver", () => {
  expect(wf).toMatch(/^on:\n {2}pull_request:/m);
  expect(wf).toContain("name: e2e (ubuntu-24.04)");
  expect(wf.match(/runs-on: \S+/g)).toEqual(["runs-on: ubuntu-24.04", "runs-on: ubuntu-24.04", "runs-on: ubuntu-24.04"]);
  expect(wf).toContain("cargo build --release --locked -p freewheel --features tauri/custom-protocol,e2e");
  expect(wf).toContain("webkit2gtk-driver xvfb openbox wmctrl");
  expect(wf).toContain("cargo install tauri-driver --locked --version 2.1.0");
  expect(wf).toContain("node scripts/e2e-linux.mjs --app target/release/freewheel --out e2e-shots");
  expect(wf).toMatch(/name: e2e-linux-screenshots\n\s+path: e2e-shots/);
  for (const uses of wf.match(/uses: \S+/g)) expect(uses).toMatch(/@[0-9a-f]{40}$/);
});
