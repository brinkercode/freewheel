#!/usr/bin/env node
// What freewheel costs a Windows PC, measured on the release-built exe
// (.github/workflows/e2e-windows.yml, job footprint): the exe's size (the
// release file users download: freewheel-<ver>-windows-x64.exe),
// what a session writes to disk (config, logs, icon cache, the WebView2 data
// folder), RAM of freewheel.exe plus every WebView2 process it started
// (msedgewebview2.exe, found through the parent process ids) with the settings
// window open and after 60 s closed to the tray, and CPU averaged over 60 s
// idle in the tray. Runs both a normal start (no Speed Editor attached) and
// --simulate. Fails when a number is over its budget (footprint-budgets.json)
// and writes a Markdown table (to $GITHUB_STEP_SUMMARY in CI).
//
//   node scripts/footprint-windows.mjs --app <freewheel-<ver>-windows-x64.exe>
//     [--budgets scripts/footprint-budgets.json] [--out dir] [--idle 60]
import { execFileSync, spawn } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const MB = 1024 * 1024;
const PROBE = fileURLToPath(new URL("./footprint-windows.ps1", import.meta.url));

export function parseArgs(argv) {
  const opts = { app: null, budgets: null, out: "footprint", idle: 60 };
  for (let i = 0; i < argv.length; i += 2) {
    const [key, value] = [argv[i], argv[i + 1]];
    if (key === "--app") opts.app = value;
    else if (key === "--budgets") opts.budgets = value;
    else if (key === "--out") opts.out = value;
    else if (key === "--idle") opts.idle = Number(value);
    else throw new Error(`unknown option ${key}`);
  }
  if (!opts.app) throw new Error("--app <freewheel.exe> is required");
  return opts;
}

/** `root` and every process it started, directly or not (WebView2's renderers are children of its browser process). */
export function tree(procs, root) {
  const out = [];
  const seen = new Set();
  const visit = (pid) => {
    for (const p of procs) {
      if (p.pid === pid && !seen.has(pid)) {
        seen.add(pid);
        out.push(p);
      }
    }
    for (const p of procs) if (p.parent === pid && !seen.has(p.pid) && p.pid !== root) visit(p.pid);
  };
  visit(root);
  return out;
}

/** RAM of a process tree: totals and the WebView2 share. */
export function memory(procs) {
  const sum = (list, k) => list.reduce((n, p) => n + p[k], 0);
  const webview = procs.filter((p) => /^msedgewebview2\.exe$/i.test(p.name));
  return {
    processes: procs.length,
    webviewProcesses: webview.length,
    workingSet: sum(procs, "workingSet"),
    privateBytes: sum(procs, "privateBytes"),
    webviewWorkingSet: sum(webview, "workingSet"),
    webviewPrivateBytes: sum(webview, "privateBytes"),
  };
}

/** CPU % (of one core, and of the whole machine) a tree used between two snapshots `seconds` apart. */
export function cpuPercent(before, after, seconds, cores) {
  const start = new Map(before.map((p) => [p.pid, p.cpu100ns]));
  let used = 0;
  for (const p of after) used += p.cpu100ns - (start.get(p.pid) ?? 0);
  const oneCore = (used / 1e7 / seconds) * 100;
  return { oneCore, machine: oneCore / cores };
}

/** Bytes under `dir` (0 when it does not exist), and how many files. */
export function dirSize(dir) {
  let bytes = 0;
  let files = 0;
  if (!existsSync(dir)) return { bytes, files };
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      const sub = dirSize(p);
      bytes += sub.bytes;
      files += sub.files;
    } else {
      try {
        bytes += statSync(p).size;
        files += 1;
      } catch {
        // gone while counting
      }
    }
  }
  return { bytes, files };
}

/** Where freewheel writes on Windows (directories::ProjectDirs, Tauri's WebView2 data folder). */
export function places(env = process.env) {
  const roaming = env.APPDATA;
  const local = env.LOCALAPPDATA;
  return {
    config: path.join(roaming, "freewheel"),
    logs: path.join(local, "freewheel", "data", "logs"),
    iconCache: path.join(local, "freewheel", "cache", "app-icons"),
    appLocal: path.join(local, "freewheel"),
    webviewData: path.join(local, "com.brinkercode.freewheel"),
    webviewRoaming: path.join(roaming, "com.brinkercode.freewheel"),
  };
}

export function disk(env = process.env) {
  const p = places(env);
  const size = Object.fromEntries(Object.entries(p).map(([k, dir]) => [k, dirSize(dir)]));
  return {
    config: size.config.bytes,
    logs: size.logs.bytes,
    iconCache: size.iconCache.bytes,
    webviewData: size.webviewData.bytes + size.webviewRoaming.bytes,
    total: size.config.bytes + size.appLocal.bytes + size.webviewData.bytes + size.webviewRoaming.bytes,
  };
}

