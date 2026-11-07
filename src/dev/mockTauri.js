// Dev-only browser harness: fakes the Tauri bridge so `npm run dev` renders the
// settings window in a plain browser. main.jsx imports this only when
// import.meta.env.DEV and no Tauri bridge exists, so it never ships in the
// production bundle (src/BuildOutput.test.js checks that).
//
// URL parameters pick the state to show:
//   ?firstrun=1            the first run instead of the settings window
//   ?conn=usb|bt|btlow|none  how the Speed Editor is connected (default usb)
//   ?off=1                 freewheel is Off
//   ?paused=1              stepped aside for DaVinci Resolve
//   ?perm=missing          a macOS permission is missing
//   ?appearance=Light|Dark|System  the Appearance setting (default System)
//   ?empty=1               an empty setup: one page, nothing set
//   ?simulate=1            as in --simulate: the Simulator harness drives it (__fwEmit)
//   ?updated=0.1.1         just updated from 0.1.0 to that version: What's new shows

import { compareVersions } from "../whatsNew.js";

const params = new URLSearchParams(globalThis.location?.search ?? "");
const sleep = (ms) => new Promise((r) => globalThis.setTimeout(r, ms));

const hotkey = (key, ...modifiers) => ({ Hotkey: { combo: { modifiers, key } } });

/** The prototype's sample setup: two pages, Main and Music, several keys set. */
export function sampleConfig() {
  const empty = params.get("empty") === "1";
  const binding = (left, right) => ({ left, right, threshold: 3, shuttle_max_rate_hz: null });
  return {
    version: 1,
    settings: {
      enabled: params.get("off") !== "1",
      start_at_login: true,
      resolve_process_name: "Resolve.exe",
      low_battery_threshold: 15,
      appearance: params.get("appearance") ?? "System",
      auto_update: true,
      step_aside_for_resolve: true,
      detailed_logs: false,
    },
    base_layer: "main",
    layers: empty
      ? [{ id: "main", name: "Main", bindings: {} }]
      : [
          {
            id: "main",
            name: "Main",
            bindings: {
              SmartInsrt: hotkey("Z", "Ctrl"),
              Appnd: hotkey("Y", "Ctrl"),
              In: hotkey("C", "Ctrl"),
              Out: hotkey("V", "Ctrl"),
              Cut: hotkey("S", "Ctrl"),
              StopPlay: hotkey("Space"),
              AudioOnly: hotkey("M", "Ctrl"),
              Cam1: { LaunchApp: { path: "C:\\Program Files\\Spotify\\Spotify.exe", args: [], enabled: true } },
              Cam2: { LaunchApp: { path: "C:\\Program Files\\Google\\Chrome\\Application\\Chrome.exe", args: [], enabled: true } },
              Cam3: { LaunchApp: { path: "explorer.exe", args: ["https://youtube.com"], enabled: true } },
              FullView: { OverrideToggle: { mode: "SystemVolume" } },
              Esc: { LayerPush: { target: "music" } },
              Trans: { Latched: { combo: { modifiers: ["Shift"], key: "" } } },
              Snap: { Held: { combo: { modifiers: ["Alt"], key: "" } } },
              Shtl: { SetJogMode: { mode: "Shuttle" } },
              Jog: { SetJogMode: { mode: "Jog" } },
              Scrl: { SetJogMode: { mode: "Volume" } },
            },
          },
          {
            id: "music",
            name: "Music",
            bindings: {
              Cam1: hotkey("Left", "Ctrl"),
              Cam2: hotkey("Space"),
              Cam3: hotkey("Right", "Ctrl"),
              AudioLevel: { OverrideToggle: { mode: "AppVolume" } },
            },
          },
        ],
    jog: {
      default_mode: "Jog",
      // An empty setup scrolls on every wheel button, as the default config (config.rs).
      per_mode: empty
        ? Object.fromEntries(["Shuttle", "Jog", "Scroll"].map((m) => [m, binding({ Scroll: { amount: -1 } }, { Scroll: { amount: 1 } })]))
        : {
            Shuttle: binding(hotkey("Left"), hotkey("Right")),
            Jog: binding({ Scroll: { amount: -1 } }, { Scroll: { amount: 1 } }),
            Scroll: binding({ Scroll: { amount: -1 } }, { Scroll: { amount: 1 } }),
          },
    },
    overrides: { system_volume_step: 0.02, app_volume_step: 0.02, brightness_step: 5 },
  };
}

