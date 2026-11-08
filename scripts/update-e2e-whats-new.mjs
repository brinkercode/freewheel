#!/usr/bin/env node
// update-e2e (published releases): after the previous release's exe updated
// itself headless to the newest release (no notes stored by the window, as
// after an update from outside the app), the updated exe shows the newest
// release's notes in What's new, fetched from its GitHub release by the
// app itself (src-tauri release_notes.rs) and cleaned and formatted as the
// release's own cut (src/releaseNotes.js appNotes): headings and bullets,
// no scope labels, PR links or hashes.
//
// The shipped exe opens its WebView2 DevTools port only in --simulate
// (FREEWHEEL_E2E_DEVTOOLS_PORT); the release notes come from the real
// releases API there too. This script asks the same API itself for the
// expected notes and compares the page's text with them, line by line.
//
//   node scripts/update-e2e-whats-new.mjs --app <folder>\freewheel-<A>-windows-x64.exe --from 1.0.0 --to 1.1.0 [--port 9224] [--driver http://127.0.0.1:4444] [--out update-e2e-shots]
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { notesText } from "../src/notesMarkdown.js";
import { appNotes } from "../src/releaseNotes.js";
import { DEVTOOLS_PORT_ENV, Driver, FIND_JS, NOTES_ON_PAGE_JS, shotName } from "./e2e.mjs";

/** The releases API URL the app asks (src-tauri/src/release_notes.rs RELEASES_API). */
export const releaseApi = (version) => `https://api.github.com/repos/brinkercode/freewheel/releases/tags/v${version}`;

export function parseArgs(argv) {
  const opts = { app: null, from: null, to: null, port: "9224", driver: "http://127.0.0.1:4444", out: "update-e2e-shots" };
  for (let i = 0; i < argv.length; i++) {
    const name = argv[i].replace(/^--/, "");
    if (!(name in opts)) throw new Error(`unknown option ${argv[i]}`);
    opts[name] = argv[++i];
  }
  for (const need of ["app", "from", "to"]) if (!opts[need]) throw new Error(`--${need} is required`);
  return opts;
}

