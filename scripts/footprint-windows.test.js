import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { MB, cpuPercent, memory, overBudget, parseArgs, places, summary, tree } from "./footprint-windows.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const wf = read("../.github/workflows/e2e-windows.yml");
const budgets = JSON.parse(read("./footprint-budgets.json"));

const proc = (pid, parent, name, mb = 10, cpu = 0) => ({ pid, parent, name, workingSet: mb * MB, privateBytes: (mb / 2) * MB, cpu100ns: cpu });

test("the app's tree is freewheel.exe and every WebView2 process under it, nothing else", () => {
  const procs = [
    proc(1, 0, "explorer.exe"),
    proc(10, 1, "freewheel.exe", 20),
    proc(11, 10, "msedgewebview2.exe", 30),
    proc(12, 11, "msedgewebview2.exe", 40),
    proc(13, 11, "msedgewebview2.exe", 50),
    proc(20, 1, "msedgewebview2.exe", 99),
  ];
  const t = tree(procs, 10);
  expect(t.map((p) => p.pid).sort()).toEqual([10, 11, 12, 13]);
  expect(memory(t)).toEqual({
    processes: 4,
    webviewProcesses: 3,
    workingSet: 140 * MB,
    privateBytes: 70 * MB,
    webviewWorkingSet: 120 * MB,
    webviewPrivateBytes: 60 * MB,
    names: ["freewheel.exe", "msedgewebview2.exe", "msedgewebview2.exe", "msedgewebview2.exe"],
  });
  expect(tree(procs, 99)).toEqual([]);
});

test("a process whose parent pid was reused by the app is not the app's child", () => {
  const at = (p, created) => ({ ...p, created });
  const procs = [
    at(proc(10, 1, "freewheel-0.1.0-windows-x64.exe", 20), 5000),
    // Started (by an earlier owner of pid 10) before the app: not its child.
    at(proc(30, 10, "conhost.exe", 36), 1000),
    at(proc(11, 10, "msedgewebview2.exe", 30), 6000),
    at(proc(12, 11, "msedgewebview2.exe", 40), 7000),
  ];
  expect(tree(procs, 10).map((p) => p.pid)).toEqual([10, 11, 12]);
  expect(read("./footprint-windows.ps1")).toContain("ToUnixTimeMilliseconds()");
});

test("idle CPU is the tree's CPU time over the wall time, per core and per machine", () => {
  const before = [proc(10, 1, "freewheel.exe", 1, 1_000_000)];
  // 0.3 s of CPU over 60 s, plus a process that started in between (0.3 s).
  const after = [proc(10, 1, "freewheel.exe", 1, 4_000_000), proc(11, 10, "msedgewebview2.exe", 1, 3_000_000)];
  const cpu = cpuPercent(before, after, 60, 4);
  expect(cpu.oneCore).toBeCloseTo(1);
  expect(cpu.machine).toBeCloseTo(0.25);
});

const result = (trayMB, cpu) => ({
  exe: 10 * MB,
  exeName: "freewheel-0.1.0-windows-x64.exe",
  modes: {
    normal: {
      memory: { open: memory([proc(1, 0, "freewheel.exe", 200)]), tray: memory([proc(1, 0, "freewheel.exe", trayMB)]) },
      cpu: { oneCore: cpu, machine: cpu / 4, seconds: 60, cores: 4 },
      disk: { config: 1000, logs: 2000, iconCache: 0, webviewData: 5 * MB, total: 6 * MB },
    },
  },
});

test("a number over its budget fails; a missing budget is not checked", () => {
  const b = { exeMB: 12, idleCpuPercentOfOneCore: 1, modes: { normal: { diskMB: 8, tray: { workingSetMB: 30, privateMB: 20 } } } };
  expect(overBudget(result(20, 0.1), b)).toEqual([]);
  expect(overBudget(result(40, 2), b)).toEqual([
    "normal tray working set MB: 40.00 > budget 30",
    "normal idle CPU % of one core: 2.00 > budget 1",
  ]);
  expect(overBudget(result(42, 0), b)).toContain("normal tray private MB: 21.00 > budget 20");
  expect(overBudget(result(40, 2), {})).toEqual([]);
});

test("the summary is a table per mode with sizes, disk and CPU", () => {
  const r = { ...result(20, 0.05), problems: [] };
  const md = summary(r);
  expect(md).toContain("| freewheel-0.1.0-windows-x64.exe (the download) | 10.0 MB |");
  expect(md).not.toMatch(/zip/);
  expect(md).toContain("| tray | 1 (0) | 20.0 MB | 10.0 MB | 0.0 MB | 0.0 MB |");
  expect(md).toContain("CPU 0.050 % of one core");
  expect(md).toContain("WebView2 data 5.0 MB");
  expect(summary({ ...r, problems: ["x > budget 1"] })).toContain("- x > budget 1");
});

test("the arguments and the places freewheel writes on Windows", () => {
  expect(parseArgs(["--app", "a.exe", "--idle", "5"])).toMatchObject({ app: "a.exe", idle: 5, out: "footprint" });
  expect(() => parseArgs([])).toThrow(/--app/);
  expect(() => parseArgs(["--x", "1"])).toThrow(/unknown/);
  const p = places({ APPDATA: "R", LOCALAPPDATA: "L" });
  expect(p.config).toMatch(/^R.freewheel$/);
  expect(p.logs).toMatch(/^L.freewheel.data.logs$/);
  expect(p.iconCache).toMatch(/^L.freewheel.cache.app-icons$/);
  // Tauri's WebView2 data folder is named after the identifier.
  expect(p.webviewData).toMatch(new RegExp(JSON.parse(read("../src-tauri/tauri.conf.json")).identifier + "$"));
});

test("the footprint job measures the release build and enforces the budgets", () => {
  expect(wf).toContain("name: footprint (windows-2025)");
  expect(wf).toContain('node "$TAURI_CLI" build --ci --no-bundle');
  expect(wf).toContain("node scripts/portable-release.mjs package --os windows");
  expect(wf).toContain("exe=$(ls footprint-package/freewheel-*-windows-x64.exe | head -n 1)");
  expect(wf).toContain('node scripts/footprint-windows.mjs --app "$exe" --budgets scripts/footprint-budgets.json');
  // The download is the exe: its size is the budget, no zip.
  expect(budgets.packageMB).toBeUndefined();
  expect(budgets.exeMB).toBeLessThanOrEqual(20);
  expect(budgets.idleCpuPercentOfOneCore).toBeLessThanOrEqual(1);
  // Closed to the tray, no WebView2 runs (src-tauri/src/app_windows.rs).
  for (const mode of ["normal", "simulate"]) {
    expect(budgets.modes[mode].tray.webviewProcesses).toBe(0);
    expect(budgets.modes[mode].tray.workingSetMB).toBeLessThan(64);
  }
  expect(overBudget({ exe: 0, modes: { normal: { memory: { tray: memory([proc(1, 0, "freewheel.exe"), proc(2, 1, "msedgewebview2.exe")]) }, cpu: { oneCore: 0 }, disk: { total: 0 } } } }, budgets)).toEqual([
    "normal tray WebView2 processes: 1.00 > budget 0",
  ]);
});