/** The budget problems in `result` (empty when every number is in budget); a missing budget is not checked. */
export function overBudget(result, budgets) {
  const problems = [];
  const check = (what, value, limit) => {
    if (limit != null && value > limit) problems.push(`${what}: ${value.toFixed(2)} > budget ${limit}`);
  };
  check("exe (download) MB", result.exe / MB, budgets.exeMB);
  for (const [mode, run] of Object.entries(result.modes)) {
    const b = budgets.modes?.[mode] ?? {};
    check(`${mode} disk MB`, run.disk.total / MB, b.diskMB);
    for (const [phase, mem] of Object.entries(run.memory)) {
      check(`${mode} ${phase} working set MB`, mem.workingSet / MB, b[phase]?.workingSetMB);
      check(`${mode} ${phase} private MB`, mem.privateBytes / MB, b[phase]?.privateMB);
      check(`${mode} ${phase} WebView2 processes`, mem.webviewProcesses, b[phase]?.webviewProcesses);
    }
    check(`${mode} idle CPU % of one core`, run.cpu.oneCore, budgets.idleCpuPercentOfOneCore);
  }
  return problems;
}

const mb = (n) => `${(n / MB).toFixed(1)} MB`;

/** The job summary: one table per mode plus the sizes. */
export function summary(result) {
  const lines = ["## freewheel footprint (Windows, release build)", ""];
  lines.push(`| | |`, `|---|---|`, `| ${result.exeName ?? "freewheel.exe"} (the download) | ${mb(result.exe)} |`);
  lines.push("");
  for (const [mode, run] of Object.entries(result.modes)) {
    lines.push(`### ${mode}`, "");
    lines.push("| phase | processes (WebView2) | working set | private bytes | WebView2 working set | WebView2 private |");
    lines.push("|---|---|---|---|---|---|");
    for (const [phase, m] of Object.entries(run.memory)) {
      lines.push(
        `| ${phase} | ${m.processes} (${m.webviewProcesses}) | ${mb(m.workingSet)} | ${mb(m.privateBytes)} | ${mb(m.webviewWorkingSet)} | ${mb(m.webviewPrivateBytes)} |`,
      );
    }
    lines.push("");
    lines.push(`Idle in tray for ${run.cpu.seconds} s: CPU ${run.cpu.oneCore.toFixed(3)} % of one core (${run.cpu.machine.toFixed(3)} % of ${run.cpu.cores} cores).`);
    const d = run.disk;
    lines.push("", `Disk after the session: config ${mb(d.config)}, logs ${mb(d.logs)}, icon cache ${mb(d.iconCache)}, WebView2 data ${mb(d.webviewData)}, total ${mb(d.total)}.`, "");
  }
  if (result.problems?.length) lines.push("**Over budget:**", "", ...result.problems.map((p) => `- ${p}`), "");
  else lines.push("Every number is within its budget (scripts/footprint-budgets.json).", "");
  return lines.join("\n");
}

// ---- Windows-only runner -------------------------------------------------

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const probe = (action, pids = []) =>
  JSON.parse(
    execFileSync("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", PROBE, "-Action", action, "-Pids", pids.join(",")], {
      encoding: "utf8",
      maxBuffer: 64 * MB,
    }) || "[]",
  );
const appTree = (pid) => tree(probe("procs"), pid);

async function waitFor(what, fn, ms = 60_000) {
  const end = Date.now() + ms;
  let last;
  while (Date.now() < end) {
    last = fn();
    if (last) return last;
    await sleep(1000);
  }
  throw new Error(`timed out waiting for ${what}`);
}

const visibleWindows = (pid) => probe("windows", appTree(pid).map((p) => p.pid));

async function session(opts, mode) {
  const log = (...a) => console.log(`[${mode}]`, ...a);
  for (const dir of Object.values(places())) rmSync(dir, { recursive: true, force: true });
  const args = mode === "simulate" ? ["--simulate"] : [];
  const child = spawn(opts.app, args, { detached: true, stdio: "ignore" });
  child.unref();
  const pid = child.pid;
  log("started", pid);
  const memoryAt = {};
  const windows = mode === "simulate" ? 2 : 1;
  await waitFor("the window(s) and WebView2", () => visibleWindows(pid).length >= windows && memory(appTree(pid)).webviewProcesses > 0);
  await sleep(15_000);
  memoryAt.open = memory(appTree(pid));
  log("open", JSON.stringify(memoryAt.open), visibleWindows(pid));

  const closeAll = async () => {
    const closed = probe("close", appTree(pid).map((p) => p.pid));
    log("closed", closed);
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
  memoryAt.tray = memory(after);
  const cpu = { ...cpuPercent(before, after, seconds, os.cpus().length), seconds: Math.round(seconds), cores: os.cpus().length };
  log("tray", JSON.stringify(memoryAt.tray), JSON.stringify(cpu));

  if (mode === "normal") {
    // A second launch opens the settings window again (single instance).
    spawn(opts.app, [], { detached: true, stdio: "ignore" }).unref();
    await waitFor("the settings window to open again", () => visibleWindows(pid).length >= 1 && memory(appTree(pid)).webviewProcesses > 0);
    await sleep(10_000);
    memoryAt.reopened = memory(appTree(pid));
    log("reopened", JSON.stringify(memoryAt.reopened));
    await closeAll();
    await sleep(10_000);
    memoryAt["tray again"] = memory(appTree(pid));
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
    exeName: path.basename(opts.app),
    modes: {},
  };
  result.modes.normal = await session(opts, "normal");
  result.modes.simulate = await session(opts, "simulate");
  const budgets = opts.budgets ? JSON.parse(readFileSync(opts.budgets, "utf8")) : {};
  result.problems = overBudget(result, budgets);
  writeFileSync(path.join(opts.out, "footprint.json"), JSON.stringify(result, null, 2));
  const table = summary(result);
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
