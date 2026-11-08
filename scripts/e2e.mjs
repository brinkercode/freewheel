#!/usr/bin/env node
// Clicks through the real freewheel `--simulate` over WebDriver on Windows
// (.github/workflows/e2e-windows.yml) and Linux (e2e-linux.yml): the first
// run, every settings tab, the key panel, and the Simulator window's
// controls. After every action both windows must answer a round trip
// through the app (a Tauri command, so a blocked main thread fails it)
// within 5 s. Every step is screenshotted into the output dir. One flow;
// what differs per OS (how the app starts and is attached to, what an
// installed app and a chosen file are, Resolve's process name) is in
// PLATFORMS below. Entry points: scripts/e2e-windows.mjs, scripts/e2e-linux.mjs.
//
// Windows: node scripts/e2e-windows.mjs --app target/release/freewheel.exe --out e2e-shots [--driver http://127.0.0.1:4444] [--port 9222]
//   With --port (the CI default), the script starts the app itself with
//   FREEWHEEL_E2E_DEVTOOLS_PORT set, so its WebView2 opens that DevTools port,
//   and msedgedriver attaches to it (debuggerAddress): on hosted windows-2025
//   runners WebView2 ignores the port msedgedriver would pass when it launches
//   the app (tauri-driver's way, actions/runner-images#14738). Without --port,
//   the session goes through tauri-driver (`tauri:options`).
import { createServer } from "node:http";
// Linux: node scripts/e2e-linux.mjs --app target/release/freewheel --out e2e-shots --tauri-driver <path> [--native-driver /usr/bin/WebKitWebDriver]
//   The script starts tauri-driver (in front of WebKitGTK's WebKitWebDriver)
//   with the dialog stubs' env; WebKitWebDriver starts the app (`tauri:options`).
// Plain fetch and the W3C WebDriver protocol: no npm dependencies.
import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { notesText } from "../src/notesMarkdown.js";

/** The app's log file on Windows (src-tauri logging: the data dir's logs). */
export const appLog = (localAppData = process.env.LOCALAPPDATA ?? "") => join(localAppData, "freewheel", "data", "logs", "freewheel.log");

/** The app's log file on Linux: $XDG_DATA_HOME (else ~/.local/share)/freewheel/logs. */
export const linuxAppLog = (home = homedir(), dataHome = process.env.XDG_DATA_HOME) => join(dataHome || join(home, ".local", "share"), "freewheel", "logs", "freewheel.log");

/** The log lines that tell the update flow, the settings page's readiness, the tray and freezes apart: the last `n`. */
export function flowLines(text, n = 60) {
  const keep = /update|settings window|tray|main thread|simulator log line=Tray|panicked|ERROR|WARN/;
  return text.split(/\r?\n/).filter((l) => keep.test(l)).slice(-n);
}

/** The settings page's trace (src/trace.js), one line per entry. */
export function traceLines(trace) {
  return (Array.isArray(trace) ? trace : []).map(({ at, what, ...rest }) => `${at} ${what} ${JSON.stringify(rest)}`);
}

/** The env var that opens the app's DevTools port (src-tauri/src/simulate.rs). */
export const DEVTOOLS_PORT_ENV = "FREEWHEEL_E2E_DEVTOOLS_PORT";

/**
 * The folder the e2e build's stubbed OS dialogs answer with files in
 * (src-tauri/src/e2e.rs, built only with `--features e2e`): Back up saves
 * there, Restore reads that file back, 'Choose file…' picks a fixed exe.
 */
export const DIALOG_DIR_ENV = "FREEWHEEL_E2E_DIALOG_DIR";

/** How long a window may take to answer after an action. */
export const RESPONSIVE_MS = 5000;

