#!/usr/bin/env node
// Clicks through the real `freewheel.exe --simulate` on Windows over
// WebDriver (tauri-driver in front of msedgedriver, .github/workflows/
// e2e-windows.yml): the first run, every settings tab, the key panel, and the
// Simulator window's controls. After every action both windows must answer a
// round trip through the app (a Tauri command, so a blocked main thread fails
// it) within 5 s. Every step is screenshotted into the output dir.
//
//   node scripts/e2e-windows.mjs --app target/release/freewheel.exe --out e2e-shots [--driver http://127.0.0.1:4444] [--port 9222]
// With --port (the CI default), the script starts the app itself with
// FREEWHEEL_E2E_DEVTOOLS_PORT set, so its WebView2 opens that DevTools port,
// and msedgedriver attaches to it (debuggerAddress): on hosted windows-2025
// runners WebView2 ignores the port msedgedriver would pass when it launches
// the app (tauri-driver's way, actions/runner-images#14738). Without --port,
// the session goes through tauri-driver (`tauri:options`).
// Plain fetch and the W3C WebDriver protocol: no npm dependencies.
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** The env var that opens the app's DevTools port (src-tauri/src/simulate.rs). */
export const DEVTOOLS_PORT_ENV = "FREEWHEEL_E2E_DEVTOOLS_PORT";

/** How long a window may take to answer after an action. */
export const RESPONSIVE_MS = 5000;

export function parseArgs(argv) {
  const opts = { driver: "http://127.0.0.1:4444", app: null, out: "e2e-shots", port: null };
  for (let i = 0; i < argv.length; i++) {
    const name = argv[i].replace(/^--/, "");
    if (!(name in opts)) throw new Error(`unknown option ${argv[i]}`);
    opts[name] = argv[++i];
  }
  if (!opts.app) throw new Error("--app <path to freewheel.exe> is required");
  return opts;
}

/**
 * The new-session body: msedgedriver attached to the running app's
 * WebView2 on `port`, or (no port) tauri-driver starting the app with
 * --simulate.
 */
export function sessionBody(app, port = null) {
  const options = port
    ? { browserName: "webview2", "ms:edgeOptions": { debuggerAddress: `127.0.0.1:${port}` } }
    : { "tauri:options": { application: app, args: ["--simulate"] } };
  return { capabilities: { alwaysMatch: options } };
}

/** Starts `app --simulate` with its DevTools port open; resolves once the port answers. */
async function launch(app, port, say) {
  const child = spawn(app, ["--simulate"], { env: { ...process.env, [DEVTOOLS_PORT_ENV]: String(port) }, stdio: "inherit" });
  child.on("exit", (code) => say(`app exited with ${code}`));
  const end = Date.now() + 60000;
  while (Date.now() < end) {
    if (child.exitCode !== null) throw new Error(`the app exited with ${child.exitCode} before opening port ${port}`);
    // Attach only once both windows' pages exist: msedgedriver attached to
    // a WebView2 still creating them fails with "chrome not reachable".
    const pages = await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(2000) })
      .then((r) => r.json())
      .then((list) => list.filter((t) => t.type === "page").length, () => 0);
    if (pages >= 2) return child;
    await new Promise((r) => setTimeout(r, 300));
  }
  child.kill();
  throw new Error(`the app's two windows did not show up on DevTools port ${port} within 60 s`);
}

/** A safe, ordered file name for a step's screenshot. */
export const shotName = (n, window, step) =>
  `${String(n).padStart(2, "0")}-${window}-${step.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "")}.png`;