/** The release's body from GitHub (as the app asks: no token). */
export async function releaseBody(version, fetchImpl = fetch) {
  const res = await fetchImpl(releaseApi(version), {
    headers: { accept: "application/vnd.github+json", "user-agent": "freewheel-update-e2e" },
    signal: AbortSignal.timeout(15000),
  });
  if (!res.ok) throw new Error(`${releaseApi(version)}: HTTP ${res.status}`);
  const { body } = await res.json();
  if (typeof body !== "string" || !body.trim()) throw new Error(`v${version}'s GitHub release has no notes`);
  return body;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  const out = resolve(opts.out);
  const app = resolve(opts.app);
  mkdirSync(out, { recursive: true });
  const log = [];
  const say = (line) => (console.log(line), log.push(line));
  const d = new Driver(opts.driver);
  let child = null;
  let n = 0;
  const shot = async (step) => {
    const file = shotName(++n, "settings", step);
    writeFileSync(join(out, file), Buffer.from(await d.screenshot(), "base64"));
    say(`  screenshot ${file}`);
  };
  const waitFor = async (what, predicate, args = [], ms = 20000) => {
    const end = Date.now() + ms;
    let last;
    while (Date.now() < end) {
      try {
        last = await d.exec(predicate, args);
        if (last) return last;
      } catch (e) {
        last = e.message;
      }
      await sleep(150);
    }
    throw new Error(`waited ${ms} ms for ${what} (last: ${JSON.stringify(last)})`);
  };
  const click = async (css, text = "") => {
    const el = await waitFor(`${css} "${text}"`, FIND_JS, [css, text]);
    await d.exec("arguments[0].scrollIntoView({ block: 'center' })", [el]);
    await d.click(Object.values(el)[0]);
  };
  try {
    const body = await releaseBody(opts.to);
    const expected = notesText(appNotes(body));
    say(`v${opts.to}'s GitHub release notes, as What's new shows them (${expected.length} lines): ${expected.slice(0, 5).join(" | ")}`);
    if (!expected.length) throw new Error("the cleaned notes are empty");

    say(`starting ${app} --simulate (DevTools port ${opts.port})`);
    child = spawn(app, ["--simulate"], { env: { ...process.env, [DEVTOOLS_PORT_ENV]: opts.port }, stdio: "inherit", cwd: dirname(app) });
    const up = Date.now() + 60000;
    for (;;) {
      const pages = await fetch(`http://127.0.0.1:${opts.port}/json/list`, { signal: AbortSignal.timeout(2000) })
        .then((r) => r.json())
        .catch(() => []);
      if (pages.filter((p) => p.type === "page" && p.url && !p.url.startsWith("about:")).length >= 1) break;
      if (child.exitCode !== null) throw new Error(`the app exited with ${child.exitCode}`);
      if (Date.now() > up) throw new Error("no settings page on the DevTools port within 60 s");
      await sleep(300);
    }
    await d.start(app, opts.port);
    const end = Date.now() + 30000;
    for (let found = false; !found; ) {
      for (const h of await d.handles()) {
        await d.switchTo(h);
        if (await d.exec("return document.readyState === 'complete' && location.hash === ''").catch(() => false)) {
          found = true;
          break;
        }
      }
      if (!found && Date.now() > end) throw new Error("no settings window");
      if (!found) await sleep(300);
    }
    const version = await d.execAsync(
      `const done = arguments[arguments.length - 1]; window.__TAURI__.core.invoke("app_version").then(done, (e) => done("error: " + e));`,
    );
    say(`running version: ${version}`);
    if (version !== opts.to) throw new Error(`the updated exe is ${version}, expected ${opts.to}`);

    // As after the update: the window last saw the old version and stored no notes.
    await d.exec(
      `localStorage.setItem("freewheel.first-run-done", "1"); localStorage.setItem("freewheel.last-seen-version", arguments[0]);
       localStorage.removeItem("freewheel.update-notes"); localStorage.removeItem("freewheel.release-notes"); location.reload();`,
      [opts.from],
    );
    await sleep(1000);
    await waitFor("the What's new notice", `return document.body.innerText.includes("freewheel is updated to " + arguments[0])`, [opts.to]);
    await shot(`${opts.to}: what's new notice`);
    await click(".update-banner button", `What’s new in v${opts.to}`);
    // GitHub's unauthenticated API may refuse a shared runner now and then: a
    // failure is not kept, so Done and General's What's new ask again.
    for (let attempt = 1; ; attempt++) {
      try {
        await waitFor(`v${opts.to}'s GitHub release notes, formatted`, NOTES_ON_PAGE_JS, [JSON.stringify(expected)], 30000);
        break;
      } catch (e) {
        if (attempt >= 3) throw e;
        say(`  attempt ${attempt}: ${e.message}; asking again`);
        await sleep(20000);
        await click(".tab-page button", "Done");
        await click("nav.toolbar button.tab", "General");
        await click(".version-card .whats-new-link", `What’s new in v${opts.to}`);
      }
    }
    const shown = await d.exec(`const c = document.querySelector(".whats-new-notes");
      return { headings: [...c.querySelectorAll("h3, h4")].map((e) => e.textContent), bullets: [...c.querySelectorAll("li")].length, text: c.textContent };`);
    say(`  headings ${JSON.stringify(shown.headings)}, ${shown.bullets} bullets`);
    if (!shown.headings.length || shown.bullets < 1) throw new Error(`no heading and bullets: ${JSON.stringify(shown)}`);
    if (/\*\*|\(\[#\d|\/commit\/|[0-9a-f]{7}\)\)/.test(shown.text)) throw new Error(`the notes were not cleaned: ${shown.text}`);
    if (shown.text.includes("The notes for this version are on GitHub.")) throw new Error("What's new shows the placeholder");
    await shot(`${opts.to}: what's new from GitHub`);
    say("update e2e What's new: PASS");
  } catch (e) {
    say(`update e2e What's new: FAIL: ${e.message}`);
    await shot("failure").catch((err) => say(`  no failure screenshot: ${err.message}`));
    process.exitCode = 1;
  } finally {
    writeFileSync(join(out, "whats-new-log.txt"), log.join("\n") + "\n");
    await d.quit();
    child?.kill();
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