export function parseArgs(argv) {
  const opts = { driver: "http://127.0.0.1:4444", app: null, out: "e2e-shots", port: null, "tauri-driver": null, "native-driver": null, "next-notes": null };
  for (let i = 0; i < argv.length; i++) {
    const name = argv[i].replace(/^--/, "");
    if (!(name in opts)) throw new Error(`unknown option ${argv[i]}`);
    opts[name] = argv[++i];
  }
  if (!opts.app) throw new Error("--app <path to freewheel> is required");
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

/** Windows: starts `app --simulate` with its DevTools port open; resolves once the port answers. */
async function launch(app, port, say, dialogs) {
  const env = { ...process.env, [DEVTOOLS_PORT_ENV]: String(port), [DIALOG_DIR_ENV]: dialogs };
  const child = spawn(app, ["--simulate"], { env, stdio: "inherit" });
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

/** Linux: true while a process named `name` runs (`ps -C`, exact name). */
export function running(name) {
  try {
    return execFileSync("ps", ["-C", name, "-o", "pid="], { encoding: "utf8" }).trim().length > 0;
  } catch {
    return false;
  }
}

/** Linux: starts tauri-driver on `port` with `env` (WebKitWebDriver and the app it starts inherit it); resolves once it answers. */
async function startTauriDriver(bin, nativeDriver, port, env, say) {
  const args = ["--port", String(port), ...(nativeDriver ? ["--native-driver", nativeDriver] : [])];
  const child = spawn(bin, args, { env, stdio: "inherit" });
  child.on("exit", (code) => say(`tauri-driver exited with ${code}`));
  const end = Date.now() + 30000;
  while (Date.now() < end) {
    if (child.exitCode !== null) throw new Error(`tauri-driver exited with ${child.exitCode}`);
    const up = await fetch(`http://127.0.0.1:${port}/status`, { signal: AbortSignal.timeout(2000) }).then((r) => r.ok, () => false);
    if (up) return child;
    await new Promise((r) => setTimeout(r, 300));
  }
  child.kill();
  throw new Error(`tauri-driver did not answer on port ${port} within 30 s`);
}

/** The Linux e2e's installed app: a desktop entry with a themed icon in the user's data dirs. */
export const LINUX_APP = { id: "freewheel-e2e-example", name: "E2E Example Editor" };

/** Writes the Linux e2e's desktop entry and its hicolor icon in the user's data dir ($XDG_DATA_HOME, else ~/.local/share). */
export function installLinuxApp(home, icon, dataHome = process.env.XDG_DATA_HOME) {
  const data = dataHome || join(home, ".local", "share");
  mkdirSync(join(data, "applications"), { recursive: true });
  mkdirSync(join(data, "icons", "hicolor", "32x32", "apps"), { recursive: true });
  copyFileSync(icon, join(data, "icons", "hicolor", "32x32", "apps", `${LINUX_APP.id}.png`));
  const desktop = join(data, "applications", `${LINUX_APP.id}.desktop`);
  writeFileSync(desktop, `[Desktop Entry]\nType=Application\nName=${LINUX_APP.name}\nExec=/usr/bin/true %F\nIcon=${LINUX_APP.id}\n`);
  return desktop;
}

/**
 * What differs per OS. `start(opts, say, dialogs)` gets the app running and
 * attachable (returns a handle with `quit()`), `session(app, port)` is the
 * new-session body, `exited()` tells when the app is gone after the tray's Quit.
 */
export const PLATFORMS = {
  windows: {
    name: "Windows",
    async start(opts, say, dialogs) {
      const app = opts.port ? await launch(resolve(opts.app), opts.port, say, dialogs) : null;
      return { app, exited: () => !app || app.exitCode !== null, quit: () => app?.kill() };
    },
    restart: true,
    // A Start Menu shortcut opens through Explorer.
    installedApp: (v) => v.path === "explorer.exe" && /\.lnk$/i.test(v.args[0]),
    installedLaunch: "would run explorer.exe",
    knownIcon: /notepad/i,
    otherIcons: /powershell|paint|command prompt|edge|remote desktop|task manager|file explorer/i,
    iconShare: 0.5,
    chosen: "E2E Example App.exe",
    resolve: "Resolve.exe",
    // Profiles per app: Notepad for real, then a PowerShell window as another
    // app (a console window belongs to conhost, not to its own process).
    frontApp: {
      id: "notepad.exe",
      start: () => background(spawn("notepad.exe", [], { detached: true, stdio: "ignore" })),
      startOther: () =>
        background(
          spawn(
            "powershell.exe",
            ["-NoProfile", "-STA", "-Command", "Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.Form; $f.Text = 'fw-e2e-other'; [void]$f.ShowDialog()"],
            // Not detached: PowerShell started with no console exits at once.
            { stdio: "ignore" },
          ),
        ),
      // Notepad may hand its window to another process: find it by name.
      bring: (_child, which) => frontWindows(which === "app" ? ["-Name", "notepad"] : ["-Title", "fw-e2e-other"]),
      stop: (children) => {
        for (const c of children) c?.kill();
        try {
          execFileSync("taskkill", ["/F", "/IM", "notepad.exe"], { stdio: "ignore", timeout: 10000 });
        } catch {
          // already gone
        }
      },
    },
  },
  linux: {
    name: "Linux",
    async start(opts, say, dialogs) {
      if (!opts["tauri-driver"]) throw new Error("--tauri-driver <path> is required on Linux");
      const driver = await startTauriDriver(
        opts["tauri-driver"],
        opts["native-driver"],
        new URL(opts.driver).port || 4444,
        { ...process.env, [DIALOG_DIR_ENV]: dialogs },
        say,
      );
      return { app: null, exited: () => !running("freewheel"), quit: () => driver.kill() };
    },
    restart: true,
    // A desktop entry: freewheel runs its Exec line.
    installedApp: (v) => /\.desktop$/.test(v.path) && v.args.length === 0,
    installedLaunch: ".desktop",
    knownIcon: /E2E Example Editor/,
    otherIcons: /^$/,
    iconShare: 0,
    chosen: "E2E Example App.AppImage",
    jsSelectAll: true,
    resolve: "resolve",
    // Profiles per app on X11 (openbox sets _NET_ACTIVE_WINDOW): xmessage
    // stands in for an app, xev's window is another app (x11-utils).
    frontApp: {
      id: "xmessage",
      start: () => background(spawn("xmessage", ["-title", "fw-e2e-app", "freewheel e2e"], { detached: true, stdio: "ignore" })),
      startOther: () => background(spawn("xev", ["-name", "fw-e2e-other"], { detached: true, stdio: "ignore" })),
      bring: (_child, which) => frontX11(which === "app" ? "fw-e2e-app" : "fw-e2e-other"),
      stop: (children) => children.forEach((c) => c?.kill()),
    },
    prepare: (say) => {
      const desktop = installLinuxApp(homedir(), fileURLToPath(new URL("../src-tauri/icons/32x32.png", import.meta.url)));
      say(`installed the e2e desktop entry ${desktop}`);
      return () => rmSync(desktop, { force: true });
    },
  },
};

/** A stand-in app the script must not wait for: closed when the run ends, even after a failed step. */
const stray = new Set();
function background(child) {
  child.on("error", () => {});
  child.unref();
  stray.add(child);
  return child;
}
process.on("exit", () => {
  for (const c of stray) {
    try {
      c.kill();
    } catch {
      // already gone
    }
  }
});

/** Windows: brings a window to the front for real (scripts/front-window.ps1); resolves once it is. */
async function frontWindows(args) {
  const script = fileURLToPath(new URL("./front-window.ps1", import.meta.url));
  let last = "";
  for (let i = 0; i < 3; i++) {
    try {
      last = execFileSync("powershell.exe", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", script, ...args], { encoding: "utf8", timeout: 40000 }).trim();
      if (last.endsWith("ok")) return last;
    } catch (e) {
      last = `${e.stdout ?? ""}${e.message}`;
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error(`could not bring the window to the front: ${last}`);
}

/** Linux (X11): activates the window titled `title` with wmctrl once it exists. */
async function frontX11(title) {
  let last = "";
  for (let i = 0; i < 40; i++) {
    try {
      execFileSync("wmctrl", ["-F", "-a", title], { stdio: "ignore", timeout: 5000 });
      const active = execFileSync("xprop", ["-root", "_NET_ACTIVE_WINDOW"], { encoding: "utf8", timeout: 5000 });
      const list = execFileSync("wmctrl", ["-l"], { encoding: "utf8", timeout: 5000 });
      const id = list.split("\n").find((l) => l.endsWith(` ${title}`))?.split(" ")[0];
      if (id && Number.parseInt(id, 16) === Number.parseInt(active.split("#")[1] ?? "0", 16)) return "ok";
      last = `${active.trim()} / ${list.trim()}`;
    } catch (e) {
      last = e.message;
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`could not bring ${title} to the front: ${last}`);
}

/** A safe, ordered file name for a step's screenshot. */
export const shotName = (n, window, step) =>
  `${String(n).padStart(2, "0")}-${window}-${step.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "")}.png`;

export class Driver {
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
  /** A real pointer click at viewport point (x, y) (W3C actions). */
  async pointerClick(x, y) {
    const pointer = { type: "pointer", id: "mouse", parameters: { pointerType: "mouse" }, actions: [
      { type: "pointerMove", origin: "viewport", x, y, duration: 0 },
      { type: "pointerDown", button: 0 },
      { type: "pointerUp", button: 0 },
    ] };
    await this.call("POST", "/actions", { actions: [pointer] });
    await this.call("DELETE", "/actions");
  }
  type(id, text) {
    return this.call("POST", `/element/${id}/value`, { text });
  }
  screenshot() {
    return this.call("GET", "/screenshot", undefined, 30000);
  }
  elementShot(id) {
    return this.call("GET", `/element/${id}/screenshot`, undefined, 30000);
  }
}

/**
 * In-page check of the settings window's header row: the status (dot and
 * text) must not touch a tab, the On/Off text, the switch or a window
 * control, its text must stay inside it, and neither the tabs nor the row
 * may overflow. Returns { bad: [...], ... } for the log.
 */
export const HEADER_JS = `const box = (e) => e.getBoundingClientRect();
  const hit = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
  const q = (s) => document.querySelector(s);
  const status = q(".toolbar .status");
  if (!status) throw new Error("no header status");
  const s = box(status);
  const others = [...document.querySelectorAll(".toolbar .tab, .toolbar .toolbar-sep, .toolbar .switch-text, .toolbar .switch, .toolbar .window-control")];
  const bad = others.filter((o) => hit(s, box(o))).map((o) => "overlaps " + (o.getAttribute("aria-label") || o.textContent.trim() || o.className));
  const text = status.querySelector(".status-text");
  const t = box(text);
  if (t.width > 0 && (t.left < s.left - 0.5 || t.right > s.right + 0.5)) bad.push("text outside the status");
  if (s.width < 8) bad.push("no room for the status dot");
  const tabs = q(".toolbar .tabs");
  if (tabs.scrollWidth > tabs.clientWidth + 1) bad.push("tabs clipped (" + tabs.scrollWidth + " > " + tabs.clientWidth + ")");
  const row = q("nav.toolbar");
  if (row.scrollWidth > row.clientWidth + 1) bad.push("header overflows (" + row.scrollWidth + " > " + row.clientWidth + ")");
  const last = [...document.querySelectorAll(".toolbar .tab")].pop();
  // The short label is cut (an ellipsis) when its text is wider than its box.
  const cut = getComputedStyle(text).display !== "none" && text.scrollWidth > text.clientWidth;
  return { bad, width: innerWidth, label: status.getAttribute("aria-label"), shown: text.textContent, cut, status: [Math.round(s.left), Math.round(s.right)], text: Math.round(t.width), lastTab: Math.round(box(last).right), switch: Math.round(box(q(".toolbar .switch")).left) };`;


// In-page helper: the first element matching `css` whose text contains `text`.
/**
 * What's new's notes as the page shows them, in order (headings, bullets,
 * paragraphs: src/notesMarkdown.js notesText of the same markdown); throws
 * what it saw unless that equals arguments[0] (a JSON string).
 */
export const NOTES_ON_PAGE_JS = `const card = document.querySelector(".whats-new-notes");
if (!card) throw new Error("no notes card");
const got = JSON.stringify([...card.querySelectorAll("h3, h4, li, p")].map((e) => e.textContent));
if (got === arguments[0]) return true;
throw new Error("the page shows " + got.slice(0, 600));`;

/** What the e2e pastes into the Simulator's "Paste notes…": every part of the markdown subset. */
export const PASTED_NOTES = `## Pasted in the e2e

### New

* **keys:** a pasted change with \`code\` and a [link](https://github.com/brinkercode/freewheel/pull/1)
* a second pasted change

A closing paragraph.
`;

/** The notes the Simulator's "New version's notes" picks, read back from the app (sim_notes). */
export const SIM_NOTES_STATE_JS = `const done = arguments[arguments.length - 1];
window.__TAURI__.core.invoke("sim_notes", { choice: "State", text: null }).then((n) => done(JSON.stringify(n)), (e) => done("error: " + e));`;

export const FIND_JS = `const [css, text] = arguments;
  return [...document.querySelectorAll(css)].find((e) => !text || e.textContent.replace(/\\s+/g, " ").includes(text)) ?? null;`;

export async function main(platform, argv = process.argv.slice(2)) {
  const P = PLATFORMS[platform];
  if (!P) throw new Error(`unknown platform ${platform}`);
  const opts = parseArgs(argv);
  const out = resolve(opts.out);
  mkdirSync(out, { recursive: true });
  const d = new Driver(opts.driver);
  const log = [];
  let n = 0;
  let current = null;
  const win = {};
  let app = null;
  let cleanup = null;
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
    // Below the fold of a scrolling panel (the Simulator's Jump to) WebDriver may not scroll it in itself.
    await d.exec("arguments[0].scrollIntoView({ block: 'center' })", [el]);
    const id = Object.values(el)[0];
    try {
      await d.click(id);
    } catch (e) {
      // WebKitWebDriver finds no interactable point on SVG groups (the
      // drawing's keys): click their centre with a real pointer instead.
      if (!/not interactable/.test(e.message)) throw e;
      const [x, y] = await d.exec(`const r = arguments[0].getBoundingClientRect(); return [Math.round(r.left + r.width / 2), Math.round(r.top + r.height / 2)];`, [el]);
      await d.pointerClick(x, y);
    }
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
      // A page still loading (a step that reloads it) is not a freeze: ask
      // again until it has loaded, within the same RESPONSIVE_MS.
      for (;;) {
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
        if (!String(r).startsWith("not loaded") || Date.now() - t > RESPONSIVE_MS) break;
        await new Promise((ok) => setTimeout(ok, 100));
      }
      const ms = Date.now() - t;
      if (r !== "ok" || ms > RESPONSIVE_MS)
        throw new Error(`after "${after}": the ${name} window did not answer ${command} within ${RESPONSIVE_MS} ms (${ms} ms: ${r})`);
    }
  };
  /** Runs `body` (an async function body; `arguments` as usual) in window `name` through the app; resolves to its value. */
  const ask = async (name, body, args = []) => {
    await use(name);
    return d.execAsync(
      `const done = arguments[arguments.length - 1];
       (async (...args) => { ${body} })(...[...arguments].slice(0, -1)).then(done, (e) => done("error: " + e));`,
      args,
    );
  };
  /** Polls `body` (as for ask) in window `name` until it returns `want`. */
  const waitAsk = async (name, what, body, want, ms = 10000) => {
    const end = Date.now() + ms;
    let last;
    while (Date.now() < end) {
      last = await ask(name, body).catch((e) => e.message);
      if (last === want) return last;
      await new Promise((r) => setTimeout(r, 150));
    }
    throw new Error(`${name}: waited ${ms} ms for ${what} (last: ${JSON.stringify(last)})`);
  };
  const W = "const w = window.__TAURI__.window.getCurrentWindow();";
  /** The window's own title bar: no OS decorations; its minimise, maximise / restore buttons act on the real window. */
  const windowControls = async (name) => {
    await step(`${name}: no OS title bar, the app's header and controls`, async () => {
      const decorated = await ask(name, `${W} return await w.isDecorated();`);
      if (decorated !== false) throw new Error(`${name}: the window has OS decorations (${JSON.stringify(decorated)})`);
      const drag = await d.exec(`return document.querySelector('[data-tauri-drag-region="deep"] .window-controls') ? "ok" : "none"`);
      if (drag !== "ok") throw new Error(`${name}: no draggable header with window controls`);
      // One header row: the controls' vertical centre is the On switch's (settings) or the heading's (simulator).
      const mid = await d.exec(
        `const c = (s) => { const e = document.querySelector(s); if (!e) return null; const r = e.getBoundingClientRect(); return r.top + r.height / 2; };
         return [c(".window-controls"), c(arguments[0])];`,
        [name === "settings" ? ".toolbar .switch" : ".fw-sim-head h1"],
      );
      if (mid[0] == null || mid[1] == null || Math.abs(mid[0] - mid[1]) > 4)
        throw new Error(`${name}: window controls not in line with the header row (centres ${JSON.stringify(mid)})`);
      say(`  controls centre ${mid[0]}, row centre ${mid[1]}`);
    }, [name]);
    await step(`${name}: maximise`, async () => {
      await click(name, ".window-control[aria-label=Maximise]");
      await waitAsk(name, "maximised", `${W} return await w.isMaximized();`, true);
      await waitFor(name, "the Restore button", `return document.querySelector(".window-control[aria-label=Restore]")`);
    }, [name]);
    await step(`${name}: restore`, async () => {
      await click(name, ".window-control[aria-label=Restore]");
      await waitAsk(name, "restored", `${W} return await w.isMaximized();`, false);
      await waitFor(name, "the Maximise button", `return document.querySelector(".window-control[aria-label=Maximise]")`);
    }, [name]);
    await step(`${name}: minimise, then back`, async () => {
      await click(name, ".window-control[aria-label=Minimise]");
      await waitAsk(name, "minimised", `${W} return await w.isMinimized();`, true);
      await ask(name, `${W} await w.unminimize(); return true;`);
      await waitAsk(name, "shown again", `${W} return await w.isMinimized();`, false);
    }, [name]);
  };
  /** One step: the action, then the responsiveness check, then the screenshots. */
  let stepLine = 0;
  const step = async (title, action, shots = ["settings"]) => {
    say(`step: ${title}`);
    // Log checks look only at lines numbered after this point: what this
    // step's actions did, however many lines they add (the window keeps 40).
    const was = current;
    stepLine = win.simulator ? await lastLine().catch(() => 0) : 0;
    if (was) await use(was);
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

  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  /** The running config, read through get_config in the settings window. */
  const readConfig = async () => {
    await use("settings");
    const r = await d.execAsync(`const done = arguments[0];
      window.__TAURI__.core.invoke("get_config").then((c) => done(JSON.stringify(c)), (e) => done("error " + e));`);
    if (r.startsWith("error")) throw new Error(`get_config: ${r}`);
    return JSON.parse(r);
  };
  /** Polls the config until `check(config)` holds; returns that config. */
  const configShows = async (what, check, ms = 10000) => {
    const end = Date.now() + ms;
    let last;
    while (Date.now() < end) {
      try {
        // The settings window may be reloading (a Simulator setup reloads it).
        last = await readConfig();
        if (check(last)) {
          say(`  config: ${what}`);
          return last;
        }
      } catch {
        // not there yet
      }
      await sleep(200);
    }
    throw new Error(`the config never showed ${what}: ${JSON.stringify(last).slice(0, 600)}`);
  };
  const basePage = (c) => c.layers.find((l) => l.id === c.base_layer);
  /** One of the newest lines of the Simulator's log contains `text`. */
  const logged = (text, ms = 10000) =>
    waitFor("simulator", `the log line "${text}"`, `return [...document.querySelectorAll(".fw-sim-log li[data-n]")].some((li) => Number(li.dataset.n) > arguments[1] && li.textContent.includes(arguments[0]))`, [text, stepLine], ms);
  const shows = (name, text, ms) => waitFor(name, `"${text}"`, `return document.body.innerText.includes(arguments[0])`, [text], ms);
  const gone = (name, css) => waitFor(name, `no ${css}`, `return !document.querySelector(arguments[0])`, [css]);
  /** Types `text` (WebDriver keys: \uE009 Ctrl, \uE008 Shift, \uE000 releases them, \uE007 Enter) into `css`. */
  const SELECT_ALL = "\uE009a\uE000";
  const typeInto = async (name, css, text) => {
    const el = await waitFor(name, css, FIND_JS, [css, ""]);
    // WebKitWebDriver does not turn Ctrl+A into select-all: select the text itself.
    if (P.jsSelectAll && text.startsWith(SELECT_ALL)) {
      await d.exec("arguments[0].focus(); arguments[0].select();", [el]);
      text = text.slice(SELECT_ALL.length);
    }
    await d.type(Object.values(el)[0], text);
  };
  /** Presses `mods` (WebDriver key codes) + `key` together, as a person does, into the focused element. */
  const chord = async (mods, key) => {
    const down = [...mods, key].map((value) => ({ type: "keyDown", value }));
    const up = [key, ...[...mods].reverse()].map((value) => ({ type: "keyUp", value }));
    await d.call("POST", "/actions", { actions: [{ type: "key", id: "keyboard", actions: [...down, ...up] }] });
    await d.call("DELETE", "/actions");
  };
  /** Clicks the record box `css` (its capture input is invisible) and records the shortcut. */
  const record = async (css, mods, key) => {
    await click("settings", css);
    await waitFor("settings", "the record box listening", `return document.activeElement?.classList.contains("combo-capture")`);
    await chord(mods, key);
  };
  const switchState = (name, label) =>
    waitFor(name, `the "${label}" switch`, `return document.querySelector('[role=switch][aria-label="' + arguments[0] + '"]')?.getAttribute("aria-checked")`, [label]);
  /** Clicks switch `label` and waits until it reads `on`. */
  const flip = async (label, on) => {
    await click("settings", `[role=switch][aria-label="${label}"]`);
    await waitFor("settings", `"${label}" ${on ? "on" : "off"}`, `return document.querySelector('[role=switch][aria-label="' + arguments[0] + '"]')?.getAttribute("aria-checked") === arguments[1]`, [label, String(on)]);
  };
  /** A press and release of `key` (printed name) with the Simulator's quick keys. */
  const press = (key) => click("simulator", ".fw-sim-quick button", key);
  const turn = (dir) => click("simulator", ".fw-sim-line button", dir === "left" ? "Turn wheel left" : "Turn right");
  const choice = (text) => click("settings", ".side-panel label.choice", text);
  const allChoices = () => click("settings", ".side-panel button.back-link", "All choices");
  /** Back to the key panel's choices list, if a choice's detail is open. */
  const toChoices = async () => {
    await use("settings");
    if (await d.exec(`return !!document.querySelector(".side-panel button.back-link")`)) await allChoices();
  };
  const keyIn = (key) => click("settings", `.device [data-binding="${key}"]`);
  // Every state the tray's items take is recorded from before each click on, so a
  // short one (Checking for updates… lasts 1.5 s) is seen however slow the polling is.
  const TRAY_WATCH_JS = `if (!window.__fwTraySeen) {
      window.__fwTraySeen = {};
      const note = () => document.querySelectorAll("[data-tray]").forEach((b) => {
        const seen = (window.__fwTraySeen[b.dataset.tray] ??= []);
        const state = [b.textContent, b.getAttribute("aria-disabled") === "true"];
        const last = seen[seen.length - 1];
        if (!last || last[0] !== state[0] || last[1] !== state[1]) seen.push(state);
      });
      note();
      new MutationObserver(note).observe(document.body, { subtree: true, childList: true, characterData: true, attributes: true });
    }
    for (const k of Object.keys(window.__fwTraySeen)) window.__fwTraySeen[k] = window.__fwTraySeen[k].slice(-1);
    return true;`;
  const tray = async (id) => {
    await use("simulator");
    await d.exec(TRAY_WATCH_JS);
    return click("simulator", `[role=group][aria-label="Tray menu"] button[data-tray="${id}"]`);
  };
  const jumpTo = (text) => click("simulator", ".fw-sim-grid button", text);
  /** The newest Simulator log line's number (Simulator.jsx data-n). */
  const lastLine = async () => (await use("simulator"), d.exec(`return Number(document.querySelector(".fw-sim-log li[data-n]")?.dataset.n ?? 0);`));
  /** Clicks tray item `id` and waits for its own new log line `Tray: <text>` (never an older one). */
  const trayClick = async (id, text) => {
    const before = await lastLine();
    await tray(id);
    await waitFor("simulator", `a new log line "Tray: ${text}"`, `const [n, text] = arguments;
      return [...document.querySelectorAll(".fw-sim-log li[data-n]")].some((li) => Number(li.dataset.n) > n && li.textContent.includes("Tray: " + text));`, [before, text]);
  };

  /** Both windows: the settings window ("main") and the Simulator (#simulator). */
  const findWindows = async () => {
    delete win.settings;
    delete win.simulator;
    current = null;
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
  };
  /** Saves `config` through save_config from the settings window. */
  const saveConfig = async (config) => {
    await use("settings");
    const r = await d.execAsync(`const [config, done] = arguments;
      window.__TAURI__.core.invoke("save_config", { config }).then(() => done("ok"), (e) => done("error " + JSON.stringify(e)));`, [config]);
    if (r !== "ok") throw new Error(`save_config: ${r}`);
  };
  /**
   * The mappings part of a config, as a backup file holds it: pages by name
   * (a restore on a fresh setup makes page ids from the names, SCHEMA.md).
   */
  const mappings = (c) => {
    const name = (id) => c.layers.find((l) => l.id === id)?.name ?? id;
    const fix = (a) => (a?.LayerPush ? { LayerPush: { target: name(a.LayerPush.target) } } : a);
    const layers = c.layers.map((l) => ({ name: l.name, bindings: Object.fromEntries(Object.entries(l.bindings).sort().map(([k, a]) => [k, fix(a)])) }));
    const jog = { ...c.jog, per_mode: Object.fromEntries(Object.entries(c.jog.per_mode).sort().map(([m, b]) => [m, { ...b, left: fix(b.left), right: fix(b.right) }])) };
    return JSON.stringify({ layers, base: name(c.base_layer), jog, overrides: c.overrides });
  };
  const titleOf = (key) => d.exec(`return document.querySelector('.device [data-binding="' + arguments[0] + '"]')?.getAttribute("title") ?? null`, [key]);
  /** The drawing's label of each key in `want` ({ key: label }). */
  const drawingShows = (want) =>
    waitFor("settings", `the drawing showing ${JSON.stringify(want)}`, `const want = arguments[0];
      return Object.entries(want).every(([k, label]) => document.querySelector('.device [data-binding="' + k + '"]')?.getAttribute("title") === label);`, [want], 15000);
  const activePill = (name) => waitFor("settings", `the page "${name}" shown`, `return document.querySelector(".pill.is-active button[role=tab]")?.textContent.includes(arguments[0])`, [name]);
  /** Presses `key` (binding name) with the Simulator's key picker. */
  const pressKey = async (key) => {
    await click("simulator", `select[aria-label="Key to press"] option[value="${key}"]`);
    await click("simulator", ".fw-sim-line button.fw-sim-primary", "Press");
  };
  /** Opens the tab's More section unless it is open. */
  const openMore = async () => {
    await waitFor("settings", "More", `return document.querySelector("button.more")`);
    if (await d.exec(`return document.querySelector("button.more").getAttribute("aria-expanded") !== "true"`)) await click("settings", "button.more", "More");
  };
  /** Help & backup → Restore from a backup… with 'Let restored keys open apps' `allow`; `file` (else the backup) via the stubbed dialog. */
  const restoreFrom = async (file, allow) => {
    if (file) writeFileSync(join(dialogs, "next-open.txt"), file);
    await click("settings", "nav.toolbar button.tab", "Help & backup");
    await waitFor("settings", "Help & backup", `return document.querySelector("#backup")`);
    await openMore();
    if ((await switchState("settings", "Let restored keys open apps")) !== String(allow)) await flip("Let restored keys open apps", allow);
    await click("settings", "#backup button", "Restore from a backup");
  };
  const lineWith = (...parts) =>
    waitFor("simulator", `a log line with ${parts.join(" + ")}`, `const parts = arguments[0];
      return [...document.querySelectorAll(".fw-sim-log li[data-n]")].some((li) => Number(li.dataset.n) > arguments[1] && parts.every((p) => li.textContent.includes(p)));`, [parts, stepLine]);
  const dialogs = join(out, "dialogs");
  let saved = null;
  let freshWheel = null;

  /**
   * README-grade stills (docs/images): the window's own client area only
   * (it draws its own title bar, so no desktop shows), at its 1000x640
   * default, the pointer parked and toasts gone, in Light and Dark, into
   * <out>/readme/<look>-<screen>.png. The sample setup gives the drawing
   * real labels.
   */
  const README_DIR = join(out, "readme");
  const still = async (name, file) => {
    await use(name);
    await d.exec("document.querySelectorAll('*').forEach((e) => { if (e.scrollTop) e.scrollTop = 0; })");
    await d.call("POST", "/actions", { actions: [{ type: "pointer", id: "mouse", parameters: { pointerType: "mouse" }, actions: [{ type: "pointerMove", x: 2, y: 2, origin: "viewport" }] }] }).catch(() => {});
    await d.call("DELETE", "/actions").catch(() => {});
    await sleep(2500); // the "You pressed" line and toasts fade, transitions end
    const size = await d.exec("return [innerWidth, innerHeight, devicePixelRatio].join('x')");
    writeFileSync(join(README_DIR, file), Buffer.from(await d.screenshot(), "base64"));
    say(`  readme screenshot ${file} (${size})`);
  };
  const readmeTab = async (t) => {
    await click("settings", "nav.toolbar button.tab", t);
    await waitFor("settings", `the ${t} tab`, `return document.querySelector('nav.toolbar button.tab[aria-pressed="true"]')?.textContent.includes(arguments[0])`, [t]);
  };
  const readmeShots = async () => {
    mkdirSync(README_DIR, { recursive: true });
    await step("README: the sample setup over USB", async () => {
      await click("simulator", `[role=group][aria-label="Speed Editor"] button`, "USB");
      // Only the sample keys on one page, and the wheel as a fresh install has it.
      await click("simulator", `[role=group][aria-label="Keys"] button`, "Empty setup");
      await logged("empty setup loaded");
      await configShows("no keys", (c) => c.layers.length === 1 && Object.keys(c.layers[0].bindings).length === 0);
      if (freshWheel) await saveConfig({ ...(await readConfig()), ...freshWheel });
      await click("simulator", `[role=group][aria-label="Keys"] button`, "Several set");
      await logged("sample mappings loaded");
      await waitFor("settings", "the drawing's Play/Pause", `return document.querySelector('.device [data-binding="StopPlay"]')?.getAttribute("title") === "Play/Pause"`, [], 20000);
      await waitFor("settings", "Connected", `return document.querySelector("button.status")?.textContent.includes("Connected")`);
      await press("SCRL");
      await waitFor("settings", "SCRL lit", `return document.querySelector('.device [data-binding="Scrl"]')?.classList.contains("is-lit")`);
    }, []);
    for (const look of ["Light", "Dark"]) {
      const f = look.toLowerCase();
      await step(`README: screenshots (${look})`, async () => {
        await readmeTab("General");
        await click("settings", "button.appearance-option", look);
        await waitFor("settings", `appearance ${look}`, `return document.documentElement.dataset.appearance === arguments[0]`, [look]);
        await sim("Computer appearance", look).catch(() => {});
        await still("settings", `${f}-general.png`);
        await readmeTab("Wheel");
        await still("settings", `${f}-wheel.png`);
        await readmeTab("DaVinci Resolve");
        await still("settings", `${f}-resolve.png`);
        await readmeTab("Help & backup");
        await still("settings", `${f}-help.png`);
        await readmeTab("Keys");
        await keyIn("StopPlay");
        await waitFor("settings", "STOP/PLAY's panel", `return document.querySelector(".side-panel .panel-title")?.textContent.includes("STOP")`);
        await still("settings", `${f}-keys.png`);
        await click("settings", '.side-panel button[aria-label="Close"]');
        await still("simulator", `${f}-simulator.png`);
      }, []);
    }
    await step("README: back to Match my computer", async () => {
      await readmeTab("General");
      await click("settings", "button.appearance-option", "Match my computer");
      await sim("Computer appearance", "Light");
      await readmeTab("Keys");
    }, []);
  };

  try {
    say(`starting ${opts.app} --simulate`);
    mkdirSync(dialogs, { recursive: true });
    cleanup = P.prepare?.(say) ?? null;
    app = await P.start(opts, say, dialogs);
    await d.start(resolve(opts.app), opts.port);
    await findWindows();
    say(`windows: ${JSON.stringify(win)}`);

    // --simulate does what a key is set to on this PC unless the Simulator's
    // 'Do actions for real' is off: on at first; off for the walk-through
    // below (its keys would type into these windows), on again where a
    // press must really run something.
    const realSwitch = `[role=switch][aria-label="Do actions for real"]`;
    const setReal = async (on) => {
      await use("simulator");
      if ((await d.exec(`return document.querySelector(arguments[0]).checked`, [realSwitch])) !== on) await click("simulator", realSwitch);
      await waitFor("simulator", `Do actions for real ${on ? "on" : "off"}`, `return document.querySelector(arguments[0]).checked === arguments[1]`, [realSwitch, on]);
      await shows("simulator", on ? "On: a press does what its key is set to on this PC" : "Off: nothing happens on this PC");
    };
    await step("simulator: Do actions for real is on at first, then off for the walk-through", async () => {
      const on = await waitFor("simulator", "the Do actions for real switch", `const s = document.querySelector(arguments[0]); return s && String(s.checked)`, [realSwitch]);
      if (on !== "true") throw new Error(`Do actions for real is ${on} at first, not on`);
      await setReal(false);
      await logged("Do actions for real is off");
    }, ["simulator"]);
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

    // The OS title bar is gone (decorations false); the app draws its own.
    await windowControls("settings");
    await windowControls("simulator");
    // Here freewheel's permissions are the Simulator's: the notice shows only when
    // it fakes a missing macOS permission, and Open System Settings says what it
    // would open there; granting it in the Simulator clears it.
    const noPermissionNotice = `return !document.querySelector(".notice-permission") && !document.body.innerText.includes("needs your permission")`;
    await step(`no permission notice on ${P.name}`, () => waitFor("settings", "no permission notice", noPermissionNotice));
    await sim("macOS permission", "Missing");
    await step("permission missing: the notice", () => waitFor("settings", "the permission notice", `return document.querySelector(".notice-permission")`));
    await step("permission missing: Fix this", async () => {
      await click("settings", ".notice-permission button", "Fix this");
      await waitFor("settings", "the permission fix", `return document.body.innerText.includes("freewheel needs your permission")`);
    });
    await step("permission missing: Open System Settings", async () => {
      await click("settings", ".permission-row button.btn-primary", "Open System Settings");
      const toast = await waitFor("settings", "the simulated System Settings line", `return document.querySelector(".toast")?.textContent.includes("would open System Settings › Privacy & Security › Accessibility") && document.querySelector(".toast").textContent`);
      say(`  toast: ${toast}`);
    });
    await sim("macOS permission", "Allowed");
    await step("permission granted: Done", async () => {
      await waitFor("settings", "Allowed", `return document.querySelector(".permission-row.granted") && !document.querySelector(".permission-row:not(.granted)")`);
      await click("settings", ".permissions button.btn-lg", "Done");
      await waitFor("settings", "the settings tabs, no notice", `return document.querySelector("nav.toolbar") && !document.querySelector(".notice-permission")`);
    });

    await tab("Keys", "What each key does");
    freshWheel = await readConfig().then((c) => ({ jog: c.jog, overrides: c.overrides })); // the README stills use the default wheel
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
      const site = JSON.parse(saved)?.LaunchApp?.args?.[0] ?? "";
      if (URL.canParse(site) ? new URL(site).hostname !== "example.com" : true) {
        throw new Error(`CUT not saved in the config: ${saved}`);
      }
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
    // Updates in --simulate: the Rust updater's simulated source (selfupdate.rs)
    // through the same states as a real update, dark then light.
    const updateState = (state, ms = 15000) =>
      waitFor("settings", `the update state ${state}`, `return document.querySelector('.version-card [data-update="' + arguments[0] + '"]')`, [state], ms);
    await step("General: appearance Dark for the update", async () => {
      await click("settings", "button.appearance-option", "Dark");
      await waitFor("settings", "appearance Dark", `return document.documentElement.dataset.appearance === "Dark"`);
    });
    await step("updates: checking (dark)", async () => {
      await click("settings", ".version-card .update-button", "Check for updates");
      await updateState("checking", 2000);
    });
    await step("updates: up to date (dark)", () => updateState("current"));
    await step("updates: Update automatically off", async () => {
      await click("settings", '[aria-label="Update automatically"]');
      await waitFor("settings", "Update automatically off", `return document.querySelector('[aria-label="Update automatically"]').getAttribute("aria-checked") === "false"`);
    });
    // The simulated update carries the notes the Simulator picked: first the
    // next release's real notes, embedded at build (--next-notes is the same
    // file, scripts/release-notes.mjs preview), shown verbatim in What's new.
    const nextNotes = opts["next-notes"] ? readFileSync(opts["next-notes"], "utf8") : null;
    const NOTES_GROUP = "New version’s notes";
    const simNotesState = async () => {
      await use("simulator");
      const r = await d.execAsync(SIM_NOTES_STATE_JS);
      if (String(r).startsWith("error")) throw new Error(`sim_notes: ${r}`);
      return JSON.parse(r);
    };
    const firstChoice = nextNotes ? "Next release (real notes)" : "Sample notes";
    await sim(NOTES_GROUP, firstChoice);
    let expectedNotes;
    {
      const state = await simNotesState();
      if (nextNotes) {
        if (!state.next_available || state.notes !== nextNotes) throw new Error(`the build's next-release notes are not ${opts["next-notes"]}: ${JSON.stringify(state).slice(0, 300)}`);
        say(`  next release notes embedded: ${notesText(nextNotes).length} lines`);
      }
      expectedNotes = notesText(state.notes);
    }
    await step("simulator: New version, available (dark)", async () => {
      await click("simulator", ".fw-sim-grid button", "New version");
      await updateState("available");
      await waitFor("settings", "the banner", `return document.querySelector('.update-banner[data-update="available"]')`);
    }, ["simulator", "settings"]);
    await step("updates: downloading with progress (dark)", async () => {
      await click("settings", ".version-card .update-button", "Download and install");
      await waitFor("settings", "progress past 25%", `return Number(document.querySelector(".version-card [role=progressbar]")?.getAttribute("aria-valuenow")) >= 25`);
    });
    await step("updates: ready, restart to update (dark)", () => updateState("ready", 10000));
    await step("General: appearance Light, ready", async () => {
      await click("settings", "button.appearance-option", "Light");
      await waitFor("settings", "appearance Light", `return document.documentElement.dataset.appearance === "Light"`);
    });
    await step("updates: restarting (light)", async () => {
      await click("settings", ".update-banner button", "Restart to update");
      await updateState("restarting", 2000);
    });
    const newer = await waitFor("settings", "What's new after the simulated restart", `return document.body.innerText.match(/freewheel is updated to (\\S+)/)?.[1]`, [], 10000);
    await step(`updates: What's new for ${newer} (light)`, async () => {
      await waitFor("settings", "the new version on the General tab", `return document.querySelector(".version-card .card-title").textContent === "freewheel " + arguments[0]`, [newer]);
    });
    await step("updates: What's new page", async () => {
      await click("settings", ".update-banner button", "What’s new in v");
      await waitFor("settings", `the notes (${firstChoice}), verbatim`, NOTES_ON_PAGE_JS, [JSON.stringify(expectedNotes)]);
      say(`  What's new shows all ${expectedNotes.length} lines of the ${firstChoice}`);
    });
    // Then the sample notes and pasted ones, each through the whole update.
    for (const [choice, label] of [["Sample", "Sample notes"], ["Paste", "Paste notes…"]]) {
      await step(`simulator: ${NOTES_GROUP}: ${label}`, async () => {
        await click("simulator", `[role=group][aria-label="${NOTES_GROUP}"] button`, label);
        if (choice === "Paste") {
          // A paste into the textarea (React's value setter + input), then Use these notes.
          await waitFor("simulator", "the paste box", `return document.querySelector('[aria-label="Pasted notes"]')`);
          await d.exec(`const ta = document.querySelector('[aria-label="Pasted notes"]');
            Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(ta, arguments[0]);
            ta.dispatchEvent(new Event("input", { bubbles: true }));`, [PASTED_NOTES]);
          await click("simulator", ".fw-sim-paste button", "Use these notes");
        }
        await waitFor("simulator", `${label} picked`, `return document.querySelector("[data-notes-preview]") && [...document.querySelectorAll('[role=group][aria-label="' + arguments[0] + '"] button')].some((b) => b.textContent === arguments[1] && b.getAttribute("aria-pressed") === "true")`, [NOTES_GROUP, label]);
        const state = await simNotesState();
        if (state.choice !== choice) throw new Error(`picked ${choice}, the app has ${JSON.stringify(state).slice(0, 200)}`);
        if (choice === "Paste" && state.notes !== PASTED_NOTES) throw new Error(`the app kept other notes: ${JSON.stringify(state.notes)}`);
        if (choice === "Sample" && !state.notes.includes("Smoother scrolling")) throw new Error(`not the sample notes: ${state.notes}`);
        expectedNotes = notesText(state.notes);
      }, ["simulator"]);
      await step(`updates (${label}): New version, available`, async () => {
        await click("settings", "nav.toolbar button.tab", "General");
        await click("simulator", ".fw-sim-grid button", "New version");
        await updateState("available");
      });
      await step(`updates (${label}): downloaded, ready`, async () => {
        await click("settings", ".version-card .update-button", "Download and install");
        await updateState("ready", 15000);
      });
      await step(`updates (${label}): Restart to update`, async () => {
        await click("settings", ".update-banner button", "Restart to update");
        await waitFor("settings", "What's new after the restart", `return document.querySelector(".update-banner button") && [...document.querySelectorAll(".update-banner button")].some((b) => b.textContent.startsWith("What’s new in v")) && document.body.innerText.includes("freewheel is updated to")`, [], 15000);
      });
      await step(`updates (${label}): What's new shows exactly those notes`, async () => {
        await click("settings", ".update-banner button", "What’s new in v");
        await waitFor("settings", `the notes (${label}), verbatim`, NOTES_ON_PAGE_JS, [JSON.stringify(expectedNotes)]);
        say(`  What's new shows all ${expectedNotes.length} lines of the ${label}`);
      });
    }
    await step("updates: Update automatically back on", async () => {
      await click("settings", "nav.toolbar button.tab", "General");
      await click("settings", '[aria-label="Update automatically"]');
      await waitFor("settings", "Update automatically on", `return document.querySelector('[aria-label="Update automatically"]').getAttribute("aria-checked") === "true"`);
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

    // ---- Every remaining control: click it, then see what it did -------
    await tab("Help & backup", "Help");
    await step("Help: Download logs", async () => {
      await click("settings", "#logs button.btn-primary", "Download logs");
      await waitFor("settings", "Downloaded", `return document.querySelector("#logs .success-line")?.textContent.includes("Downloaded")`);
      await logged("Saved the logs to");
    }, ["settings", "simulator"]);
    await step("Help: Show file", async () => {
      await click("settings", "#logs a.show-file", "Show file");
      await logged("in its folder");
      await use("settings");
      const failed = await d.exec(`return document.querySelector("#logs [role=alert]")?.textContent ?? null`);
      if (failed) throw new Error(`Show file failed: ${failed}`);
    }, ["settings", "simulator"]);
    await step("Help: Back up", async () => {
      await click("settings", "#backup button", "Back up");
      await shows("settings", "Your keys are backed up.");
      await logged("Backed up the keys to");
    }, ["settings", "simulator"]);
    await step("Help: Restore from a backup", async () => {
      await click("settings", "#backup button", "Restore from a backup");
      await shows("settings", "Your keys are restored.");
      await logged("Restored the keys from");
    }, ["settings", "simulator"]);
    await step("Help: More, detailed logs on and off", async () => {
      await click("settings", "button.more", "More");
      await flip("Turn on detailed logs for troubleshooting", true);
      await configShows("detailed_logs on", (c) => c.settings.detailed_logs === true);
      await flip("Turn on detailed logs for troubleshooting", false);
      await configShows("detailed_logs off", (c) => c.settings.detailed_logs === false);
    });
    await step("Help: let restored keys open apps", async () => {
      await flip("Let restored keys open apps", true);
      await flip("Let restored keys open apps", false);
    });
    // Help & backup > More > Device test: read-only, live from the
    // device-diagnostics event, fed by the simulated Speed Editor.
    const reportShows = (what, js) =>
      waitFor("settings", what, `return [...document.querySelectorAll(".diag-report")].some((li) => ${js})`, [], 10000);
    await step("Help: Device test shows the simulated device", async () => {
      await click("settings", "#device-test button", "Open device test");
      await waitFor("settings", "the Device test view", `return document.querySelector('[data-screen-label="Device test"]')`);
      await shows("settings", "Simulated Speed Editor: these reports come from the Simulator window.");
      await waitFor("settings", "Connected", `return document.querySelector(".diag-status .status-dot.dot-green")`);
      await waitFor("settings", "no switches or inputs", `return !document.querySelector('[data-screen-label="Device test"] input, [data-screen-label="Device test"] [role=switch]')`);
    }, ["settings"]);
    await step("Device test: a key press arrives live as its raw report", async () => {
      await click("simulator", ".fw-sim-quick button", "CUT");
      await reportShows("the CUT key report", `li.querySelector(".diag-hex").textContent.startsWith("04 0f 00") && li.textContent.includes("keys held: CUT")`);
      await waitFor("settings", "CUT in the presses", `return document.body.innerText.includes("Pressed (newest first)") && document.querySelector('[aria-label="Keys"]').textContent.includes("CUT")`);
    }, ["settings", "simulator"]);
    await step("Device test: a wheel turn arrives live", async () => {
      await click("simulator", ".fw-sim-line button", "Turn right");
      await reportShows("the wheel report", `li.querySelector(".diag-hex").textContent.startsWith("03 ") && li.textContent.includes("wheel mode")`);
    }, ["settings", "simulator"]);
    await sim("Speed Editor", "Bluetooth 64%");
    await step("Device test: battery and the wheel-mode write", async () => {
      await waitFor("settings", "64% on Bluetooth", `const t = document.querySelector('[data-screen-label="Device test"]')?.innerText ?? ""; return t.includes("Bluetooth") && t.includes("64%")`);
      await reportShows("the wheel mode set on connect", `li.dataset.direction === "Out" && li.textContent.includes("set wheel mode")`);
    }, ["settings"]);
    await step("Device test: Copy diagnostics", async () => {
      await click("settings", '[data-screen-label="Device test"] button.btn-primary', "Copy diagnostics");
      const line = await waitFor("settings", "the copy confirmation", `return document.querySelector('[data-screen-label="Device test"] .success-line')?.textContent`);
      say(`  copy: ${line}`);
      if (!line.includes("Diagnostics copied")) throw new Error(`Copy diagnostics: ${line}`);
    }, ["settings"]);
    await step("Device test: back to Help & backup", async () => {
      await click("settings", '[data-screen-label="Device test"] button.more', "Help & backup");
      await waitFor("settings", "Help & backup", `return document.querySelector("#backup")`);
    });

    await tab("General", "Appearance");
    await step("General: start at login on and off (simulated)", async () => {
      await flip("Start freewheel when I log in", true);
      await logged("would turn start at login on");
      await configShows("start_at_login on", (c) => c.settings.start_at_login === true);
      await flip("Start freewheel when I log in", false);
      await logged("would turn start at login off");
      await configShows("start_at_login off", (c) => c.settings.start_at_login === false);
    }, ["settings", "simulator"]);
    await step("General: appearance Match my computer", async () => {
      await click("settings", "button.appearance-option", "Match my computer");
      await waitFor("settings", "appearance System", `return document.documentElement.dataset.appearance === "System"`);
      await configShows("appearance System", (c) => c.settings.appearance === "System");
    });
    await step("General: update automatically flipped and back", async () => {
      const was = (await switchState("settings", "Update automatically")) === "true";
      await flip("Update automatically", !was);
      await configShows(`auto_update ${!was}`, (c) => c.settings.auto_update === !was);
      await flip("Update automatically", was);
      await configShows(`auto_update ${was}`, (c) => c.settings.auto_update === was);
    });

    await tab("DaVinci Resolve");
    await step("Resolve: step aside off, Resolve opens, freewheel stays on", async () => {
      await flip("Step aside automatically when DaVinci Resolve opens", false);
      await configShows("step_aside_for_resolve off", (c) => c.settings.step_aside_for_resolve === false);
      await click("simulator", `[role=group][aria-label="DaVinci Resolve"] button`, "Open");
      await sleep(500);
      const on = await waitFor("simulator", "freewheel group", `return [...document.querySelectorAll('[role=group][aria-label="freewheel"] button')].find((b) => b.getAttribute("aria-pressed") === "true")?.textContent ?? "none"`);
      if (on !== "On") throw new Error(`freewheel stepped aside although the switch is off (${on})`);
      await click("simulator", `[role=group][aria-label="DaVinci Resolve"] button`, "Closed");
      await flip("Step aside automatically when DaVinci Resolve opens", true);
      await configShows("step_aside_for_resolve on", (c) => c.settings.step_aside_for_resolve === true);
    }, ["settings", "simulator"]);
    await step("Resolve: program name, then Reset", async () => {
      await click("settings", "button.more", "More");
      await typeInto("settings", "input.program-name", `${SELECT_ALL}Resolve2.exe\uE007`);
      await configShows("resolve_process_name Resolve2.exe", (c) => c.settings.resolve_process_name === "Resolve2.exe");
      await click("settings", ".input-row button", "Reset");
      await configShows(`resolve_process_name ${P.resolve}`, (c) => c.settings.resolve_process_name === P.resolve);
    });

    await tab("Wheel", "When I turn the wheel");
    const wheelKind = (text) => click("settings", ".radio-rows button.radio-row", text);
    await step("Wheel: SHTL changes the volume", async () => {
      await click("settings", ".wheel-buttons button", "SHTL");
      await wheelKind("Change the volume");
      await configShows("SHTL: Volume", (c) => basePage(c).bindings.Shtl.SetJogMode.mode === "Volume");
      await press("SHTL");
      await turn("right");
      await logged("would turn volume up");
    }, ["settings", "simulator"]);
    await step("Wheel: SHTL changes the brightness", async () => {
      await wheelKind("Change screen brightness");
      await configShows("SHTL: Brightness", (c) => basePage(c).bindings.Shtl.SetJogMode.mode === "Brightness");
      await press("SHTL");
      await turn("left");
      await logged("would turn brightness down");
    }, ["settings", "simulator"]);
    await step("Wheel: SHTL does nothing", async () => {
      await wheelKind("Nothing");
      await configShows("SHTL: nothing", (c) => basePage(c).bindings.Shtl.SetJogMode.mode === "Shuttle" && c.jog.per_mode.Shuttle.left === null && c.jog.per_mode.Shuttle.right === null);
    });
    await step("Wheel: SHTL presses recorded shortcuts", async () => {
      await wheelKind("Press shortcuts");
      await record(".wheel-record:nth-child(1) .record-box", ["\uE009"], "k");
      await configShows("Shuttle left Ctrl+K", (c) => JSON.stringify(c.jog.per_mode.Shuttle.left) === JSON.stringify({ Hotkey: { combo: { modifiers: ["Ctrl"], key: "K" } } }));
      await record(".wheel-record:nth-child(2) .record-box", ["\uE009"], "j");
      await configShows("Shuttle right Ctrl+J", (c) => c.jog.per_mode.Shuttle.right.Hotkey.combo.key === "J");
      await press("SHTL");
      await turn("left");
      await logged("would press Ctrl+K");
    }, ["settings", "simulator"]);
    await step("Wheel: SHTL scrolls again", async () => {
      await wheelKind("Scroll up and down");
      await configShows("Shuttle scrolls", (c) => c.jog.per_mode.Shuttle.left.Scroll.amount === -1);
    });
    await step("Wheel: sensitivity slider slow, then fast", async () => {
      await typeInto("settings", 'input[type=range][aria-label="Sensitivity"]', "\uE011");
      await configShows("threshold 10 (slow)", (c) => c.jog.per_mode[c.jog.default_mode].threshold === 10);
      await typeInto("settings", 'input[type=range][aria-label="Sensitivity"]', "\uE010");
      await configShows("threshold 1 (fast)", (c) => c.jog.per_mode[c.jog.default_mode].threshold === 1);
    });

    await tab("Keys", "What each key does");
    await step("Keys: List view, then Drawing", async () => {
      await click("settings", ".segmented button[data-view=List]");
      await waitFor("settings", "the list", `return document.querySelector("table.button-list")`);
      await click("settings", ".segmented button[data-view=Drawing]");
      await waitFor("settings", "the drawing", `return document.querySelector(".device [data-binding]")`);
    });
    await step("Keys: IN records Ctrl+Shift+K", async () => {
      await keyIn("In");
      await choice("Press a shortcut");
      await record(".side-panel .record-box", ["\uE009", "\uE008"], "k");
      await configShows("IN Ctrl+Shift+K", (c) => JSON.stringify(basePage(c).bindings.In) === JSON.stringify({ Hotkey: { combo: { modifiers: ["Ctrl", "Shift"], key: "K" } } }));
      await waitFor("settings", "Saved", `return document.querySelector(".panel-try.is-saved")`);
    });
    await step("Keys: IN presses the Play/Pause media key", async () => {
      await click("settings", ".side-panel button.preset", "Play/Pause");
      await configShows("IN PlayPause", (c) => basePage(c).bindings.In.Hotkey.combo.key === "PlayPause");
      await press("IN");
      await logged("would press PlayPause");
      await click("settings", ".side-panel button.preset", "Mute");
      await configShows("IN Mute", (c) => basePage(c).bindings.In.Hotkey.combo.key === "Mute");
      await press("IN");
      await logged("would press Mute");
    }, ["settings", "simulator"]);
    await step("Keys: IN holds Shift (Quick picks)", async () => {
      await keyIn("In");
      await toChoices();
      await choice("Hold a key while I hold this one");
      await click("settings", ".side-panel button.preset", "Shift");
      await configShows("IN holds Shift", (c) => JSON.stringify(basePage(c).bindings.In) === JSON.stringify({ Held: { combo: { modifiers: ["Shift"], key: "" } } }));
      await press("IN");
      await logged("would release Shift");
    }, ["settings", "simulator"]);
    await step("Keys: IN turns Ctrl on and off (Quick picks)", async () => {
      await allChoices();
      await choice("Turn a key on and off");
      await click("settings", ".side-panel button.preset", "Ctrl");
      await configShows("IN latches Ctrl", (c) => basePage(c).bindings.In.Latched.combo.modifiers[0] === "Ctrl");
    });
    await step("Keys: IN turns the wheel into a volume knob; Wheel tab: Back to normal", async () => {
      await allChoices();
      await choice("Turn the wheel into");
      await click("settings", ".side-panel button.option-row", "System volume");
      await configShows("IN volume knob", (c) => basePage(c).bindings.In.OverrideToggle.mode === "SystemVolume");
      await press("IN");
      await click("settings", "nav.toolbar button.tab", "Wheel");
      await waitFor("settings", "the override notice", `return document.querySelector(".override-notice")?.textContent.includes("IN has turned the wheel into a volume knob")`);
      await turn("right");
      await logged("would turn volume up");
      await click("settings", ".override-notice button", "Back to normal");
      await gone("settings", ".override-notice");
      await logged("the wheel is back to normal");
      await click("settings", "nav.toolbar button.tab", "Keys");
    }, ["settings", "simulator"]);
    await step(`Keys: the app picker shows real app icons (${P.knownIcon.source.replace(/[^A-Za-z0-9 ]/g, "")})`, async () => {
      await keyIn("In");
      await toChoices();
      await choice("Open an app or website");
      // Each row's icon is the shell's own (src-tauri app_icons.rs): a PNG data URL in an <img>.
      const found = await waitFor("settings", "a real app icon for a known app", `const rows = [...document.querySelectorAll(".side-panel .app-row")];
        const real = (r) => r.querySelector("img.app-icon")?.getAttribute("src")?.startsWith("data:image/png;base64,iVBORw0KGgo");
        const withIcon = rows.filter(real);
        const [known_, others] = [...arguments].map((s) => new RegExp(s, "i"));
        const known = withIcon.find((r) => known_.test(r.textContent)) ?? withIcon.find((r) => others.source !== "^$" && others.test(r.textContent));
        if (!known) return null;
        known.scrollIntoView({ block: "center" });
        return JSON.stringify({ known: known.textContent.trim(), withIcon: withIcon.length, rows: rows.length, letters: rows.length - withIcon.length });`, [P.knownIcon.source, P.otherIcons.source], 40000);
      say(`  app icons: ${found}`);
      const { withIcon, rows } = JSON.parse(found);
      if (withIcon < rows * P.iconShare) throw new Error(`only ${withIcon} of ${rows} apps have a real icon`);
    });
    await step("Keys: IN opens an installed app (search, pick)", async () => {
      await keyIn("In");
      await toChoices();
      await choice("Open an app or website");
      const first = await waitFor("settings", "the installed apps", `return document.querySelector(".side-panel .app-row .grow")?.textContent.trim()`, [], 20000);
      say(`  first app: ${first}`);
      await typeInto("settings", '.side-panel input[aria-label="Search your apps"]', first.slice(0, 4));
      await waitFor("settings", "the search to filter", `const q = arguments[0].toLowerCase();
        const rows = [...document.querySelectorAll(".side-panel .app-row .grow")];
        return rows.length > 0 && rows.every((r) => r.textContent.toLowerCase().includes(q));`, [first.slice(0, 4)]);
      await typeInto("settings", '.side-panel input[aria-label="Search your apps"]', `${SELECT_ALL}zzzz-no-such-app`);
      await waitFor("settings", "no apps match", `return document.querySelector(".side-panel .app-none")?.textContent.includes("No apps match")`);
      await typeInto("settings", '.side-panel input[aria-label="Search your apps"]', `${SELECT_ALL}\uE003`);
      await click("settings", ".side-panel .app-row", first);
      await configShows("IN opens an installed app", (c) => P.installedApp(basePage(c).bindings.In.LaunchApp));
      await press("IN");
      await lineWith("would run", P.installedLaunch);
    }, ["settings", "simulator"]);
    await step("Keys: IN opens a chosen file (Choose file…)", async () => {
      await click("settings", ".side-panel button.choose-file", "Choose file");
      await configShows("IN opens the chosen program", (c) => basePage(c).bindings.In.LaunchApp.path.endsWith(P.chosen));
      await logged("Chose the app E2E Example App");
      await press("IN");
      await lineWith("would run", P.chosen);
    }, ["settings", "simulator"]);
    await step("Keys: IN goes to a new page Music", async () => {
      await allChoices();
      await choice("Switch to another set of keys");
      await click("settings", ".side-panel button.suggestion", "Music");
      await configShows("a Music page IN goes to", (c) => {
        const music = c.layers.find((l) => l.name === "Music");
        return music && basePage(c).bindings.In.LayerPush.target === music.id;
      });
      await waitFor("settings", "the page pills", `return document.querySelectorAll(".pills .pill").length === 2`);
    });
    await step("Keys: on Music, IN goes back (Show the page)", async () => {
      await click("settings", ".pill button[role=tab]", "Music");
      await keyIn("In");
      await waitFor("settings", "the back card", `return document.querySelector(".side-panel .back-card")`);
      await click("settings", ".side-panel button", "Show the ");
      await waitFor("settings", "the first page", `return document.querySelector(".pill.is-active")?.textContent.includes(arguments[0])`, [basePage(await readConfig()).name]);
    });
    await step("Keys: add a page, rename it, delete it", async () => {
      await click("settings", "button.pill-add", "Add page");
      await shows("settings", "To reach this page");
      const added = await configShows("a third page", (c) => c.layers.length === 3);
      const id = added.layers[2].id;
      await click("settings", '.pill.is-active button[aria-label="Rename"]');
      await typeInto("settings", ".pill.is-active input.pill-rename", `${SELECT_ALL}Mixer\uE007`);
      await configShows("the page renamed Mixer", (c) => c.layers.find((l) => l.id === id)?.name === "Mixer");
      await click("settings", ".pill.is-active button.pill-icon", "Delete page");
      await click("settings", ".confirm-inline button", "Delete");
      await configShows("Mixer deleted", (c) => !c.layers.some((l) => l.id === id));
    });
    await step("Keys: IN does nothing", async () => {
      await keyIn("In");
      await toChoices();
      await choice("Do nothing");
      await configShows("IN cleared", (c) => !("In" in basePage(c).bindings));
    });
    await step("Keys: SHTL is a wheel button: Set up the wheel", async () => {
      await keyIn("Shtl");
      await click("settings", ".side-panel button", "Set up the wheel");
      await shows("settings", "When I turn the wheel");
      await click("settings", "nav.toolbar button.tab", "Keys");
    });
    await step("Keys: close the key panel", async () => {
      await keyIn("Cut");
      await click("settings", '.side-panel button[aria-label="Close"]');
      await waitFor("settings", "Pick a key", `return document.querySelector(".side-panel .panel-empty")`);
    });

    await step("Toolbar: switch Off, then the notice's Turn on", async () => {
      await click("settings", 'button.switch[aria-label="Turn freewheel on or off"]');
      await waitFor("settings", "Off", `return document.querySelector("button.status")?.getAttribute("aria-label") === "Off: Resolve can use it"`);
      await click("settings", ".notice-off button", "Turn on");
      await waitFor("settings", "On again", `return document.querySelector("button.status")?.textContent.includes("Connected")`);
    }, ["settings", "simulator"]);

    // ---- The Simulator window's remaining controls ---------------------
    await step("simulator: pick SNAP and Press", async () => {
      await click("simulator", 'select[aria-label="Key to press"] option[value="Snap"]');
      await click("simulator", ".fw-sim-line button.fw-sim-primary", "Press");
      await logged("You pressed SNAP");
    }, ["simulator", "settings"]);
    await sim("Computer appearance", "Dark");
    await sim("Computer appearance", "Light");
    await step("simulator: Bluetooth 12% shows the low battery notice", async () => {
      await click("simulator", `[role=group][aria-label="Speed Editor"] button`, "Bluetooth 12%");
      await shows("settings", "Battery is low (12%).");
      await click("simulator", `[role=group][aria-label="Speed Editor"] button`, "USB");
    }, ["simulator", "settings"]);
    await step("simulator: Keys, empty setup", async () => {
      await click("simulator", `[role=group][aria-label="Keys"] button`, "Empty setup");
      await logged("empty setup loaded");
      await configShows("no keys", (c) => c.layers.every((l) => Object.keys(l.bindings).length === 0));
    }, ["simulator", "settings"]);
    await step("simulator: Keys, sample setup with media keys", async () => {
      await click("simulator", `[role=group][aria-label="Keys"] button`, "Several set");
      await logged("sample mappings loaded");
      await configShows("STOP/PLAY Play/Pause, AUDIO ONLY Mute", (c) => basePage(c).bindings.StopPlay.Hotkey.combo.key === "PlayPause" && basePage(c).bindings.AudioOnly.Hotkey.combo.key === "Mute");
      await waitFor("settings", "the drawing's Play/Pause", `return document.querySelector('.device [data-binding="StopPlay"]')?.getAttribute("title") === "Play/Pause"`, [], 15000);
      await click("simulator", 'select[aria-label="Key to press"] option[value="StopPlay"]');
      await click("simulator", ".fw-sim-line button.fw-sim-primary", "Press");
      await logged("would press PlayPause");
      await press("FULL VIEW");
      await click("settings", "nav.toolbar button.tab", "Wheel");
      await waitFor("settings", "the override notice", `return document.querySelector(".override-notice")?.textContent.includes("FULL VIEW has turned the wheel into a volume knob")`);
      await click("settings", ".override-notice button", "Back to normal");
      await gone("settings", ".override-notice");
      await click("settings", "nav.toolbar button.tab", "Keys");
    }, ["simulator", "settings"]);
    await step("LEDs: the wheel button and a latched TRANS light in the drawing and the Simulator", async () => {
      // As the device: the drawing's lit LEDs (status.lit) and the Simulator's lit quick keys.
      const drawingLit = (keys) =>
        waitFor("settings", `the drawing lights ${keys}`, `const want = [...arguments[0]].sort().join();
          const lit = [...document.querySelectorAll(".keys-tab .device .key.is-lit")];
          const got = [...new Set(lit.map((k) => k.dataset.binding))].sort().join();
          const fills = [...new Set(lit.map((k) => getComputedStyle(k.querySelector(".key-led")).fill.replace(/ /g, "")))].join();
          if (got === want && fills === "rgb(255,181,71)") return true;
          throw new Error("lit " + got + " fill " + fills);`, [keys]);
      const simLit = (labels) =>
        waitFor("simulator", `the Simulator lights ${labels}`, `return [...document.querySelectorAll(".fw-sim-quick button.is-lit")].map((b) => b.textContent).join() === arguments[0].join()`, [labels]);
      await press("SCRL");
      await drawingLit(["Scrl"]);
      await simLit(["SCRL"]);
      await press("JOG");
      await drawingLit(["Jog"]);
      await simLit(["JOG"]);
      await keyIn("Trans");
      await toChoices();
      await choice("Turn a key on and off");
      await click("settings", ".side-panel button.preset", "Ctrl");
      await configShows("TRANS latches Ctrl", (c) => basePage(c).bindings.Trans?.Latched?.combo.modifiers[0] === "Ctrl");
      await press("TRANS");
      await drawingLit(["Trans", "Jog"]);
      await simLit(["TRANS", "JOG"]);
      await press("TRANS");
      await drawingLit(["Jog"]);
      await press("SCRL");
      await drawingLit(["Scrl"]);
      await simLit(["SCRL"]);
    }, ["settings", "simulator"]);
    await step("simulator: macOS permission missing, tray's Needs permission line", async () => {
      await click("simulator", `[role=group][aria-label="macOS permission"] button`, "Missing");
      await shows("settings", "freewheel needs your permission.");
      await tray("status");
      await logged("Tray: ");
      await waitFor("settings", "the permission fix", `return document.querySelector(".setup")`);
      await click("simulator", `[role=group][aria-label="macOS permission"] button`, "Allowed");
      // The Simulator's Record a key leaves the permission fix.
      await jumpTo("Record a key");
      await waitFor("settings", "the settings tabs", `return document.querySelector("nav.toolbar")`);
    }, ["simulator", "settings"]);
    await step("simulator: Record a key opens CUT recording", async () => {
      await jumpTo("Record a key");
      await logged("Recording a shortcut for CUT");
      await waitFor("settings", "CUT's panel recording", `return document.querySelector(".side-panel .panel-title")?.textContent.includes("CUT") && document.querySelector(".side-panel .mode-title")?.textContent.includes("Press a shortcut")`);
    }, ["simulator", "settings"]);
    await step("simulator: Reload settings", async () => {
      await jumpTo("Reload settings");
      await logged("Settings window reloaded");
      await waitFor("settings", "the reloaded window", `return document.readyState === "complete" && document.querySelector("nav.toolbar")`, [], 20000);
    }, ["simulator", "settings"]);
    await step("simulator: Settings window", () => jumpTo("Settings window"), ["simulator", "settings"]);
    await step("simulator: First run, then through it again", async () => {
      await jumpTo("First run");
      await logged("First run opened");
      await click("settings", "button.get-started");
      await waitFor("settings", "Found it!", `return document.querySelector(".found")`, [], 20000);
      await click("settings", ".first-run-buttons button.btn-primary", "Continue");
      await waitFor("settings", "the next screen", `return document.querySelector("nav.toolbar") || [...document.querySelectorAll("button")].some((b) => b.textContent.includes("Start using freewheel"))`);
      if (!(await d.exec(`return !!document.querySelector("nav.toolbar")`))) await click("settings", "button", "Start using freewheel");
      await waitFor("settings", "the settings tabs", `return document.querySelector("nav.toolbar")`);
    }, ["simulator", "settings"]);
    await step("simulator: Do actions for real on, then off", async () => {
      await setReal(true);
      await setReal(false);
    }, ["simulator"]);
    /** The window's switch, the tray's check (the Simulator's Tray menu) and get_status all read `on`. */
    const allRead = async (on, by) => {
      await waitFor("settings", `the window's switch ${on ? "On" : "Off"} (${by})`, `const on = arguments[0];
        const sw = document.querySelector('button.switch[aria-label="Turn freewheel on or off"]');
        const word = document.querySelector(".toolbar .switch-text")?.textContent;
        if (sw?.getAttribute("aria-checked") === String(on) && word === (on ? "On" : "Off")) return true;
        throw new Error("switch " + sw?.getAttribute("aria-checked") + ", " + word);`, [on]);
      await waitFor("simulator", `the tray's check ${on ? "on" : "off"} (${by})`, `const on = arguments[0];
        const item = document.querySelector('[data-tray="toggle"]');
        if (item?.getAttribute("aria-pressed") === String(on) && item.textContent.includes(on ? "freewheel is On" : "freewheel is Off")) return true;
        throw new Error("tray " + item?.getAttribute("aria-pressed") + ", " + item?.textContent);`, [on]);
      await waitAsk("settings", `get_status enabled ${on} (${by})`, `return (await window.__TAURI__.core.invoke("get_status")).enabled;`, on);
      say(`  ${by}: the window's switch, the tray's check and get_status are all ${on ? "On" : "Off"}`);
    };
    const windowSwitch = () => click("settings", 'button.switch[aria-label="Turn freewheel on or off"]');
    await step("tray: freewheel is On / Off", async () => {
      await tray("toggle");
      await logged("Tray: freewheel is");
      await waitFor("settings", "Off", `return document.querySelector("button.status")?.getAttribute("aria-label") === "Off: Resolve can use it"`);
      await waitFor("simulator", "the tray's Off", `return document.querySelector('[data-tray="toggle"]')?.textContent.includes("Off")`);
      await allRead(false, "tray Off");
      await tray("toggle");
      await waitFor("settings", "On", `return document.querySelector("button.status")?.textContent.includes("Connected")`);
      await allRead(true, "tray On");
    }, ["simulator", "settings"]);
    await step("On/Off: the window's switch moves the tray's check", async () => {
      await windowSwitch();
      await allRead(false, "window switch Off");
      await windowSwitch();
      await allRead(true, "window switch On");
      await waitFor("settings", "Connected", `return document.querySelector("button.status")?.textContent.includes("Connected")`);
    }, ["simulator", "settings"]);
    await step("On/Off: tray Off, window On, window Off, tray On", async () => {
      await tray("toggle");
      await allRead(false, "tray Off");
      await windowSwitch();
      await allRead(true, "window On after the tray's Off");
      await windowSwitch();
      await allRead(false, "window Off");
      await tray("toggle");
      await allRead(true, "tray On after the window's Off");
    }, ["simulator", "settings"]);

    // ---- The header at the narrowest window: the status never overlaps -
    const OUT_HEADER = (name) => join(out, `header-${name}.png`);
    /** Resizes the settings window (from the Simulator, which may) to `width` x 640 and waits for it. */
    const resize = async (width) => {
      const r = await ask("simulator", `const W = window.__TAURI__.window;
        await new W.Window("main", { skip: true }).setSize(new W.LogicalSize(args[0], 640)); return "ok";`, [width]);
      if (r !== "ok") throw new Error(`resizing the settings window: ${r}`);
      await waitFor("settings", `a ${width}px window`, `if (innerWidth === arguments[0]) return true; throw new Error("innerWidth " + innerWidth);`, [width]);
    };
    /** Sets the window's look (as General's appearance does, without saving it). */
    const look = (appearance) =>
      d.exec(`document.documentElement.style.colorScheme = arguments[0] === "Dark" ? "dark" : "light"; document.documentElement.dataset.appearance = arguments[0]; return true;`, [appearance]);
    /** Checks the header (HEADER_JS); throws with what overlapped. */
    const headerOk = async (what) => {
      await use("settings");
      const r = await d.exec(HEADER_JS);
      say(`  header ${what}: ${JSON.stringify(r)}`);
      if (r.bad.length) throw new Error(`header ${what}: ${r.bad.join("; ")} ${JSON.stringify(r)}`);
      return r;
    };
    const headerShot = async (name) => {
      await use("settings");
      const [el] = await d.find("nav.toolbar");
      writeFileSync(OUT_HEADER(name), Buffer.from(await d.elementShot(el), "base64"));
      say(`  screenshot header-${name}.png`);
    };
    await step("header: Looking for your Speed Editor… at 820px never overlaps", async () => {
      await click("simulator", `[role=group][aria-label="Speed Editor"] button`, "Not found");
      await waitFor("settings", "Looking", `return document.querySelector(".toolbar .status")?.getAttribute("aria-label") === "Looking for your Speed Editor…"`);
      await resize(820);
      for (const appearance of ["Light", "Dark"]) {
        await look(appearance);
        await headerOk(`820px ${appearance}`);
        await headerShot(`820-${appearance.toLowerCase()}`);
      }
      await look("Light");
      // Longer than any status there is: the text gives way, nothing moves.
      await use("settings");
      const long = "Looking for your Speed Editor… ".repeat(6);
      const before = await d.exec(`const t = document.querySelector(".toolbar .status-text").firstChild; const was = t.nodeValue; t.nodeValue = arguments[0]; return was;`, [long]);
      for (const width of [820, 900, 1000]) {
        await resize(width);
        await headerOk(`${width}px, ${long.length} characters`);
      }
      await use("settings");
      await d.exec(`document.querySelector(".toolbar .status-text").firstChild.nodeValue = arguments[0]; return true;`, [before]);
    }, ["settings"]);
    /** At 1000px the header shows `short` whole (no ellipsis), the full sentence as its tooltip. */
    const wholeAt1000 = async (short, full) => {
      const r = await headerOk(`1000px ${short}`);
      if (r.shown !== short || r.label !== full) throw new Error(`the header says ${JSON.stringify(r.shown)} (${r.label}), not ${short} (${full})`);
      if (r.cut || r.text === 0) throw new Error(`at 1000px "${short}" is cut or hidden: ${JSON.stringify(r)}`);
    };
    await step("header: every status at 820px and 1000px, light and dark", async () => {
      for (const [device, label, short] of [
        ["USB", "Connected by USB", "Connected · USB"],
        ["Bluetooth 64%", "Connected by Bluetooth · 64%", "Bluetooth · 64%"],
        ["Not found", "Looking for your Speed Editor…", "Looking for device"],
      ]) {
        await click("simulator", `[role=group][aria-label="Speed Editor"] button`, device);
        await waitFor("settings", label, `return document.querySelector(".toolbar .status")?.getAttribute("aria-label") === arguments[0]`, [label]);
        await resize(820);
        await headerOk(`820px ${label}`);
        await resize(1000);
        await wholeAt1000(short, label);
        if (device === "Not found")
          for (const appearance of ["Light", "Dark"]) {
            await look(appearance);
            await headerShot(`1000-looking-${appearance.toLowerCase()}`);
          }
        await look("Light");
      }
      // The widest a battery gets: 100%.
      await click("simulator", `[role=group][aria-label="Speed Editor"] button`, "Bluetooth 64%");
      await waitFor("settings", "Bluetooth", `return document.querySelector(".toolbar .status-text")?.textContent === "Bluetooth · 64%"`);
      await use("settings");
      await d.exec(`document.querySelector(".toolbar .status-text").firstChild.nodeValue = "Bluetooth · 100%"; return true;`);
      const widest = await headerOk("1000px Bluetooth · 100%");
      if (widest.cut) throw new Error(`at 1000px "Bluetooth · 100%" is cut: ${JSON.stringify(widest)}`);
      await click("simulator", `[role=group][aria-label="Speed Editor"] button`, "USB");
      await waitFor("settings", "Connected by USB", `return document.querySelector(".toolbar .status-text")?.textContent === "Connected · USB"`);
      // Resolve open: Paused; the permission missing: Needs permission.
      await click("simulator", `[role=group][aria-label="DaVinci Resolve"] button`, "Open");
      await waitFor("settings", "Paused", `return document.querySelector(".toolbar .status")?.getAttribute("aria-label") === "Paused: DaVinci Resolve is open"`);
      await wholeAt1000("Paused for Resolve", "Paused: DaVinci Resolve is open");
      await click("simulator", `[role=group][aria-label="DaVinci Resolve"] button`, "Closed");
      await click("simulator", `[role=group][aria-label="macOS permission"] button`, "Missing");
      await waitFor("settings", "Needs permission", `return document.querySelector(".toolbar .status")?.getAttribute("aria-label") === "Needs permission (click to fix)"`);
      await wholeAt1000("Needs permission", "Needs permission (click to fix)");
      await click("simulator", `[role=group][aria-label="macOS permission"] button`, "Allowed");
      await waitFor("settings", "Connected by USB", `return document.querySelector(".toolbar .status")?.getAttribute("aria-label") === "Connected by USB"`);
      for (const appearance of ["Light", "Dark"]) {
        await look(appearance);
        await headerShot(`1000-${appearance.toLowerCase()}`);
      }
      await look("Light");
      await windowSwitch();
      await allRead(false, "window switch Off");
      await wholeAt1000("Off", "Off: Resolve can use it");
      await resize(820);
      await headerOk("820px Off");
      await windowSwitch();
      await allRead(true, "window switch On");
      await waitFor("settings", "Connected by USB", `return document.querySelector(".toolbar .status")?.getAttribute("aria-label") === "Connected by USB"`);
      await resize(1000);
    }, ["settings", "simulator"]);
    await step("tray: Open settings", async () => {
      await tray("open-settings");
      await logged("Tray: Open settings");
    }, ["simulator", "settings"]);
    await step("tray: Start at login on and off", async () => {
      await tray("start-at-login");
      await logged("would turn start at login on");
      await waitFor("simulator", "checked", `return document.querySelector('[data-tray="start-at-login"]')?.getAttribute("aria-pressed") === "true"`);
      await configShows("start_at_login on (tray)", (c) => c.settings.start_at_login === true);
      await tray("start-at-login");
      await logged("would turn start at login off");
      await configShows("start_at_login off (tray)", (c) => c.settings.start_at_login === false);
    }, ["simulator", "settings"]);
    await step("tray: Download logs for support opens Help & backup with the file", async () => {
      await click("settings", "nav.toolbar button.tab", "Keys");
      await tray("download-logs");
      await logged("Tray: Download logs");
      await logged("Saved the logs to", 20000);
      await waitFor("settings", "Help & backup's Downloaded line", `return document.querySelector('nav.toolbar button.tab[aria-pressed="true"]')?.textContent.includes("Help") && document.querySelector("#logs .success-line")?.textContent.includes("to your Downloads folder")`, [], 20000);
    }, ["simulator", "settings"]);
    const trayItem = async (id) => (await use("simulator"), d.exec(`const b = document.querySelector('[data-tray="' + arguments[0] + '"]'); return b && { text: b.textContent, disabled: b.getAttribute("aria-disabled") === "true" };`, [id]));
    const trayShows = (id, what, check) =>
      waitFor("simulator", `the tray's ${id} item ${what}`, `const b = document.querySelector('[data-tray="' + arguments[0] + '"]');
        b?.closest("section")?.scrollIntoView({ block: "center" });
        if (!b) return null;
        const ok = ${check};
        // The state now, or one it took since the last tray click (TRAY_WATCH_JS).
        const seen = [...(window.__fwTraySeen?.[arguments[0]] ?? []), [b.textContent, b.getAttribute("aria-disabled") === "true"]];
        const hit = seen.find(([t, off]) => ok(t, off));
        if (hit && window.__fwTraySeen?.[arguments[0]]) window.__fwTraySeen[arguments[0]] = window.__fwTraySeen[arguments[0]].slice(seen.indexOf(hit) + 1);
        return hit ? hit[0] : null;`, [id], 20000);
    const trayIconIs = (text) =>
      waitFor("simulator", `the tray icon ${text}`, `return document.querySelector('.fw-sim-tray-item[aria-current="true"]')?.textContent.includes(arguments[0])`, [text]);
    await step("tray: the status line follows the device; a disabled line does nothing", async () => {
      await click("simulator", `[role=group][aria-label="Speed Editor"] button`, "USB");
      await trayShows("status", "Connected by USB (disabled)", `(t, off) => t === "Connected by USB" && off`);
      await trayIconIs("On");
      await tray("status");
      await logged("Tray: Connected by USB (disabled, nothing happens)");
    }, ["simulator"]);
    await step("tray: Bluetooth 12% needs attention and notifies low battery", async () => {
      await click("simulator", `[role=group][aria-label="Speed Editor"] button`, "Bluetooth 64%");
      await trayShows("status", "Bluetooth · 64%", `(t) => t === "Connected by Bluetooth · 64%"`);
      await click("simulator", `[role=group][aria-label="Speed Editor"] button`, "Bluetooth 12%");
      await trayShows("status", "Bluetooth · 12%", `(t) => t === "Connected by Bluetooth · 12%"`);
      await trayIconIs("Needs attention");
      await logged("Notification: Speed Editor battery low — 12% left.");
      await click("simulator", `[role=group][aria-label="Speed Editor"] button`, "USB");
    }, ["simulator"]);
    await step("tray: Resolve opens, Paused, stepped-aside notification", async () => {
      await click("simulator", `[role=group][aria-label="DaVinci Resolve"] button`, "Open");
      await trayShows("status", "Paused", `(t) => t === "Paused: DaVinci Resolve is open"`);
      await trayIconIs("Off / paused");
      await logged("Notification: freewheel — DaVinci Resolve started, so freewheel switched Off.");
      await click("simulator", `[role=group][aria-label="DaVinci Resolve"] button`, "Closed");
      await trayShows("status", "connected again", `(t) => t.startsWith("Connected")`);
    }, ["simulator", "settings"]);
    await step("tray: Check for updates, up to date", async () => {
      await trayShows("update", "Check for updates", `(t, off) => !off && /Check/.test(t)`);
      await trayClick("update", "");
      await trayShows("update", "Checking (disabled)", `(t, off) => t === "Checking for updates…" && off`);
      await shows("settings", "You’re up to date.");
      await trayShows("update", "up to date", `(t, off) => t === "You’re up to date · Check again" && !off`);
    }, ["simulator", "settings"]);
    await step("tray: a new version downloads by itself, the tray says Restart to update and notifies", async () => {
      await click("simulator", ".fw-sim-grid button", "New version");
      await trayShows("update", "Downloading (disabled)", `(t, off) => t === "Downloading update…" && off`);
      await trayShows("update", "Restart to update", `(t, off) => t === "Restart to update" && !off`);
      await lineWith("Notification", "is ready");
    }, ["simulator", "settings"]);
    await step("tray: Restart to update installs it", async () => {
      await tray("update");
      await logged("Tray: Restart to update");
      await waitFor("settings", "the restart to the new version", `return /Updated to freewheel|freewheel is updated to/.test(document.body.innerText)`, [], 20000);
      await trayShows("update", "Check for updates again", `(t, off) => t === "Check for updates" && !off`);
      say(`  tray after the update: ${JSON.stringify(await trayItem("update"))}`);
    }, ["simulator", "settings"]);

    // ---- Do actions for real: a press really runs what CUT is set to ----
    // A program in the dialogs folder leaves a marker file named after its
    // argument; a website goes to a local server that records the hit.
    const marker = (name) => join(dialogs, `marker-${name}.txt`);
    // Windows: a .cmd; Linux: a shell script you may run. Websites open
    // through explorer.exe / xdg-open.
    const markName = platform === "windows" ? "fw-mark.cmd" : "fw-mark.sh";
    const markCmd = join(dialogs, markName);
    if (platform === "windows") writeFileSync(markCmd, `@echo off\r\necho ran> "${dialogs}\\marker-%~1.txt"\r\n`);
    else writeFileSync(markCmd, `#!/bin/sh\necho ran > "${dialogs}/marker-$1.txt"\n`, { mode: 0o755 });
    const opener = platform === "windows" ? "explorer.exe" : "xdg-open";
    const markApp = (arg) => ({ LaunchApp: { path: markCmd, args: [arg], enabled: true } });
    const setCut = async (action) => {
      const cur = await readConfig();
      await saveConfig({ ...cur, layers: cur.layers.map((l) => (l.id === cur.base_layer ? { ...l, bindings: { ...l.bindings, Cut: action } } : l)) });
      await configShows("CUT set", (c) => JSON.stringify(basePage(c).bindings.Cut) === JSON.stringify(action));
    };
    const waitFile = async (file, ms) => {
      const end = Date.now() + ms;
      while (Date.now() < end) {
        if (existsSync(file)) return true;
        await sleep(200);
      }
      return false;
    };
    await step("Do actions for real on: pressing CUT runs its program for real", async () => {
      await setCut(markApp("on"));
      await setReal(true);
      await press("CUT");
      if (!(await waitFile(marker("on"), 15000))) throw new Error(`CUT did not run ${markCmd}: no ${marker("on")}`);
      await lineWith("launched \"fw-mark\" ok", markName);
      say(`  ${marker("on")}: written by the press`);
    }, ["simulator", "settings"]);
    await step("Do actions for real off: pressing CUT runs nothing and says so", async () => {
      await setReal(false);
      await setCut(markApp("off"));
      await press("CUT");
      await lineWith("not done: Do actions for real is off", "would run", markName);
      if (await waitFile(marker("off"), 3000)) throw new Error(`CUT ran ${markCmd} although Do actions for real is off`);
    }, ["simulator", "settings"]);
    // A website opens through explorer.exe with the address, as the key
    // panel saves it. A hosted runner may have no default browser set up,
    // so the hit is reported, not required; the launch line is.
    const hits = [];
    const server = createServer((req, res) => (hits.push(req.url), res.end("freewheel e2e")));
    await new Promise((r) => server.listen(0, "127.0.0.1", r));
    const site = `http://127.0.0.1:${server.address().port}/fw-e2e-hit`;
    await step(`Do actions for real on: a website key opens it through ${opener}`, async () => {
      await setCut({ LaunchApp: { path: opener, args: [site], enabled: true } });
      await setReal(true);
      await press("CUT");
      await lineWith("launched 127.0.0.1", opener);
      const end = Date.now() + 20000;
      while (!hits.length && Date.now() < end) await sleep(250);
      say(`  website hit: ${hits.length ? hits.join(", ") : "none within 20 s (no default browser on the runner?)"}`);
      await setReal(false);
    }, ["simulator", "settings"]);
    server.close();
    await step("Download logs: the bundle has each press, mapping, result and the per-key config", async () => {
      const path = await ask("settings", `return await window.__TAURI__.core.invoke("export_logs");`);
      if (typeof path !== "string" || path.startsWith("error")) throw new Error(`export_logs: ${path}`);
      const text = readFileSync(path, "utf8");
      writeFileSync(join(out, "log-bundle.txt"), text);
      // The log bundle writes the home folder as "~" (paths stay private when shared).
      const inBundle = (p) => p.split(homedir()).join("~");
      const want = [
        `simulated device: key CUT pressed on page "${basePage(await readConfig()).name}" -> Open app "fw-mark" (`,
        `launched "fw-mark" ok: ${inBundle(markCmd)} "on"`,
        `not done: Do actions for real is off (would run ${inBundle(markCmd)} "off")`,
        "simulator: Do actions for real is on: presses send keys, open apps and change the volume on this PC",
        `config saved: CUT: Open app "fw-mark" -> Open website 127.0.0.1`,
        "-> Open website 127.0.0.1",
        `launched 127.0.0.1:${new URL(site).port} ok: ${opener} "http://127.0.0.1:${new URL(site).port}/…"`,
        "simulated device: wheel turned",
        "== config ==",
        "  CUT -> Open website 127.0.0.1",
      ];
      const missing = want.filter((w) => !text.includes(w));
      if (missing.length) throw new Error(`the log bundle lacks: ${JSON.stringify(missing)}`);
      for (const line of text.split(/\r?\n/).filter((l) => /simulated device: |launched |not done: |config saved: |^ {2}[A-Z0-9 /]+ -> /.test(l)).slice(-40)) say(`  bundle: ${line}`);
    }, ["settings"]);
    await setCut({ Hotkey: { combo: { modifiers: ["Ctrl"], key: "C" } } });

    // ---- Backups, restores and imports prove the data ------------------
    let backedUp;
    // A version-2 backup names programs by path, and a restore keeps a path
    // on only when it exists (SCHEMA.md "How open finds the app").
    const roundTrip = resolve(dialogs, "Round Trip.exe");
    // Linux: a program is a file you may run (else open would hand it to xdg-open).
    writeFileSync(roundTrip, platform === "windows" ? "not really a program" : "#!/bin/sh\n", { mode: 0o755 });
    await step("data: a distinctive setup on two pages", async () => {
      const cur = await readConfig();
      await saveConfig({
        ...cur,
        layers: [
          {
            id: cur.base_layer,
            name: basePage(cur).name,
            bindings: {
              In: { Hotkey: { combo: { modifiers: ["Ctrl", "Shift"], key: "S" } } },
              Cut: { LaunchApp: { path: roundTrip, args: ["--fw"], enabled: true } },
              Trans: { Held: { combo: { modifiers: ["Shift"], key: "" } } },
              Snap: { Latched: { combo: { modifiers: ["Alt"], key: "" } } },
              Cam5: { LayerPush: { target: "studio" } },
              Shtl: { SetJogMode: { mode: "Volume" } },
              FullView: { OverrideToggle: { mode: "Brightness" } },
            },
          },
          { id: "studio", name: "Studio", bindings: { Cut: { Hotkey: { combo: { modifiers: [], key: "PlayPause" } } }, Out: { Hotkey: { combo: { modifiers: ["Ctrl"], key: "O" } } } } },
        ],
        jog: {
          default_mode: "Jog",
          per_mode: {
            Jog: { left: { Hotkey: { combo: { modifiers: [], key: "Left" } } }, right: { Hotkey: { combo: { modifiers: [], key: "Right" } } }, threshold: 3, shuttle_max_rate_hz: null },
            Scroll: { left: { Scroll: { amount: -1 } }, right: { Scroll: { amount: 1 } }, threshold: 2, shuttle_max_rate_hz: null },
            Shuttle: { left: null, right: null, threshold: 1, shuttle_max_rate_hz: 8 },
          },
        },
        overrides: { ...cur.overrides, system_volume_step: 0.05, app_volume_step: 0.03, brightness_step: 7 },
      });
      // The Keys tab reads the config afresh; the second page is renamed by hand.
      await click("settings", "nav.toolbar button.tab", "Wheel");
      await click("settings", "nav.toolbar button.tab", "Keys");
      await click("settings", ".pill button[role=tab]", "Studio");
      await click("settings", '.pill.is-active button[aria-label="Rename"]');
      await typeInto("settings", ".pill.is-active input.pill-rename", `${SELECT_ALL}Edit Bay\uE007`);
      backedUp = await configShows("the page renamed Edit Bay", (c) => c.layers.find((l) => l.id === "studio")?.name === "Edit Bay");
    });
    await step("data: Back up writes every page, key, wheel mode and dial step", async () => {
      await click("settings", "nav.toolbar button.tab", "Help & backup");
      await click("settings", "#backup button", "Back up");
      await shows("settings", "Your keys are backed up.");
      const text = readFileSync(join(dialogs, "freewheel-mappings.json"), "utf8");
      const file = JSON.parse(text);
      // The short form (SCHEMA.md): printed key names, one-line actions.
      const base = file.pages?.[basePage(backedUp).name];
      const want = { IN: "Ctrl+Shift+S", TRANS: "hold Shift", SNAP: "toggle Alt", CAM5: "page Edit Bay", SHTL: "wheel mode volume", "FULL VIEW": "wheel brightness" };
      const wrong = Object.entries(want).filter(([k, v]) => base?.[k] !== v);
      if (file.version !== 2 || !file.$schema?.endsWith("freewheel-mappings.schema.json") || wrong.length || base.CUT?.open !== roundTrip || base.CUT?.args?.[0] !== "--fw")
        throw new Error(`the backup file is not the short form of the setup (${JSON.stringify(wrong)}): ${text.slice(0, 600)}`);
      const studio = file.pages["Edit Bay"];
      if (studio?.CUT !== "Play/Pause" || studio?.CAM5 !== "back" || file.wheel?.mode !== "jog" || file.dial?.brightness_step !== 7)
        throw new Error(`the second page, wheel or dial is wrong: ${text}`);
      say(`  backup file: ${Object.keys(file.pages).length} pages, ${Object.values(file.pages).reduce((n, p) => n + Object.keys(p).length, 0)} keys, ${text.split("\n").length} lines`);
    });
    await step("data: Empty setup clears it", async () => {
      await click("simulator", `[role=group][aria-label="Keys"] button`, "Empty setup");
      await configShows("no keys and one page", (c) => c.layers.length === 1 && Object.keys(c.layers[0].bindings).length === 0);
    }, ["simulator", "settings"]);
    await step("data: Restore brings back exactly the backed-up setup", async () => {
      await restoreFrom(null, true);
      await shows("settings", "Your keys are restored.");
      const now = await configShows("the restored mappings equal the backup", (c) => mappings(c) === mappings(backedUp));
      say(`  restored: ${now.layers.map((l) => l.name).join(", ")}`);
      await click("settings", "nav.toolbar button.tab", "Keys");
      await waitFor("settings", "both pages", `return [...document.querySelectorAll(".pills .pill")].map((p) => p.textContent).join("|").includes("Edit Bay")`);
      await drawingShows({ In: "Ctrl+Shift+S", Cut: "Opens Round Trip", Trans: "Hold Shift", Snap: "Alt on/off", Cam5: "→ Edit Bay", FullView: "Brightness" });
    });
    await step("data: the page key switches pages and the Keys tab follows", async () => {
      await pressKey("Cam5");
      await activePill("Edit Bay");
      const second = (await readConfig()).layers.find((l) => l.name === "Edit Bay").id;
      await waitFor("simulator", "the Speed Editor on the second page", `return document.querySelector(".fw-sim-meta")?.textContent.includes("Page " + arguments[0])`, [second]);
      await drawingShows({ Cut: "Play/Pause", Cam5: "← Back to " + basePage(backedUp).name });
      await press("CUT");
      await logged("would press PlayPause");
      await pressKey("Cam5");
      await activePill(basePage(backedUp).name);
      await press("CUT");
      await lineWith("would run", "Round Trip.exe");
    }, ["settings", "simulator"]);
    // ---- App profiles: a real app in front picks its profile ------------
    const F = P.frontApp;
    const fronts = [];
    /** The simulated Speed Editor on page `want` (the Simulator's status line). */
    const metaIs = (what, want) =>
      waitFor("simulator", what, `const m = document.querySelector(".fw-sim-meta")?.textContent ?? "";
        if (m.includes("Page " + arguments[0] + " ")) return true;
        throw new Error(m + " / " + (document.querySelector("[data-front-app]")?.textContent ?? ""));`, [want], 20000);
    /** The live dots: on profile `profile` and on page `page` (when the profile has several pages). */
    const liveOn = (profile, page) => waitFor("settings", `the live dots on ${profile} / ${page}`, `const [profile, page] = arguments;
      const p = document.querySelector(".profile-pill .live-dot")?.closest(".profile-pill")?.textContent ?? "";
      const g = document.querySelector(".pill .live-dot")?.closest(".pill")?.textContent ?? null;
      const shown = document.querySelector(".profile-pill.is-active")?.textContent ?? "";
      if (p.includes(profile) && shown.includes(profile) && (page === null || g?.includes(page))) return true;
      throw new Error("live on " + p + " / " + g + ", showing " + shown);`, [profile, page]);
    const writing = (c) => (c.profiles ?? []).find((p) => p.name === "Writing");
    await step(`profiles: New profile Writing, for ${F.id}`, async () => {
      await click("settings", "button.profile-add", "New profile");
      await waitFor("settings", "the new profile's apps panel", `return document.querySelector(".side-panel .profile-panel")?.textContent.includes("used?")`);
      await shows("settings", "Any other app: back to Default");
      await typeInto("settings", '.side-panel input[aria-label="Program name"]', `${F.id}\uE007`);
      await configShows(`a profile for ${F.id}`, (c) => c.profiles?.at(-1)?.apps?.some((a) => a[platform] === F.id));
      await waitFor("settings", "the app listed", `return document.querySelector('.side-panel [role=list]')?.textContent.includes(arguments[0])`, [F.id]);
      await click("settings", '.profile-pill.is-active button[aria-label="Rename profile"]');
      await typeInto("settings", ".profile-pill.is-active input.profile-rename", `${SELECT_ALL}Writing\uE007`);
      await configShows("the profile renamed Writing", (c) => writing(c)?.pages.length === 1);
      await click("settings", '.profile-pill.is-active button[aria-label="Apps for this profile"]');
      await waitFor("settings", "Writing's apps panel", `return document.querySelector(".side-panel .profile-panel")?.textContent.includes("When is Writing used?")`);
      await waitFor("settings", "Writing's own page Main", `return [...document.querySelectorAll(".keys-title")].length === 1 && document.querySelector(".profile-pill.is-active")?.textContent.includes("Writing")`);
    });
    await step("profiles: a page key on Writing makes a page of Writing", async () => {
      // CUT is not set on the new page: its panel opens on the choices.
      await keyIn("Cut");
      await choice("Switch to another set of keys");
      await click("settings", ".side-panel button.suggestion", "");
      const c = await configShows("Writing's second page, CUT goes to it", (c) => {
        const w = writing(c);
        const main = c.layers.find((l) => l.id === w?.pages[0]);
        return w?.pages.length === 2 && main?.bindings.Cut?.LayerPush?.target === w.pages[1];
      });
      await waitFor("settings", "Writing's two page pills", `return document.querySelectorAll(".pills .pill").length === 2`);
      // Default's pages are untouched.
      if (c.layers.filter((l) => !c.profiles.some((p) => p.pages.includes(l.id))).length !== 2) throw new Error("a Default page was added");
    });
    let defaultPage = null;
    await step(`profiles: ${F.id} in front for real: Writing on its first page, the live dots follow`, async () => {
      // Default on its second page first: it comes back there.
      await click("settings", ".profile-pill button[role=tab]", "Default");
      await pressKey("Cam5");
      defaultPage = (await readConfig()).layers.find((l) => l.name === "Edit Bay").id;
      await metaIs("the Speed Editor on Edit Bay", defaultPage);
      const c = await readConfig();
      fronts.push(F.start());
      say(`  ${await F.bring(fronts.at(-1), "app")}`);
      await metaIs("the Speed Editor on Writing's first page", writing(c).pages[0]);
      await waitFor("simulator", `${F.id} in front`, `return document.querySelector("[data-front-app]")?.textContent.toLowerCase().includes(arguments[0])`, [F.id]);
      await logged('profile is now "Writing", page "Main"');
      await liveOn("Writing", "Main");
    }, ["settings", "simulator"]);
    await step("profiles: another app in front for real: Default, on the page it was on", async () => {
      fronts.push(F.startOther());
      say(`  ${await F.bring(fronts.at(-1), "other")}`);
      await metaIs("the Speed Editor back on Edit Bay", defaultPage);
      await logged('profile is now "Default", page "Edit Bay"');
      await liveOn("Default", "Edit Bay");
      await waitFor("settings", "CAM 5 lit (the way back to Main)", `return document.querySelector('.device [data-binding="Cam5"]')?.classList.contains("is-lit")`);
      F.stop(fronts);
      await pressKey("Cam5");
      await metaIs("the Speed Editor on Main", (await readConfig()).base_layer);
    }, ["settings", "simulator"]);
    await step("simulator: App in front fakes the app in front", async () => {
      const c = await readConfig();
      await typeInto("simulator", 'input[aria-label="App in front"]', `${SELECT_ALL}${F.id}`);
      await click("simulator", ".fw-sim-front button.fw-sim-primary", "Bring to front");
      await metaIs("the faked app on Writing", writing(c).pages[0]);
      await liveOn("Writing", "Main");
      await click("simulator", '[role=group][aria-label="Other apps in front"] button', "Another app");
      await metaIs("another faked app on Default", c.base_layer);
      await liveOn("Default", basePage(c).name);
    }, ["simulator", "settings"]);
    await step("profiles: Back up writes the profile with its apps and pages", async () => {
      await click("settings", "nav.toolbar button.tab", "Help & backup");
      await click("settings", "#backup button", "Back up");
      await shows("settings", "Your keys are backed up.");
      const text = readFileSync(join(dialogs, "freewheel-mappings.json"), "utf8");
      const file = JSON.parse(text);
      const w = file.profiles?.find((p) => p.name === "Writing");
      if (!file.pages?.["Edit Bay"] || !w || Object.keys(w.pages ?? {}).length !== 2 || !JSON.stringify(w.apps).includes(F.id) || !Object.values(w.pages)[0].CUT?.startsWith("page "))
        throw new Error(`the backup lacks the profile: ${text.slice(0, 900)}`);
      // Back to the Keys tab without the profile for the steps after.
      await click("settings", "nav.toolbar button.tab", "Keys");
      await click("settings", ".profile-pill button[role=tab]", "Writing");
      await click("settings", '.profile-pill.is-active button[aria-label="Delete profile"]');
      await click("settings", ".confirm-inline button", "Delete");
      await configShows("Writing deleted with its pages", (c) => !c.profiles?.length && c.layers.length === 2);
    }, ["settings"]);
    await step("data: someone else's file, apps not allowed: its app key comes in off", async () => {
      const theirs = join(dialogs, "someone-else.json");
      writeFileSync(theirs, JSON.stringify({
        version: 1,
        base_layer: "base",
        layers: [{ id: "base", name: "Their keys", bindings: { Cam1: { LaunchApp: { path: "C:\\Their\\Tool.exe", args: [], enabled: true } }, In: { Hotkey: { combo: { modifiers: ["Ctrl"], key: "T" } } } } }],
        jog: backedUp.jog,
        overrides: backedUp.overrides,
      }, null, 2));
      await restoreFrom(theirs, false);
      await configShows("Cam1 imported off", (c) => basePage(c).name === "Their keys" && basePage(c).bindings.Cam1.LaunchApp.enabled === false);
      await click("settings", "nav.toolbar button.tab", "Keys");
      await drawingShows({ Cam1: "Opens Tool (off)", In: "Ctrl+T" });
      await keyIn("Cam1");
      await waitFor("settings", "the panel saying it is off", `return document.querySelector(".side-panel .panel-now")?.textContent.includes("off: restored from a file")`);
      await press("CAM 1");
      await logged("Tool.exe is off");
    }, ["settings", "simulator"]);
    await step("data: the same file with apps allowed: its app key comes in on", async () => {
      await restoreFrom(join(dialogs, "someone-else.json"), true);
      await configShows("Cam1 imported on", (c) => basePage(c).bindings.Cam1.LaunchApp.enabled === true);
      await click("settings", "nav.toolbar button.tab", "Keys");
      await drawingShows({ Cam1: "Opens Tool" });
      await press("CAM 1");
      await lineWith("would run", "Tool.exe");
    }, ["settings", "simulator"]);
    // ---- Hand-written short files (SCHEMA.md) ---------------------------
    const restoredWith = async (text) => {
      await waitFor("settings", `the restore message "${text}"`, `const t = document.querySelector("#backup .success-line")?.textContent ?? "";
        if (t.includes(arguments[0])) return true;
        throw new Error("restore says: " + t + " / " + (document.querySelector("#backup [role=alert]")?.textContent ?? ""));`, [text]);
    };
    let handWritten;
    // Windows apps and a .cmd: the Windows run only (Notepad, explorer.exe, cmd.exe).
    if (platform === "windows") {
      await step("data: a hand-written short file restores: key names, shortcuts, pages, apps, the wheel", async () => {
        const short = join(dialogs, "hand-written.json");
        writeFileSync(short, `{
    "$schema": "https://raw.githubusercontent.com/brinkercode/freewheel/main/schema/freewheel-mappings.schema.json",
    "version": 2,
    "pages": {
      "Main": {
        "CUT": "Ctrl+Z",
        "CAM1": "open Notepad",
        "CAM2": "https://example.com",
        "TRANS": "hold Shift",
        "SNAP": "toggle Alt",
        "ESC": "page Music",
        "STOP/PLAY": "Play/Pause",
        "AUDIO LEVEL": "wheel volume",
        "cam 3": "open No Such App 42",
        "in": "ctrl + shift + F13",
        "OUT": "scroll -3"
      },
      "Music": { "ESC": "back", "CAM1": "Next track" }
    },
    "wheel": { "mode": "scroll", "speed": 5 }
  }
  `);
        await restoreFrom(short, true);
        await restoredWith("CAM3 on Main: there is no app or file called No Such App 42 on this computer.");
        handWritten = await configShows("the hand-written keys", (c) => {
          const main = basePage(c);
          const music = c.layers.find((l) => l.name === "Music");
          const b = main?.bindings ?? {};
          const ok = main?.name === "Main" && c.layers.length === 2 && music && Object.keys(music.bindings).join() === "Cam1"
            && JSON.stringify(b.Cut) === JSON.stringify({ Hotkey: { combo: { modifiers: ["Ctrl"], key: "Z" } } })
            && b.Cam1?.LaunchApp?.enabled === true && /notepad/i.test(JSON.stringify(b.Cam1))
            && JSON.stringify(b.Cam2) === JSON.stringify({ LaunchApp: { path: "explorer.exe", args: ["https://example.com"], enabled: true } })
            && b.Trans?.Held?.combo.modifiers.join() === "Shift" && b.Snap?.Latched?.combo.modifiers.join() === "Alt"
            && b.Esc?.LayerPush?.target === music.id && b.StopPlay?.Hotkey?.combo.key === "PlayPause"
            && b.AudioLevel?.OverrideToggle?.mode === "SystemVolume"
            && b.Cam3?.LaunchApp?.enabled === false && b.Cam3.LaunchApp.path === "No Such App 42"
            && JSON.stringify(b.In) === JSON.stringify({ Hotkey: { combo: { modifiers: ["Ctrl", "Shift"], key: "F13" } } })
            && b.Out?.Scroll?.amount === -3 && music.bindings.Cam1.Hotkey.combo.key === "NextTrack"
            && c.jog.default_mode === "Scroll" && Object.values(c.jog.per_mode).every((m) => m.threshold === 6);
          if (!ok) throw new Error(JSON.stringify(c).slice(0, 1500));
          return true;
        });
        await click("settings", "nav.toolbar button.tab", "Keys");
        await drawingShows({ Cut: "Undo", Cam1: "Opens Notepad", Cam2: "Opens example.com", Trans: "Hold Shift", Snap: "Alt on/off", Esc: "→ Music", StopPlay: "Play/Pause", AudioLevel: "Volume knob", Cam3: "Opens No Such App 42 (off)", In: "Ctrl+Shift+F13" });
        await click("settings", ".pill button[role=tab]", "Music");
        await drawingShows({ Esc: "← Back to Main", Cam1: "Next track" });
      }, ["settings", "simulator"]);
      await step("data: back up the hand-written setup, then restore that file: the same keys", async () => {
        // The first backup is restored again at the end: keep it.
        const original = readFileSync(join(dialogs, "freewheel-mappings.json"));
        await click("settings", "nav.toolbar button.tab", "Help & backup");
        await click("settings", "#backup button", "Back up");
        await shows("settings", "Your keys are backed up.");
        const text = readFileSync(join(dialogs, "freewheel-mappings.json"), "utf8");
        writeFileSync(join(dialogs, "hand-written-backup.json"), text);
        writeFileSync(join(dialogs, "freewheel-mappings.json"), original);
        writeFileSync(join(out, "exported-example.json"), text);
        say(`  the exported file:\n${text}`);
        for (const part of ['"CUT": "Ctrl+Z"', '"CAM3": "open No Such App 42"', '"ESC": "page Music"', '"ESC": "back"', '"IN": "Ctrl+Shift+F13"', '"OUT": "scroll -3"', '"mode": "scroll"', '"speed": 5', '"CAM2": "https://example.com"'])
          if (!text.includes(part)) throw new Error(`the exported file lacks ${part}:\n${text}`);
        await click("simulator", `[role=group][aria-label="Keys"] button`, "Empty setup");
        await configShows("no keys", (c) => c.layers.length === 1 && Object.keys(c.layers[0].bindings).length === 0);
        await restoreFrom(join(dialogs, "hand-written-backup.json"), true);
        await restoredWith("Your keys are restored.");
        await configShows("the exported file restores the same keys", (c) => {
          if (mappings(c) !== mappings(handWritten)) throw new Error(mappings(c).slice(0, 800) + " vs " + mappings(handWritten).slice(0, 800));
          return true;
        });
      }, ["settings", "simulator"]);
    }
    // A file shared by several computers: one app per OS. This computer
    // runs its own entry, and a Back up writes every OS's entry again.
    await step("data: a 3-OS file restores, and Back up keeps all three OSes", async () => {
      const original = readFileSync(join(dialogs, "freewheel-mappings.json"));
      const perOs = join(dialogs, "three-os.json");
      // This computer's entry is Round Trip; the other OSes name their own apps.
      const cam1 = platform === "windows"
        ? { open: { windows: roundTrip, macos: "com.spotify.client", linux: "spotify.desktop" }, args: ["--fw"] }
        : { open: { windows: "C:\\Program Files\\Spotify\\Spotify.exe", macos: "com.spotify.client", linux: roundTrip }, args: ["--fw"] };
      const cam2 = platform === "windows" ? { open: { macos: "com.apple.Safari", linux: "firefox" } } : { open: { windows: "notepad.exe", macos: "com.apple.Safari" } };
      const others = platform === "windows" ? "macOS and Linux" : "Windows and macOS";
      writeFileSync(perOs, JSON.stringify({ version: 2, pages: { Main: { CAM1: cam1, CAM2: cam2 } } }, null, 2));
      await restoreFrom(perOs, true);
      await restoredWith(`CAM2 on Main: the file has no "${platform}" app to open on this computer.`);
      await configShows("CAM1 runs this OS's entry and keeps the others", (c) => {
        const b = basePage(c).bindings;
        const ok = b.Cam1?.LaunchApp?.path === roundTrip && b.Cam1.LaunchApp.enabled === true && b.Cam1.LaunchApp.args.join() === "--fw"
          && JSON.stringify(b.Cam1.LaunchApp.per_os) === JSON.stringify(cam1.open)
          && b.Cam2?.LaunchApp?.enabled === false && JSON.stringify(b.Cam2.LaunchApp.per_os) === JSON.stringify(cam2.open);
        if (!ok) throw new Error(JSON.stringify(b));
        return true;
      });
      const backUp = async () => {
        await click("settings", "nav.toolbar button.tab", "Help & backup");
        await click("settings", "#backup button", "Back up");
        await shows("settings", "Your keys are backed up.");
        return readFileSync(join(dialogs, "freewheel-mappings.json"), "utf8");
      };
      const text = await backUp();
      const main = JSON.parse(text).profiles?.Main;
      if (JSON.stringify(main?.CAM1) !== JSON.stringify(cam1) || JSON.stringify(main?.CAM2) !== JSON.stringify(cam2))
        throw new Error(`the backup lost an OS's app:\n${text}`);
      say(`  exported: CAM1 ${JSON.stringify(main.CAM1)}, CAM2 ${JSON.stringify(main.CAM2)}`);
      // The key panel shows this computer's app and says the others are set.
      await click("settings", "nav.toolbar button.tab", "Keys");
      await drawingShows({ Cam1: "Opens Round Trip" });
      await keyIn("Cam1");
      await toChoices();
      await choice("Open an app or website");
      await waitFor("settings", "the other OSes' note", `const t = [...document.querySelectorAll(".side-panel [role=note]")].map((n) => n.textContent).join(" | ");
        if (t.includes("Also set for ${others}.")) return true;
        throw new Error("notes: " + t);`);
      // Picking another app here changes only this OS's entry.
      const first = await waitFor("settings", "the installed apps", `return document.querySelector(".side-panel .app-row .grow")?.textContent.trim()`, [], 20000);
      await click("settings", ".side-panel .app-row", first);
      await configShows("CAM1 picked again, the other OSes kept", (c) => {
        const a = basePage(c).bindings.Cam1?.LaunchApp;
        if (!a || a.path === roundTrip || JSON.stringify(a.per_os) !== JSON.stringify(cam1.open)) throw new Error(JSON.stringify(a));
        return true;
      });
      const again = JSON.parse(await backUp()).profiles?.Main?.CAM1;
      if (Object.keys(cam1.open).some((k) => k !== platform && again?.open?.[k] !== cam1.open[k]) || !again?.open?.[platform] || again.open[platform] === roundTrip)
        throw new Error(`after a new pick the backup is ${JSON.stringify(again)}`);
      say(`  after picking ${first}: ${JSON.stringify(again)}`);
      writeFileSync(join(dialogs, "freewheel-mappings.json"), original);
    }, ["settings", "simulator"]);
    await step("data: a mistake in a hand-written file names the line, column and key; nothing changes", async () => {
      const before = JSON.stringify(await readConfig());
      const typo = join(dialogs, "typo.json");
      writeFileSync(typo, '{\n  "pages": {\n    "Main": {\n      "CAM1": "opn Spotify"\n    }\n  }\n}\n');
      await restoreFrom(typo, true);
      const want = `This file can't be restored: line 4, column 15, CAM1 on Main: unknown action "opn Spotify" (did you mean "open Spotify"?)`;
      await waitFor("settings", "the line and key of the mistake", `const t = document.querySelector("#backup [role=alert]")?.textContent;
        if (t === arguments[0]) return true;
        throw new Error("alert: " + t);`, [want]);
      await sleep(500);
      if (JSON.stringify(await readConfig()) !== before) throw new Error("the bad file changed the config");
    });
    // Windows apps and a .cmd: the Windows run only (Notepad, explorer.exe, cmd.exe).
    if (platform === "windows") {
      await step("data: open by full path runs a .cmd with its args in its cwd (Do actions for real)", async () => {
        const marker = resolve(dialogs, "marker.cmd");
        const cwd = resolve(dialogs, "start here");
        const result = resolve(dialogs, "marker.txt");
        mkdirSync(cwd, { recursive: true });
        rmSync(result, { force: true });
        writeFileSync(marker, '@echo off\r\n> "%~dp0marker.txt" echo %~1^|%~2^|%CD%\r\n');
        const file = join(dialogs, "open-by-path.json");
        writeFileSync(file, JSON.stringify({
          version: 2,
          pages: {
            Main: {
              CAM4: { open: marker, args: ["hello world", "second"], cwd: `%${DIALOG_DIR_ENV}%\\start here` },
              CAM5: `open %${DIALOG_DIR_ENV}%\\marker.cmd`,
            },
          },
        }, null, 2));
        await restoreFrom(file, true);
        await restoredWith("Your keys are restored.");
        await configShows("CAM4 runs marker.cmd by path, CAM5 by the expanded variable", (c) => {
          const b = basePage(c).bindings;
          const ok = b.Cam4?.LaunchApp?.path === marker && b.Cam4.LaunchApp.args.join("|") === "hello world|second" && b.Cam4.LaunchApp.cwd?.toLowerCase() === cwd.toLowerCase() && b.Cam4.LaunchApp.enabled
            && b.Cam5?.LaunchApp?.path.toLowerCase() === marker.toLowerCase() && b.Cam5.LaunchApp.enabled;
          if (!ok) throw new Error(JSON.stringify(b));
          return true;
        });
        await setReal(true);
        try {
          await pressKey("Cam4");
          const end = Date.now() + 15000;
          let got = "";
          while (Date.now() < end) {
            got = existsSync(result) ? readFileSync(result, "utf8").trim() : "";
            if (got) break;
            await sleep(200);
          }
          say(`  marker.txt: ${JSON.stringify(got)}`);
          if (got.toLowerCase() !== `hello world|second|${cwd}`.toLowerCase()) throw new Error(`marker.cmd wrote ${JSON.stringify(got)}`);
          await lineWith("launched \"marker\" ok", "marker.cmd");
        } finally {
          await setReal(false);
        }
      }, ["simulator", "settings"]);
    }
    for (const [name, text, error] of [
      ["broken.json", "{ this is not a backup", "This is not a freewheel mappings file"],
      ["newer.json", JSON.stringify({ version: 99, layers: "from the future" }), "This file is from a newer version of freewheel."],
    ])
      await step(`data: ${name} is refused, the keys stay as they were`, async () => {
        const before = JSON.stringify(await readConfig());
        writeFileSync(join(dialogs, name), text);
        await restoreFrom(join(dialogs, name), true);
        await waitFor("settings", `the error "${error}"`, `return document.querySelector("#backup [role=alert]")?.textContent.includes(arguments[0])`, [error]);
        await sleep(500);
        const after = JSON.stringify(await readConfig());
        if (after !== before) throw new Error(`${name} changed the config`);
        say(`  ${name}: "${await d.exec(`return document.querySelector("#backup [role=alert]").textContent`)}"`);
      });
    await step("data: restore the setup again and set the look, to check after a restart", async () => {
      await restoreFrom(null, true);
      await configShows("the backup again", (c) => mappings(c) === mappings(backedUp));
      await click("settings", "nav.toolbar button.tab", "General");
      await click("settings", "button.appearance-option", "Dark");
      await click("settings", "nav.toolbar button.tab", "Help & backup");
      await openMore();
      if ((await switchState("settings", "Turn on detailed logs for troubleshooting")) !== "true") await flip("Turn on detailed logs for troubleshooting", true);
      saved = await configShows("Dark and detailed logs saved", (c) => c.settings.appearance === "Dark" && c.settings.detailed_logs === true);
    });
    await step("settings still clickable after the simulator", async () => {
      await click("settings", "nav.toolbar button.tab", "Wheel");
      await waitFor("settings", "the Wheel tab", `return document.body.innerText.includes("When I turn the wheel")`);
    });
    // Last, so every step before runs on the window the first run opened.
    await step("On/Off: a settings window opened after the tray's Off reads Off", async () => {
      await tray("toggle");
      await allRead(false, "tray Off");
      // Closing destroys the webview (#144); Open settings builds a new one.
      const closing = win.settings;
      await click("settings", ".window-control.window-close").catch((e) => {
        // WebKitWebDriver may answer the click after the window is gone.
        if (!/no such window/.test(e.message)) throw e;
      });
      // Wait until the old webview is gone, or findWindows may pick it up again.
      for (const end = Date.now() + 15000; (await d.handles()).includes(closing) && Date.now() < end; ) await sleep(200);
      await tray("open-settings");
      await findWindows();
      await waitFor("settings", "the settings tabs", `return document.querySelector("nav.toolbar")`, [], 30000);
      await allRead(false, "the new settings window");
      await tray("toggle");
      await allRead(true, "tray On");
      await waitFor("settings", "Connected", `return document.querySelector("button.status")?.textContent.includes("Connected")`);
    }, ["simulator", "settings"]);

    // ---- A settings window closed and built again (#144) hears the tray -
    /** Closes the settings window like a user (its close button) and waits until it is gone. */
    const closeSettings = async () => {
      const old = win.settings;
      await click("settings", ".window-control.window-close").catch((e) => {
        // WebKitWebDriver may answer the click after the window is gone.
        if (!/no such window/.test(e.message)) throw e;
      });
      const end = Date.now() + 15000;
      while ((await d.handles()).includes(old)) {
        if (Date.now() > end) throw new Error("the settings window did not close");
        await sleep(150);
      }
      current = null;
    };
    /** After the window was built again: attach to it, wait for its tabs. */
    const rebuilt = async (css = "nav.toolbar") => {
      await findWindows();
      await waitFor("settings", `the new window's ${css}`, `return document.querySelector(arguments[0])`, [css], 30000);
    };
    const updateChecked = async () => {
      await trayShows("update", "Checking (disabled)", `(t, off) => t === "Checking for updates…" && off`);
      await shows("settings", "You’re up to date.");
      await trayShows("update", "up to date", `(t, off) => t === "You’re up to date · Check again" && !off`);
    };
    const logsShown = () =>
      waitFor("settings", "Help & backup's Downloaded line", `return document.querySelector('nav.toolbar button.tab[aria-pressed="true"]')?.textContent.includes("Help") && document.querySelector("#logs .success-line")?.textContent.includes("to your Downloads folder")`, [], 20000);
    const permissionFix = async (closed) => {
      await click("simulator", `[role=group][aria-label="macOS permission"] button`, "Missing");
      await trayShows("status", "Needs permission", `(t, off) => t === "Needs permission (click to fix)" && !off`);
      if (closed) await closeSettings();
      await trayClick("status", "Needs permission");
      // A window built for the fix opens on it (no tabs).
      if (closed) await rebuilt(".setup");
      await waitFor("settings", "the permission fix", `return document.querySelector(".setup")`, [], 20000);
      await click("simulator", `[role=group][aria-label="macOS permission"] button`, "Allowed");
    };
    const recording = async (closed) => {
      if (closed) await closeSettings();
      const before = await lastLine();
      await jumpTo("Record a key");
      if (closed) await rebuilt();
      await waitFor("simulator", "a new Recording line", `const n = arguments[0]; return [...document.querySelectorAll(".fw-sim-log li[data-n]")].some((li) => Number(li.dataset.n) > n && li.textContent.includes("Recording a shortcut for CUT"));`, [before]);
      await waitFor("settings", "CUT's panel recording", `return document.querySelector(".side-panel .panel-title")?.textContent.includes("CUT") && document.querySelector(".side-panel .mode-title")?.textContent.includes("Press a shortcut")`, [], 20000);
    };
    for (const round of [1, 2]) {
      await step(`reopen ${round}: close the settings window, tray Open settings builds it again`, async () => {
        await closeSettings();
        await trayClick("open-settings", "Open settings");
        await rebuilt();
      }, ["simulator", "settings"]);
      await step(`reopen ${round}: tray Check for updates acts in the new window`, async () => {
        await trayShows("update", "Check for updates", `(t, off) => !off && /Check/.test(t)`);
        await trayClick("update", "");
        await updateChecked();
      }, ["simulator", "settings"]);
      await step(`reopen ${round}: tray Download logs opens Help & backup in the new window`, async () => {
        await click("settings", "nav.toolbar button.tab", "Keys");
        await trayClick("download-logs", "Download logs");
        await logsShown();
      }, ["simulator", "settings"]);
      await step(`reopen ${round}: tray Needs permission opens the fix in the new window`, () => permissionFix(false), ["simulator", "settings"]);
      await step(`reopen ${round}: Record a key opens CUT recording in the new window`, () => recording(false), ["simulator", "settings"]);
      await step(`reopen ${round}: tray Open settings brings the minimised new window back`, async () => {
        await ask("settings", `await window.__TAURI__.window.getCurrentWindow().minimize(); return "ok";`);
        await waitAsk("settings", "minimised", `return await window.__TAURI__.window.getCurrentWindow().isMinimized();`, true);
        await trayClick("open-settings", "Open settings");
        await waitAsk("settings", "back from the taskbar", `return await window.__TAURI__.window.getCurrentWindow().isMinimized();`, false);
      }, ["simulator", "settings"]);
    }
    // While it is closed: each tray item opens the window and acts in it.
    await step("closed: tray Check for updates opens the window and checks there", async () => {
      await closeSettings();
      await trayShows("update", "Check for updates", `(t, off) => !off && /Check/.test(t)`);
      await trayClick("update", "");
      await rebuilt();
      await updateChecked();
    }, ["simulator", "settings"]);
    await step("closed: tray Download logs opens the window on Help & backup", async () => {
      await closeSettings();
      await trayClick("download-logs", "Download logs");
      await rebuilt();
      await logsShown();
    }, ["simulator", "settings"]);
    await step("closed: tray Needs permission opens the window on the fix", () => permissionFix(true), ["simulator", "settings"]);
    await step("closed: Record a key opens the window on CUT recording", () => recording(true), ["simulator", "settings"]);

    say("step: tray: Quit freewheel");
    // The app may be gone before the click answers.
    await tray("quit").catch((e) => say(`  the Quit click: ${e.message}`));
    // Windows without --port: tauri-driver owns the app, nothing to wait for or relaunch.
    const owned = platform === "linux" || app.app;
    if (owned) {
      const end = Date.now() + 20000;
      while (!app.exited() && Date.now() < end) await sleep(200);
      if (!app.exited()) throw new Error("the tray's Quit did not end the app within 20 s");
      say(`  the app quit${app.app ? ` with ${app.app.exitCode}` : ""}`);
    }
    if (owned && saved) {
      say("relaunching the app (same config folder) to check what was saved");
      // Windows starts it again; on Linux the next session's WebKitWebDriver does.
      await d.quit();
      d.session = null;
      if (platform === "windows") app = await P.start(opts, say, dialogs);
      await d.start(resolve(opts.app), opts.port);
      await findWindows();
      await step("relaunch: pages, keys, wheel, dial steps and settings are all still there", async () => {
        await waitFor("settings", "the settings tabs (no first run)", `return document.querySelector("nav.toolbar")`, [], 20000);
        const now = await readConfig();
        if (JSON.stringify(now) !== JSON.stringify(saved)) throw new Error(`after a restart the config is ${JSON.stringify(now).slice(0, 600)}`);
        say(`  after restart: ${now.layers.map((l) => `${l.name} (${Object.keys(l.bindings).length} keys)`).join(", ")}; appearance ${now.settings.appearance}`);
        await waitFor("settings", "appearance Dark", `return document.documentElement.dataset.appearance === "Dark"`);
        await click("settings", "nav.toolbar button.tab", "Keys");
        await waitFor("settings", "both pages", `return [...document.querySelectorAll(".pills .pill")].map((p) => p.textContent).join("|").includes("Edit Bay")`);
        await drawingShows({ In: "Ctrl+Shift+S", Cut: "Opens Round Trip", Cam5: "→ Edit Bay" });
      }, ["settings", "simulator"]);
      // A fresh start (no battery warning, nothing held): the README stills.
      await readmeShots();
      say("step: tray: Quit freewheel (after the restart)");
      await tray("quit").catch((e) => say(`  the Quit click: ${e.message}`));
      const end = Date.now() + 20000;
      while (!app.exited() && Date.now() < end) await sleep(200);
      if (!app.exited()) throw new Error("the tray's Quit did not end the relaunched app within 20 s");
    }
    say("e2e: PASS");
    writeFileSync(join(out, "e2e-log.txt"), log.join("\n") + "\n");
  } catch (e) {
    say(`e2e: FAIL: ${e.message}`);
    // A frozen main thread (a deadlock) looks like a missing UI change: say which it was.
    await responsive("the failure").then(
      () => say("  the app still answers IPC from both windows"),
      (err) => say(`  the app is frozen: ${err.message}`),
    );
    // What the update flow and the windows did: the page's trace and the app's log.
    if (win.settings) {
      const t = await ask("settings", "return window.__freewheelTrace ?? []").catch((err) => say(`  no trace from the settings page: ${err.message}`));
      say("  settings page trace (src/trace.js):");
      for (const line of traceLines(t).slice(-40)) say(`    ${line}`);
    }
    const logFile = platform === "linux" ? linuxAppLog() : appLog();
    say(`  the app's log, update flow / windows / tray / freezes (${logFile}):`);
    if (existsSync(logFile)) for (const line of flowLines(readFileSync(logFile, "utf8"))) say(`    ${line}`);
    else say("    (no log file)");
    for (const name of ["settings", "simulator"])
      if (win[name]) await shot(name, "failure").catch((err) => say(`  no failure screenshot of ${name}: ${err.message}`));
    writeFileSync(join(out, "e2e-log.txt"), log.join("\n") + "\n");
    process.exitCode = 1;
  } finally {
    await d.quit();
    app?.quit();
    cleanup?.();
  }
}

const PLATFORM_OF = { win32: "windows", linux: "linux" };
if (process.argv[1] === fileURLToPath(import.meta.url)) await main(PLATFORM_OF[process.platform]);