class Driver {
  constructor(base) {
    this.base = base;
    this.session = null;
  }
  async call(method, path, body, ms = 30000) {
    const url = `${this.base}${this.session ? `/session/${this.session}` : ""}${path}`;
    let res;
    try {
      res = await fetch(url, {
        method,
        headers: { "content-type": "application/json" },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(ms),
      });
    } catch (e) {
      throw new Error(`${method} ${path}: no answer within ${ms} ms (${e.name})`);
    }
    const json = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(`${method} ${path}: ${res.status} ${json.value?.error ?? ""} ${json.value?.message?.split("\n")[0] ?? ""}`);
    return json.value;
  }
  async start(app, port) {
    let v;
    for (let attempt = 1; attempt <= 3 && !v?.value?.sessionId; attempt++) {
      if (attempt > 1) await new Promise((r) => setTimeout(r, 2000));
      v = await fetch(`${this.base}/session`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(sessionBody(app, port)),
        signal: AbortSignal.timeout(120000),
      }).then((r) => r.json());
    }
    if (!v.value?.sessionId) throw new Error(`no session: ${JSON.stringify(v).slice(0, 500)}`);
    this.session = v.value.sessionId;
    await this.call("POST", "/timeouts", { script: RESPONSIVE_MS, pageLoad: 30000, implicit: 0 });
  }
  quit() {
    return this.session ? this.call("DELETE", "", undefined, 15000).catch(() => {}) : Promise.resolve();
  }
  exec(script, args = [], ms = RESPONSIVE_MS + 2000) {
    return this.call("POST", "/execute/sync", { script, args }, ms);
  }
  execAsync(script, args = [], ms = RESPONSIVE_MS + 2000) {
    return this.call("POST", "/execute/async", { script, args }, ms);
  }
  handles() {
    return this.call("GET", "/window/handles");
  }
  switchTo(handle) {
    return this.call("POST", "/window", { handle });
  }
  async find(css) {
    const v = await this.call("POST", "/elements", { using: "css selector", value: css });
    return v.map((e) => Object.values(e)[0]);
  }
  click(id) {
    return this.call("POST", `/element/${id}/click`, {});
  }
  type(id, text) {
    return this.call("POST", `/element/${id}/value`, { text });
  }
  screenshot() {
    return this.call("GET", "/screenshot", undefined, 30000);
  }
}

