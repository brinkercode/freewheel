#!/usr/bin/env node
// The portable update, end to end, on a hosted Windows runner, without a
// published release (.github/workflows/e2e-windows.yml, job update):
// version A (unzipped from its portable package into a folder, as a user
// does) and version B's package are e2e builds with a throwaway test pubkey
// baked in. This script serves B's package, signature and latest.json
// (scripts/update-server.mjs), starts A from its folder with the updater
// pointed at that server (FREEWHEEL_E2E_UPDATE_URL, e2e builds only) and
// its WebView2 DevTools port open, and drives the settings window over
// WebDriver like a user:
//   General -> Update automatically off -> keys and a page set up (the
//   config to keep) -> Check for updates: checking -> error (server fails)
//   -> Try again: up to date -> (a tampered B published) Check: available
//   -> Download and install: rejected (bad signature), A untouched and still
//   running -> (B published) Try again: downloading with progress -> ready
//   -> Restart to update -> A swaps B in and starts it -> the same path now
//   runs B, What's new in B shows, the config is exactly as before, and
//   freewheel.exe.old and the temp download are gone.
// Every state is screenshotted into the output dir.
//
//   node scripts/e2e-update-windows.mjs --app <folder>\freewheel.exe --manifest <B latest.json> --package <B zip>
//        --from 0.1.0 --to 0.2.0 --server-port 8719 [--port 9223] [--driver http://127.0.0.1:4444] [--out e2e-update-shots]
import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { DEVTOOLS_PORT_ENV, Driver, FIND_JS, shotName } from "./e2e-windows.mjs";
import { startUpdateServer } from "./update-server.mjs";

/** The e2e builds' manifest override (src-tauri/src/selfupdate.rs, `e2e` feature only). */
export const UPDATE_URL_ENV = "FREEWHEEL_E2E_UPDATE_URL";

/** B's release notes, release-please style: What's new must show them. */
export const NOTES = `## [{to}](https://github.com/brinkercode/freewheel/compare/v{from}...v{to}) (e2e)

### Features

* **updates:** this version arrived through the portable updater in CI

### Bug Fixes

* nothing was broken; this is the update e2e
`;

/** The config the update must keep: a shortcut, a page and a key that opens it. */
export function populate(config) {
  const c = structuredClone(config);
  const base = c.layers.find((l) => l.id === c.base_layer);
  base.bindings.In = { Hotkey: { combo: { modifiers: ["Ctrl", "Shift"], key: "K" } } };
  base.bindings.Cut = { LayerPush: { target: "e2e-page" } };
  c.layers = c.layers.filter((l) => l.id !== "e2e-page");
  c.layers.push({ id: "e2e-page", name: "E2E page", bindings: { Out: { Hotkey: { combo: { modifiers: [], key: "J" } } } } });
  return c;
}

export function parseArgs(argv) {
  const opts = {
    app: null,
    manifest: null,
    package: null,
    from: null,
    to: null,
    "server-port": "8719",
    port: "9223",
    driver: "http://127.0.0.1:4444",
    out: "e2e-update-shots",
  };
  for (let i = 0; i < argv.length; i++) {
    const name = argv[i].replace(/^--/, "");
    if (!(name in opts)) throw new Error(`unknown option ${argv[i]}`);
    opts[name] = argv[++i];
  }
  for (const need of ["app", "manifest", "package", "from", "to"]) if (!opts[need]) throw new Error(`--${need} is required`);
  return opts;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const sha = (file) => createHash("sha256").update(readFileSync(file)).digest("hex");

/** Where the app keeps its config on Windows (directories::ProjectDirs config_dir). */
export const configFile = (appData = process.env.APPDATA ?? "") => join(appData, "freewheel", "config", "config.json");
/** The updater's temp dir (selfupdate.rs TEMP_DIR_NAME). */
export const updateTemp = (temp = process.env.TEMP ?? "") => join(temp, "freewheel-update");

/** The path every running freewheel.exe was started from (Windows). */
function runningPaths() {
  try {
    const out = execFileSync("powershell", ["-NoProfile", "-Command", "Get-Process freewheel -ErrorAction SilentlyContinue | ForEach-Object { $_.Path }"], { encoding: "utf8" });
    return out.split(/\r?\n/).map((l) => l.trim()).filter(Boolean);
  } catch {
    return [];
  }
}

/** The exe's file version (what Explorer shows). */
function fileVersion(exe) {
  const cmd = `$v = (Get-Item '${exe.replace(/'/g, "''")}').VersionInfo; "{0}.{1}.{2}" -f $v.FileMajorPart, $v.FileMinorPart, $v.FileBuildPart`;
  return execFileSync("powershell", ["-NoProfile", "-Command", cmd], { encoding: "utf8" }).trim();
}

/** The page targets on the DevTools port (none while nothing listens). */
const pages = (port) =>
  fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(2000) })
    .then((r) => r.json())
    .then((list) => list.filter((t) => t.type === "page"), () => []);

