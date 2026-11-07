#!/usr/bin/env node
// What freewheel costs a Linux desktop, measured on the release build
// (.github/workflows/e2e-linux.yml, job footprint): binary and package
// size, what a session writes to disk (config, logs, icon cache, the
// WebKitGTK data folders), RAM of freewheel plus every WebKitGTK process it
// started (WebKitWebProcess, WebKitNetworkProcess... and their bubblewrap
// sandboxes, found through /proc parent links) with the settings window open
// and after it is closed to the tray, and CPU averaged over the idle time in
// the tray. Runs a normal start (no Speed Editor attached; a second launch
// reopens the window through the single-instance D-Bus name) and
// --simulate. Needs an X display with an EWMH window manager (CI: Xvfb +
// openbox) for `wmctrl`, which closes the windows the way a user does
// (WM_DELETE_WINDOW). Fails over the budgets (footprint-budgets-linux.json)
// and writes a Markdown table (to $GITHUB_STEP_SUMMARY in CI).
//
//   node scripts/footprint-linux.mjs --app target/release/freewheel [--package freewheel.AppImage]
//     [--budgets scripts/footprint-budgets-linux.json] [--out dir] [--idle 60]
import { execFileSync, spawn } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { cpuPercent, dirSize, memory, overBudget, parseArgs, summary, tree } from "./footprint-windows.mjs";

/** WebKitGTK's helper processes (and the bubblewrap / D-Bus proxy sandboxes around them). */
export const WEBKIT = /^(WebKit|bwrap$|xdg-dbus-proxy$)/;
/** The process names of the app itself: the binary, or an AppImage's runtime and AppRun. */
export const OS = { name: "Linux", exe: "freewheel (binary)", package: "AppImage", webview: "WebKitGTK", budgets: "scripts/footprint-budgets-linux.json" };

/** One process from /proc/<pid>/stat and status: pid, parent, name, RSS, private (anonymous) RSS, CPU in 100 ns units. */
export function parseProc(pid, stat, status, ticks = 100) {
  // comm may contain spaces and parentheses: it ends at the last ')'.
  const close = stat.lastIndexOf(")");
  const name = stat.slice(stat.indexOf("(") + 1, close);
  const f = stat.slice(close + 2).split(" ");
  const kb = (key) => Number(status.match(new RegExp(`^${key}:\\s+(\\d+) kB`, "m"))?.[1] ?? 0) * 1024;
  const cpuTicks = Number(f[11]) + Number(f[12]);
  return { pid, parent: Number(f[1]), name, workingSet: kb("VmRSS"), privateBytes: kb("RssAnon"), cpu100ns: (cpuTicks / ticks) * 1e7 };
}

/** Every process this user can read under `proc`. */
export function procs(proc = "/proc") {
  const out = [];
  for (const entry of readdirSync(proc)) {
    if (!/^\d+$/.test(entry)) continue;
    try {
      out.push(parseProc(Number(entry), readFileSync(path.join(proc, entry, "stat"), "utf8"), readFileSync(path.join(proc, entry, "status"), "utf8")));
    } catch {
      // gone, or not ours
    }
  }
  return out;
}

/** Where freewheel writes on Linux (directories::ProjectDirs, WebKitGTK's data and cache under the identifier). */
export function places(home = os.homedir(), env = process.env) {
  const config = env.XDG_CONFIG_HOME || path.join(home, ".config");
  const data = env.XDG_DATA_HOME || path.join(home, ".local", "share");
  const cache = env.XDG_CACHE_HOME || path.join(home, ".cache");
  const id = "com.brinkercode.freewheel";
  return {
    config: path.join(config, "freewheel"),
    logs: path.join(data, "freewheel", "logs"),
    appLocal: path.join(data, "freewheel"),
    iconCache: path.join(cache, "freewheel", "app-icons"),
    appCache: path.join(cache, "freewheel"),
    webviewData: path.join(data, id),
    webviewCache: path.join(cache, id),
  };
}

export function disk(home, env) {
  const p = places(home, env);
  const size = Object.fromEntries(Object.entries(p).map(([k, dir]) => [k, dirSize(dir).bytes]));
  return {
    config: size.config,
    logs: size.logs,
    iconCache: size.iconCache,
    webviewData: size.webviewData + size.webviewCache,
    total: size.config + size.appLocal + size.appCache + size.webviewData + size.webviewCache,
  };
}