// In-page helper: the first element matching `css` whose text contains `text`.
const FIND_JS = `const [css, text] = arguments;
  return [...document.querySelectorAll(css)].find((e) => !text || e.textContent.replace(/\\s+/g, " ").includes(text)) ?? null;`;

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  const out = resolve(opts.out);
  mkdirSync(out, { recursive: true });
  const d = new Driver(opts.driver);
  const log = [];
  let n = 0;
  let current = null;
  const win = {};
  let app = null;
  const t0 = Date.now();
  const say = (line) => (console.log(`[${((Date.now() - t0) / 1000).toFixed(1)}s] ${line}`), log.push(line));

  const use = async (name) => {
    if (current !== name) await d.switchTo(win[name]);
    current = name;
  };
  const shot = async (name, step) => {
    await use(name);
    const file = shotName(++n, name, step);
    writeFileSync(join(out, file), Buffer.from(await d.screenshot(), "base64"));
    say(`  screenshot ${file}`);
  };
  /** Polls `predicate` (page JS returning truthy) in window `name`. */
  const waitFor = async (name, what, predicate, args = [], ms = 10000) => {
    await use(name);
    const end = Date.now() + ms;
    let last;
    while (Date.now() < end) {
      try {
        last = await d.exec(predicate, args);
        if (last) return last;
      } catch (e) {
        last = e.message;
      }
      await new Promise((r) => setTimeout(r, 150));
    }
    throw new Error(`${name}: waited ${ms} ms for ${what} (last: ${JSON.stringify(last)})`);
  };
  /** Clicks the element matching css (+ text) in window `name`, with a real WebDriver click. */
  const click = async (name, css, text = "") => {
    const el = await waitFor(name, `${css} "${text}"`, FIND_JS, [css, text]);
    const id = Object.values(el)[0];
    await d.click(id);
    return id;
  };
  /** Both windows answer a round trip through the app's main thread within RESPONSIVE_MS. */
  const responsive = async (after) => {
    for (const [name, command, args] of [
      ["settings", "get_status", {}],
      ["simulator", "simulate", { control: { type: "Status" } }],
    ]) {
      await use(name);
      const t = Date.now();
      let r;
      try {
        r = await d.execAsync(
          `const [command, args, done] = arguments;
           if (document.readyState !== "complete") return done("not loaded: " + document.readyState);
           window.__TAURI__.core.invoke(command, args).then(() => done("ok"), (e) => done("error: " + e));`,
          [command, args],
        );
      } catch (e) {
        r = e.message;
      }
      const ms = Date.now() - t;
      if (r !== "ok" || ms > RESPONSIVE_MS)
        throw new Error(`after "${after}": the ${name} window did not answer ${command} within ${RESPONSIVE_MS} ms (${ms} ms: ${r})`);
    }
  };
  /** One step: the action, then the responsiveness check, then the screenshots. */
  const step = async (title, action, shots = ["settings"]) => {
    say(`step: ${title}`);
    await action();
    await responsive(title);
    for (const s of shots) await shot(s, title);
  };
  const tab = (t, heading) =>
    step(`${t} tab`, async () => {
      await click("settings", "nav.toolbar button.tab", t);
      await waitFor("settings", `the ${t} tab`, `const [t, h] = arguments;
        const on = document.querySelector('nav.toolbar button.tab[aria-pressed="true"]');
        return on?.textContent.includes(t) && (!h || document.body.innerText.includes(h));`, [t, heading]);
    });
  const sim = (group, option) =>
    step(`simulator ${group}: ${option}`, async () => {
      await click("simulator", `[role=group][aria-label="${group}"] button`, option);
      await waitFor("simulator", `${group} ${option} current`, `const [g, o] = arguments;
        return [...document.querySelectorAll('[role=group][aria-label="' + g + '"] button')].some((b) => b.textContent.includes(o) && b.getAttribute("aria-pressed") === "true");`, [group, option]);
    }, ["simulator", "settings"]);

  try {
    say(`starting ${opts.app} --simulate`);
    if (opts.port) app = await launch(resolve(opts.app), opts.port, say);
    await d.start(resolve(opts.app), opts.port);
    // Both windows: the settings window ("main") and the Simulator (#simulator).
    const end = Date.now() + 30000;
    while (!(win.settings && win.simulator)) {
      if (Date.now() > end) throw new Error(`expected the settings and Simulator windows, got ${JSON.stringify(win)}`);
      for (const h of await d.handles()) {
        await d.switchTo(h);
        current = null;
        const hash = await d.exec("return document.readyState === 'complete' ? location.hash : null").catch(() => null);
        if (hash === "#simulator") win.simulator = h;
        else if (hash === "") win.settings = h;
      }
      if (!(win.settings && win.simulator)) await new Promise((r) => setTimeout(r, 300));
    }
    say(`windows: ${JSON.stringify(win)}`);

    await step("first run: welcome", () => waitFor("settings", "the welcome screen", `return document.querySelector("button.get-started")`), ["settings", "simulator"]);
    await step("first run: get started", async () => {
      await click("settings", "button.get-started");
      await waitFor("settings", "the plug in step", `return document.body.innerText.includes("Plug in or pair your Speed Editor")`);
    });
    await sim("Speed Editor", "USB");
    await step("first run: found it", () => waitFor("settings", "Found it!", `return document.querySelector(".found")`));
    await step("first run: continue", async () => {
      await click("settings", ".first-run-buttons button.btn-primary", "Continue");
      await waitFor("settings", "the next screen", `return document.querySelector("nav.toolbar") || [...document.querySelectorAll("button")].some((b) => b.textContent.includes("Start using freewheel"))`);
    });
    const perms = await d.exec(`return [...document.querySelectorAll("button")].some((b) => b.textContent.includes("Start using freewheel"))`);
    if (perms)
      await step("first run: permissions", async () => {
        await click("settings", "button", "Start using freewheel");
        await waitFor("settings", "the settings tabs", `return document.querySelector("nav.toolbar")`);
      });

    await tab("Keys", "What each key does");
    await step("Keys: click CUT in the drawing", async () => {
      await click("settings", '.device [data-binding="Cut"]');
      await waitFor("settings", "the key panel", `return document.querySelector('.side-panel [role=radiogroup][aria-label="Choices"]')`);
    });
    await step("Keys: pick Open an app or website", async () => {
      await click("settings", ".side-panel label.choice", "Open an app or website");
      await waitFor("settings", "the website field", `return document.querySelector('.side-panel input[aria-label="Website"]')`);
    });
    await step("Keys: use example.com, saved", async () => {
      const [field] = await d.find('.side-panel input[aria-label="Website"]');
      await d.type(field, "example.com");
      await click("settings", ".side-panel .input-row:has(input[aria-label=Website]) button", "Use");
      await waitFor("settings", "Saved", `return document.querySelector(".panel-try.is-saved")`);
      const saved = await d.execAsync(`const done = arguments[0];
        window.__TAURI__.core.invoke("get_config").then((c) => done(JSON.stringify(c.layers.find((l) => l.id === c.base_layer).bindings.Cut ?? null)), (e) => done("error " + e));`);
      if (!saved.includes("example.com")) throw new Error(`CUT not saved in the config: ${saved}`);
      say(`  CUT saved: ${saved}`);
    });
    await tab("Wheel", "When I turn the wheel");
    await tab("DaVinci Resolve");
    await tab("General", "Appearance");
    for (const look of ["Dark", "Light"])
      await step(`General: appearance ${look}`, async () => {
        await click("settings", "button.appearance-option", look);
        await waitFor("settings", `appearance ${look}`, `return document.documentElement.dataset.appearance === arguments[0]`, [look]);
      });
    await tab("Help & backup", "Help");

    await tab("Keys", "What each key does");
    for (const key of ["CUT", "IN", "ESC"])
      await step(`simulator: press ${key}`, async () => {
        await click("simulator", ".fw-sim-quick button", key);
        await waitFor("simulator", `the log line for ${key}`, `return document.querySelector(".fw-sim-log").textContent.includes("You pressed " + arguments[0])`, [key]);
      }, ["simulator", "settings"]);
    for (const [dir, text] of [["left", "Turn wheel left"], ["right", "Turn right"]])
      await step(`simulator: turn wheel ${dir}`, async () => {
        await click("simulator", ".fw-sim-line button", text);
        await waitFor("simulator", `turned ${dir}`, `return document.querySelector(".fw-sim-log").textContent.includes("You turned the wheel " + arguments[0])`, [dir]);
      }, ["simulator", "settings"]);
    await sim("Speed Editor", "Bluetooth 64%");
    await sim("Speed Editor", "Not found");
    await sim("Speed Editor", "USB");
    await sim("DaVinci Resolve", "Open");
    await sim("DaVinci Resolve", "Closed");
    await sim("freewheel", "Off");
    await sim("freewheel", "On");
    await step("settings still clickable after the simulator", async () => {
      await click("settings", "nav.toolbar button.tab", "Wheel");
      await waitFor("settings", "the Wheel tab", `return document.body.innerText.includes("When I turn the wheel")`);
    });
    say("e2e: PASS");
    writeFileSync(join(out, "e2e-log.txt"), log.join("\n") + "\n");
  } catch (e) {
    say(`e2e: FAIL: ${e.message}`);
    for (const name of ["settings", "simulator"])
      if (win[name]) await shot(name, "failure").catch((err) => say(`  no failure screenshot of ${name}: ${err.message}`));
    writeFileSync(join(out, "e2e-log.txt"), log.join("\n") + "\n");
    process.exitCode = 1;
  } finally {
    await d.quit();
    app?.kill();
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