async function waitForPage(port, ms, failed = () => false) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const why = failed();
    if (why) throw new Error(why);
    if ((await pages(port)).some((p) => p.url && !p.url.startsWith("about:"))) return;
    await sleep(300);
  }
  throw new Error(`no settings page on DevTools port ${port} within ${ms / 1000} s`);
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  const out = resolve(opts.out);
  const app = resolve(opts.app);
  const folder = dirname(app);
  mkdirSync(out, { recursive: true });
  const log = [];
  const t0 = Date.now();
  const say = (line) => (console.log(`[${((Date.now() - t0) / 1000).toFixed(1)}s] ${line}`), log.push(line));
  let n = 0;
  let d = new Driver(opts.driver);
  let server = null;
  let child = null;

  const shot = async (step) => {
    const file = shotName(++n, "settings", step);
    writeFileSync(join(out, file), Buffer.from(await d.screenshot(), "base64"));
    say(`  screenshot ${file}`);
  };
  const waitFor = async (what, predicate, args = [], ms = 15000) => {
    const end = Date.now() + ms;
    let last;
    while (Date.now() < end) {
      try {
        last = await d.exec(predicate, args);
        if (last) return last;
      } catch (e) {
        last = e.message;
      }
      await sleep(100);
    }
    throw new Error(`waited ${ms} ms for ${what} (last: ${JSON.stringify(last)})`);
  };
  const click = async (css, text = "") => {
    const el = await waitFor(`${css} "${text}"`, FIND_JS, [css, text]);
    await d.exec("arguments[0].scrollIntoView({ block: 'center' })", [el]);
    await d.click(Object.values(el)[0]);
  };
  const invoke = (command, args = {}) =>
    d.execAsync(
      `const [command, args, done] = arguments;
       window.__TAURI__.core.invoke(command, args).then((v) => done({ ok: v }), (e) => done({ error: String(e) }));`,
      [command, args],
    );
  /** The version card's state (data-update), its line and its button. */
  const card = () =>
    d.exec(`const sub = document.querySelector(".version-card [data-update]");
      const button = document.querySelector(".version-card .update-button");
      const banner = document.querySelector(".update-banner[data-update]");
      return sub && { state: sub.dataset.update, text: sub.textContent, button: button?.textContent, spinning: !!button?.querySelector(".spinner"),
        banner: banner?.dataset.update ?? null, progress: document.querySelector(".version-card [role=progressbar]")?.getAttribute("aria-valuenow") ?? null };`);
  const waitState = (state, ms = 20000) =>
    waitFor(`the update state ${state}`, `return document.querySelector('.version-card [data-update="' + arguments[0] + '"]')`, [state], ms);
  /** Attaches to the app's settings window. */
  const attach = async () => {
    await d.start(app, opts.port);
    const end = Date.now() + 30000;
    for (;;) {
      for (const h of await d.handles()) {
        await d.switchTo(h);
        const ok = await d.exec("return document.readyState === 'complete' && location.hash === ''").catch(() => false);
        if (ok) return;
      }
      if (Date.now() > end) throw new Error("no settings window");
      await sleep(300);
    }
  };
  const folderFiles = () => readdirSync(folder).sort();
  /** Downloads progress until ready; returns the percentages seen. */
  const watchDownload = async () => {
    const seen = new Set();
    let mid = false;
    for (const end = Date.now() + 60000; Date.now() < end; ) {
      const c = await card();
      if (c.state === "downloading" && c.progress != null) {
        seen.add(Number(c.progress));
        if (!mid && Number(c.progress) >= 20 && Number(c.progress) <= 80) {
          mid = true;
          say(`  ${JSON.stringify(c)}`);
          await shot(`downloading ${c.progress}%`);
        }
      }
      if (c.state === "ready" || c.state === "error") return { seen, last: c };
      await sleep(80);
    }
    throw new Error("the download did not finish within 60 s");
  };

  try {
    const manifest = JSON.parse(readFileSync(resolve(opts.manifest), "utf8"));
    manifest.notes = NOTES.replaceAll("{from}", opts.from).replaceAll("{to}", opts.to);
    server = await startUpdateServer({ manifest, package: resolve(opts.package), port: Number(opts["server-port"]), mode: "fail" });
    say(`update server on ${server.url} offering ${opts.to} (${opts.package}), mode fail`);
    const before = { exe: sha(app), files: folderFiles(), version: fileVersion(app) };
    say(`A in ${folder}: ${before.files.join(", ")}; file version ${before.version}`);
    if (before.version !== opts.from) throw new Error(`the exe is ${before.version}, expected ${opts.from}`);

    say(`starting ${app} (version ${opts.from})`);
    const env = { ...process.env, [DEVTOOLS_PORT_ENV]: String(opts.port), [UPDATE_URL_ENV]: `${server.url}/latest.json` };
    child = spawn(app, [], { env, stdio: "inherit", cwd: folder });
    child.on("exit", (code) => say(`A (pid ${child.pid}) exited with ${code}`));
    await waitForPage(opts.port, 60000, () => child.exitCode !== null && `A exited with ${child.exitCode}`);
    await attach();
    const a = await invoke("app_version");
    say(`running version: ${JSON.stringify(a)}`);
    if (a.ok !== opts.from) throw new Error(`expected A to be ${opts.from}, got ${JSON.stringify(a)}`);

    // Past the first run (not what this test is about), then the General tab.
    await d.exec(`localStorage.setItem("freewheel.first-run-done", "1"); location.reload();`);
    await sleep(1000);
    await waitFor("the settings tabs", `return document.readyState === "complete" && document.querySelector("nav.toolbar")`);
    await click("nav.toolbar button.tab", "General");
    await waitState("idle");
    await shot(`A ${opts.from}: General, idle`);
    const auto = await waitFor("Update automatically", `return document.querySelector('[aria-label="Update automatically"]')?.getAttribute("aria-checked")`);
    if (auto === "true") await click('[aria-label="Update automatically"]');
    await waitFor("Update automatically off", `return document.querySelector('[aria-label="Update automatically"]')?.getAttribute("aria-checked") === "false"`);
    await shot("update automatically off");

    say("step: keys and a page, the config the update must keep");
    const loaded = await invoke("get_config");
    if (loaded.error) throw new Error(`get_config: ${loaded.error}`);
    const saved = await invoke("save_config", { config: populate(loaded.ok) });
    if (saved.error) throw new Error(`save_config: ${saved.error}`);
    const config = (await invoke("get_config")).ok;
    if (!config.layers.some((l) => l.id === "e2e-page")) throw new Error("the page was not saved");
    await sleep(500);
    const configHash = sha(configFile());
    say(`  config ${configFile()} sha256 ${configHash}, ${config.layers.length} pages`);

    say("step: Check for updates, the server fails");
    await click(".version-card .update-button", "Check for updates");
    await waitState("checking", 3000);
    if (!(await card()).spinning) throw new Error("no spinner while checking");
    await shot("checking");
    await waitState("error");
    say(`  ${JSON.stringify(await card())}`);
    await shot("check failed, try again");

    say("step: Try again, nothing newer");
    server.setMode("none");
    await click(".version-card .update-button", "Try again");
    await waitState("checking", 3000);
    await waitState("current");
    await shot("up to date");

    say(`step: a tampered ${opts.to} published, Check for updates, Download`);
    server.setMode("tampered");
    await click(".version-card .update-button", "Check for updates");
    await waitState("available");
    const available = await card();
    say(`  ${JSON.stringify(available)}`);
    if (!available.text.includes(opts.to)) throw new Error(`available does not name ${opts.to}: ${available.text}`);
    if (available.banner !== "available") throw new Error("no update banner");
    await shot(`${opts.to} available (tampered)`);
    await click(".version-card .update-button", "Download and install");
    const tampered = await watchDownload();
    say(`  ${JSON.stringify(tampered.last)}`);
    if (tampered.last.state !== "error") throw new Error(`the tampered package was not rejected: ${JSON.stringify(tampered.last)}`);
    if (!/couldn’t be verified/.test(tampered.last.text)) throw new Error(`the error does not say it failed verification: ${tampered.last.text}`);
    await shot("tampered package rejected");
    if (sha(app) !== before.exe) throw new Error("the tampered package changed freewheel.exe");
    if (folderFiles().join() !== before.files.join()) throw new Error(`the folder changed: ${folderFiles().join(", ")}`);
    if (existsSync(updateTemp())) throw new Error("the rejected download was kept");
    const still = await invoke("app_version");
    if (still.ok !== opts.from || child.exitCode !== null) throw new Error(`A is not running as before: ${JSON.stringify(still)}`);
    say("  A intact: same exe, same folder, no download kept, still running");

    say(`step: ${opts.to} published, Try again: downloading with progress`);
    server.setMode("offer");
    await click(".version-card .update-button", "Try again");
    await waitState("downloading", 5000);
    const good = await watchDownload();
    say(`  progress seen: ${[...good.seen].sort((x, y) => x - y).join(", ")}`);
    if (good.last.state !== "ready") throw new Error(`the download failed: ${good.last.text}`);
    if (good.seen.size < 3) throw new Error(`the download showed too little progress: ${[...good.seen]}`);
    await shot("ready, restart to update");

    say("step: Restart to update");
    await click(".update-banner button", "Restart to update");
    await shot("restarting").catch((e) => say(`  no restarting screenshot (A already exited): ${e.message}`));
    const exited = Date.now() + 30000;
    while (child.exitCode === null && Date.now() < exited) await sleep(200);
    if (child.exitCode === null) throw new Error("A did not exit for the update");
    await d.quit();
    say(`  server saw: ${server.hits.join(" | ")}`);
    say(`  folder after the swap: ${folderFiles().join(", ")}`);
    d = new Driver(opts.driver);
    await waitForPage(opts.port, 60000);
    await attach();
    const b = await invoke("app_version");
    say(`running version after the restart: ${JSON.stringify(b)}`);
    if (b.ok !== opts.to) throw new Error(`expected ${opts.to} after the update, got ${JSON.stringify(b)}`);
    const paths = runningPaths();
    say(`  running from: ${paths.join(", ")}`);
    if (!paths.length || !paths.every((p) => p.toLowerCase() === app.toLowerCase())) throw new Error(`B does not run from ${app}: ${paths}`);
    const after = fileVersion(app);
    if (after !== opts.to) throw new Error(`${app} is ${after}, expected ${opts.to}`);

    say("step: What's new");
    await waitFor("the What's new notice", `return document.body.innerText.includes("freewheel is updated to " + arguments[0])`, [opts.to], 20000);
    await shot(`B ${opts.to}: what's new notice`);
    await click(".update-banner button", "What’s new in v");
    await waitFor("the notes page", `return document.body.innerText.includes("this version arrived through the portable updater in CI")`);
    await shot(`B ${opts.to}: what's new page`);

    say("step: settings kept, nothing left behind");
    const kept = (await invoke("get_config")).ok;
    if (JSON.stringify(kept) !== JSON.stringify(config)) throw new Error(`the config changed:\n${JSON.stringify(config)}\n${JSON.stringify(kept)}`);
    if (sha(configFile()) !== configHash) throw new Error("config.json changed on disk");
    const clean = Date.now() + 20000;
    while ((existsSync(`${app}.old`) || existsSync(updateTemp())) && Date.now() < clean) await sleep(250);
    if (existsSync(`${app}.old`)) throw new Error("freewheel.exe.old is still there");
    if (existsSync(updateTemp())) throw new Error(`${updateTemp()} is still there`);
    if (folderFiles().join() !== before.files.join()) throw new Error(`the folder is not as before: ${folderFiles().join(", ")}`);
    say(`  config identical (sha256 ${configHash}); folder: ${folderFiles().join(", ")}; no .old, no temp download`);
    say("update e2e: PASS");
  } catch (e) {
    say(`update e2e: FAIL: ${e.message}`);
    await shot("failure").catch((err) => say(`  no failure screenshot: ${err.message}`));
    if (server) say(`  server saw: ${server.hits.join(" | ")}`);
    say(`  folder: ${existsSync(folder) ? folderFiles().join(", ") : "(gone)"}`);
    process.exitCode = 1;
  } finally {
    writeFileSync(join(out, "e2e-update-log.txt"), log.join("\n") + "\n");
    await d.quit();
    child?.kill();
    await server?.close();
    rmSync(join(process.env.LOCALAPPDATA ?? "", "freewheel-e2e"), { recursive: true, force: true });
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
