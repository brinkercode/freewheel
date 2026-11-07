#!/usr/bin/env node
// The real update, end to end, on a hosted Windows runner, without a
// published release (.github/workflows/e2e-windows.yml, job update-e2e):
// version A is installed (built with a throwaway test pubkey and its updater
// endpoint on this machine, e2e builds only); this script serves version B's
// signed NSIS bundle and latest.json (scripts/update-server.mjs), starts the
// installed A with its WebView2 DevTools port open (the `e2e` feature) and
// drives the settings window over WebDriver like a user:
//   General -> Update automatically off -> Check for updates: checking ->
//   error (server fails) -> Try again: up to date -> (B published) Check:
//   available -> Download and install: downloading with progress -> ready ->
//   Restart to update -> the updater's NSIS installer replaces A and starts
//   B -> B's settings window shows version B and What's new in B.
// Every state is screenshotted into the output dir.
//
//   node scripts/e2e-update-windows.mjs --app <installed freewheel.exe> --bundle <B setup.exe> --sig <B setup.exe.sig>
//        --from 0.1.0 --to 0.2.0 --server-port 8719 [--port 9223] [--driver http://127.0.0.1:4444] [--out e2e-update-shots]
import { spawn } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { DEVTOOLS_PORT_ENV, Driver, FIND_JS, shotName } from "./e2e-windows.mjs";
import { startUpdateServer } from "./update-server.mjs";

/** B's release notes, release-please style: What's new must show them. */
export const NOTES = `## [{to}](https://github.com/brinkercode/freewheel/compare/v{from}...v{to}) (e2e)

### Features

* **updates:** this version arrived through the real updater in CI

### Bug Fixes

* nothing was broken; this is the update e2e
`;

export function parseArgs(argv) {
  const opts = {
    app: null,
    bundle: null,
    sig: null,
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
  for (const need of ["app", "bundle", "sig", "from", "to"]) if (!opts[need]) throw new Error(`--${need} is required`);
  return opts;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** The page targets on the DevTools port (none while nothing listens). */
const pages = (port) =>
  fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(2000) })
    .then((r) => r.json())
    .then((list) => list.filter((t) => t.type === "page"), () => []);

/** Starts the installed app (no --simulate) with its DevTools port open; resolves once the settings page exists. */
async function launch(app, port, say) {
  const child = spawn(app, [], { env: { ...process.env, [DEVTOOLS_PORT_ENV]: String(port) }, stdio: "inherit" });
  child.on("exit", (code) => say(`app (pid ${child.pid}) exited with ${code}`));
  await waitForPage(port, 60000, () => child.exitCode !== null && `the app exited with ${child.exitCode}`);
  return child;
}

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
  mkdirSync(out, { recursive: true });
  const log = [];
  const t0 = Date.now();
  const say = (line) => (console.log(`[${((Date.now() - t0) / 1000).toFixed(1)}s] ${line}`), log.push(line));
  let n = 0;
  let d = new Driver(opts.driver);
  let server = null;
  let app = null;

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
    await d.start(resolve(opts.app), opts.port);
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

  try {
    const signature = readFileSync(resolve(opts.sig), "utf8");
    const notes = NOTES.replaceAll("{from}", opts.from).replaceAll("{to}", opts.to);
    server = await startUpdateServer({ bundle: resolve(opts.bundle), signature, version: opts.to, notes, port: Number(opts["server-port"]), mode: "fail" });
    say(`update server on ${server.url} offering ${opts.to} (${opts.bundle}), mode fail`);

    say(`starting ${opts.app} (version ${opts.from})`);
    app = await launch(resolve(opts.app), opts.port, say);
    await attach();
    const a = await invoke("app_version");
    say(`running version: ${JSON.stringify(a)}`);
    if (a.ok !== opts.from) throw new Error(`expected the installed app to be ${opts.from}, got ${JSON.stringify(a)}`);

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

    say("step: Check for updates, the server fails");
    await click(".version-card .update-button", "Check for updates");
    await waitState("checking", 3000);
    const checking = await card();
    say(`  ${JSON.stringify(checking)}`);
    if (!checking.spinning) throw new Error("no spinner while checking");
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

    say(`step: ${opts.to} published, Check for updates`);
    server.setMode("offer");
    await click(".version-card .update-button", "Check for updates");
    await waitState("checking", 3000);
    await waitState("available");
    const available = await card();
    say(`  ${JSON.stringify(available)}`);
    if (!available.text.includes(opts.to)) throw new Error(`available does not name ${opts.to}: ${available.text}`);
    if (available.banner !== "available") throw new Error("no update banner");
    await shot(`${opts.to} available`);

    say("step: Download and install");
    await click(".version-card .update-button", "Download and install");
    await waitState("downloading", 5000);
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
      if (c.state === "ready") break;
      if (c.state === "error") throw new Error(`the download failed: ${c.text}`);
      await sleep(80);
    }
    say(`  progress seen: ${[...seen].sort((x, y) => x - y).join(", ")}`);
    if (seen.size < 3) throw new Error(`the download showed too little progress: ${[...seen]}`);
    await waitState("ready", 5000);
    await shot("ready, restart to update");

    say("step: Restart to update");
    await click(".update-banner button", "Restart to update");
    await shot("restarting").catch((e) => say(`  no restarting screenshot (the app already exited): ${e.message}`));
    // The updater starts the NSIS installer and exits A; the installer replaces it and starts B.
    const exited = Date.now() + 60000;
    while (app.exitCode === null && Date.now() < exited) await sleep(300);
    if (app.exitCode === null) throw new Error("A did not exit for the update");
    await d.quit();
    say(`  server saw: ${server.hits.join(" | ")}`);
    say("waiting for the installer to start the new version");
    d = new Driver(opts.driver);
    await waitForPage(opts.port, 180000);
    await attach();
    const b = await invoke("app_version");
    say(`running version after the restart: ${JSON.stringify(b)}`);
    if (b.ok !== opts.to) throw new Error(`expected ${opts.to} after the update, got ${JSON.stringify(b)}`);

    say("step: What's new");
    await waitFor("the What's new notice", `return document.body.innerText.includes("freewheel is updated to " + arguments[0])`, [opts.to], 20000);
    await shot(`B ${opts.to}: what's new notice`);
    await click(".update-banner button", "What’s new in v");
    await waitFor("the notes page", `return document.body.innerText.includes("this version arrived through the real updater in CI")`);
    await shot(`B ${opts.to}: what's new page`);
    say("update e2e: PASS");
  } catch (e) {
    say(`update e2e: FAIL: ${e.message}`);
    await shot("failure").catch((err) => say(`  no failure screenshot: ${err.message}`));
    if (server) say(`  server saw: ${server.hits.join(" | ")}`);
    process.exitCode = 1;
  } finally {
    writeFileSync(join(out, "e2e-update-log.txt"), log.join("\n") + "\n");
    await d.quit();
    app?.kill();
    await server?.close();
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