const BUS = { usb: "Usb", bt: "Bluetooth", btlow: "Bluetooth" };

/** The StatusEvent the URL asks for. */
export function sampleStatus() {
  const conn = params.get("conn") ?? "usb";
  return {
    enabled: params.get("off") !== "1",
    connection: params.get("off") === "1" ? "Off" : conn === "none" ? "Absent" : "Connected",
    battery: conn === "bt" ? { level: 64, charging: false } : conn === "btlow" ? { level: 12, charging: false } : null,
    layer: "main",
    override_errors: [],
    permissions: params.get("perm") === "missing" ? { Accessibility: "Missing", InputMonitoring: "Granted" } : {},
    bus: BUS[conn] ?? null,
    paused: params.get("paused") === "1",
    wheel_override: params.get("override") ?? null,
    lit: params.get("lit")?.split(",") ?? (conn === "none" ? [] : ["Scrl"]),
  };
}

export function installMockTauri(win = window) {
  let config = sampleConfig();
  const status = sampleStatus();
  const listeners = {};
  const updates = { current: params.get("updated") ?? "0.1.0", published: null, offer: null };
  const commands = {
    get_config: () => structuredClone(config),
    save_config: ({ config: next }) => {
      config = structuredClone(next);
      return null;
    },
    get_status: () => structuredClone(status),
    // A made-up sample for the Device test view (not a hardware capture).
    get_device_diagnostics: () => ({
      simulated: params.get("simulate") === "1",
      status: status.connection,
      device: { bus: status.bus, vendor_id: 0x1edb, product_id: 0xda0e, usage_page: 0xff00, usage: 1, interface: 0, product: "DaVinci Resolve Speed Editor", release: 0x0100, candidates: 1, report_descriptor: null },
      auth: { state: "Passed", passed: 2, failed: 0, session_s: 600, next_in_s: 4, last_error: null, last_passed_ms: 61000 },
      battery: status.battery,
      held: [],
      presses: ["CUT", "IN", "CAM 1"],
      wheel: { device_mode: 0, last_value: 2, total: -14, reports: 31 },
      counts: { "0x03": 31, "0x04": 6, "0x07": 1 },
      undecoded: 0,
      open_attempts: 1,
      connects: 1,
      disconnects: 0,
      last_open_error: null,
      reports: [
        { seq: 40, at_ms: 72110, direction: "In", id: 3, hex: "03 00 02 00 00 00 00", decoded: "wheel mode 0 (relative) value +2", failed: false },
        { seq: 39, at_ms: 70420, direction: "In", id: 4, hex: "04 00 00 00 00 00 00 00 00 00 00 00 00", decoded: "keys held: none", failed: false },
        { seq: 38, at_ms: 70300, direction: "In", id: 4, hex: "04 0f 00 00 00 00 00 00 00 00 00 00 00", decoded: "keys held: CUT", failed: false },
        { seq: 1, at_ms: 120, direction: "Out", id: 3, hex: "03 00 00 00 00 00 ff", decoded: "set wheel mode 0 (relative)", failed: false },
      ],
      uptime_ms: 73000,
    }),
    set_enabled: ({ on }) => {
      status.enabled = on;
      status.paused = false;
      status.connection = on ? "Connected" : "Off";
      (listeners.status ?? []).forEach((f) => f({ payload: structuredClone(status) }));
      return null;
    },
    get_start_at_login: () => config.settings.start_at_login,
    set_start_at_login: ({ on }) => on,
    app_version: () => params.get("updated") ?? "0.1.0",
    open_release_page: () => null,
    get_permissions: () => status.permissions,
    open_permission_pane: ({ permission }) =>
      params.get("simulate") === "1"
        ? `Simulated: this would open System Settings › Privacy & Security › ${permission === "InputMonitoring" ? "Input Monitoring" : permission}. Allow it in the Simulator (macOS permission: Allowed).`
        : null,
    open_bluetooth_settings: () => null,
    export_mappings: () => true,
    import_mappings: () => true,
    export_logs: () => "C:\\Users\\you\\Downloads\\freewheel-logs.zip",
    set_detailed_logs: ({ on }) => {
      config.settings.detailed_logs = on;
      return null;
    },
    show_logs_file: () => null,
    exit_wheel_override: () => {
      status.wheel_override = null;
      (listeners.status ?? []).forEach((f) => f({ payload: structuredClone(status) }));
      return null;
    },
    list_apps: () => [
      { name: "Calculator", path: "explorer.exe", args: ["C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs\\Calculator.lnk"] },
      { name: "Google Chrome", path: "explorer.exe", args: ["C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs\\Google Chrome.lnk"] },
      { name: "Spotify", path: "explorer.exe", args: ["C:\\Users\\you\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\Spotify.lnk"] },
    ],
    choose_app: () => ({ name: "Budget 2026", path: "C:\\Tools\\Budget 2026.exe", args: [] }),
    // A 1×1 blue PNG for Calculator; the others keep their letter avatars.
    app_icons: ({ paths }) =>
      Object.fromEntries(
        paths
          .filter((p) => p.endsWith("Calculator.lnk"))
          .map((p) => [p, "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=="]),
      ),
    tray_update_state: () => null,
    // As selfupdate.rs's simulated source: the Simulator's New version publishes one.
    update_check: async () => {
      await sleep(1500);
      const { current, published } = updates;
      if (!published || compareVersions(published, current) <= 0) return (updates.offer = null);
      updates.offer = published;
      return { version: published, current_version: current, body: sampleNotes(published) };
    },
    update_download: async ({ onEvent }) => {
      if (!updates.offer) throw "nothing: no update to download or install";
      const size = 8_400_000;
      onEvent.onmessage({ event: "Started", data: { contentLength: size } });
      for (let i = 0; i < 32; i++) {
        await sleep(100);
        onEvent.onmessage({ event: "Progress", data: { chunkLength: size / 32 } });
      }
      onEvent.onmessage({ event: "Finished" });
      return null;
    },
    update_install: () => {
      if (!updates.offer) throw "nothing: no update to download or install";
      updates.current = updates.offer;
      updates.offer = null;
      return "simulated";
    },
  };
  const invoke = async (cmd, args = {}) => {
    const run = commands[cmd];
    if (!run) throw new Error(`mock: no command ${cmd}`);
    return run(args);
  };
  const listen = async (event, handler) => {
    (listeners[event] ??= []).push(handler);
    return () => {
      listeners[event] = listeners[event].filter((h) => h !== handler);
    };
  };
  class Channel {
    onmessage = () => {};
  }
  win.__TAURI__ = { core: { invoke, Channel }, event: { listen } };
  // @tauri-apps/plugin-* talk to the internals directly.
  let id = 0;
  win.__TAURI_INTERNALS__ = {
    invoke,
    transformCallback: () => ++id,
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
  };
  // Simulated key press from the console: __fwPress("Cut", "CUT").
  win.__fwPress = (key, label = key.toUpperCase()) =>
    (listeners["key-pressed"] ?? []).forEach((f) => f({ payload: { key, label } }));
  // The Simulator harness (src/dev/simHarness.jsx) emits the simulate-only events.
  win.__fwEmit = (event, payload) => {
    // "New version": the simulated source publishes the next minor, as selfupdate.rs does.
    if (event === "sim-update") {
      const base = updates.published && compareVersions(updates.published, updates.current) > 0 ? updates.published : updates.current;
      const [major, minor] = base.split(".").map(Number);
      updates.published = `${major}.${minor + 1}.0`;
    }
    (listeners[event] ?? []).forEach((f) => f({ payload }));
  };
  if (params.get("simulate") === "1") win.__FREEWHEEL_SIMULATE__ = true;
  const storage = win.localStorage;
  if (params.get("firstrun") === "1") storage.removeItem("freewheel.first-run-done");
  else storage.setItem("freewheel.first-run-done", "1");
  const updated = params.get("updated");
  if (updated) {
    storage.setItem("freewheel.last-seen-version", "0.1.0");
    storage.setItem("freewheel.update-notes", JSON.stringify({ version: updated, notes: sampleNotes(updated) }));
  }
}

/** Release-please's notes for a small patch, as latest.json carries them. */
function sampleNotes(version) {
  return [
    `## [${version}](https://github.com/brinkercode/freewheel/compare/v0.1.0...v${version}) (2026-11-07)`,
    "",
    "",
    "### Bug Fixes",
    "",
    "* **keys:** a held Shift is released when freewheel turns Off ([#131](https://github.com/brinkercode/freewheel/issues/131)) ([1a2b3c4](https://github.com/brinkercode/freewheel/commit/1a2b3c4))",
    "* the wheel scrolls again after waking from sleep ([#132](https://github.com/brinkercode/freewheel/issues/132)) ([5d6e7f8](https://github.com/brinkercode/freewheel/commit/5d6e7f8))",
  ].join("\n");
}