/** The X windows (`wmctrl -lp`) of the processes in `pids`: id and title. */
export function windowsOf(list, pids) {
  return list
    .split("\n")
    .map((l) => l.match(/^(0x[0-9a-f]+)\s+-?\d+\s+(\d+)\s+\S+\s+(.*)$/i))
    .filter((m) => m && pids.includes(Number(m[2])))
    .map((m) => ({ id: m[1], pid: Number(m[2]), title: m[3] }));
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const appTree = (pid) => tree(procs(), pid);
const wmctrl = (...args) => execFileSync("wmctrl", args, { encoding: "utf8" });
const visibleWindows = (pid) => windowsOf(wmctrl("-lp"), appTree(pid).map((p) => p.pid));

async function waitFor(what, fn, ms = 60_000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const v = fn();
    if (v) return v;
    await sleep(1000);
  }
  throw new Error(`timed out waiting for ${what}`);
}

async function session(opts, mode) {
  const log = (...a) => console.log(`[${mode}]`, ...a);
  for (const dir of Object.values(places())) rmSync(dir, { recursive: true, force: true });
  const args = mode === "simulate" ? ["--simulate"] : [];
  const child = spawn(opts.app, args, { detached: true, stdio: "ignore" });
  child.unref();
  const pid = child.pid;
  log("started", pid);
  const mem = (procs_) => memory(procs_, WEBKIT);
  const memoryAt = {};
  const windows = mode === "simulate" ? 2 : 1;
  await waitFor("the window(s) and WebKitGTK", () => visibleWindows(pid).length >= windows && mem(appTree(pid)).webviewProcesses > 0);
  await sleep(15_000);
  memoryAt.open = mem(appTree(pid));
  log("open", JSON.stringify(memoryAt.open), JSON.stringify(visibleWindows(pid)));

  const closeAll = async () => {
    for (const w of visibleWindows(pid)) wmctrl("-i", "-c", w.id);
    await waitFor("every window to close", () => visibleWindows(pid).length === 0, 20_000);
  };
  await closeAll();
  await sleep(10_000);
  const before = appTree(pid);
  const t0 = Date.now();
  await sleep(opts.idle * 1000);
  const after = appTree(pid);
  const seconds = (Date.now() - t0) / 1000;
  if (!after.some((p) => p.pid === pid)) throw new Error(`${mode}: freewheel exited when its window closed`);
  memoryAt.tray = mem(after);
  const cpu = { ...cpuPercent(before, after, seconds, os.cpus().length), seconds: Math.round(seconds), cores: os.cpus().length };
  log("tray", JSON.stringify(memoryAt.tray), JSON.stringify(cpu), after.map((p) => p.name).join(","));

  if (mode === "normal") {
    // A second launch opens the settings window again (single instance, over D-Bus).
    spawn(opts.app, [], { detached: true, stdio: "ignore" }).unref();
    await waitFor("the settings window to open again", () => visibleWindows(pid).length >= 1 && mem(appTree(pid)).webviewProcesses > 0);
    await sleep(10_000);
    memoryAt.reopened = mem(appTree(pid));
    log("reopened", JSON.stringify(memoryAt.reopened));
    await closeAll();
    await sleep(10_000);
    memoryAt["tray again"] = mem(appTree(pid));
    log("tray again", JSON.stringify(memoryAt["tray again"]));
  }
  for (const p of appTree(pid).reverse()) {
    try {
      process.kill(p.pid);
    } catch {
      // already gone
    }
  }
  await sleep(3000);
  const written = disk();
  log("disk", JSON.stringify(written));
  return { memory: memoryAt, cpu, disk: written };
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  mkdirSync(opts.out, { recursive: true });
  const result = {
    exe: statSync(opts.app).size,
    exeLabel: "exe",
    package: opts.package ? statSync(opts.package).size : null,
    modes: {},
  };
  if (!existsSync(opts.app)) throw new Error(`no ${opts.app}`);
  result.modes.normal = await session(opts, "normal");
  result.modes.simulate = await session(opts, "simulate");
  const budgets = opts.budgets ? JSON.parse(readFileSync(opts.budgets, "utf8")) : {};
  result.problems = overBudget(result, budgets);
  writeFileSync(path.join(opts.out, "footprint.json"), JSON.stringify(result, null, 2));
  const table = summary(result, OS);
  console.log(table);
  writeFileSync(path.join(opts.out, "footprint.md"), table);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${table}\n`);
  if (result.problems.length) {
    console.error(result.problems.join("\n"));
    process.exit(1);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((e) => {
    console.error(e);
    process.exit(1);
  });
}
