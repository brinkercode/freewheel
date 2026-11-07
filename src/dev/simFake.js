// Dev/test only: a small JS stand-in for the `simulate` and `sim_jump`
// commands (core simulator.rs, src-tauri simulate.rs), so the Simulator window
// renders and reacts in a plain browser (src/dev/simulator.html) and in tests.
// It never ships: nothing outside src/dev and *.test.* imports it.

const KEYS = [
  "SmartInsrt", "Appnd", "RiplOwr", "CloseUp", "PlaceOnTop", "SrcOwr", "In", "Out", "TrimIn", "TrimOut", "Roll",
  "SlipSrc", "SlipDest", "TransDur", "Cut", "Dis", "SmthCut", "Source", "Timeline", "Shtl", "Jog", "Scrl", "Esc",
  "SyncBin", "AudioLevel", "FullView", "Trans", "Split", "Snap", "RiplDel", "Cam1", "Cam2", "Cam3", "Cam4", "Cam5",
  "Cam6", "Cam7", "Cam8", "Cam9", "LiveOwr", "VideoOnly", "AudioOnly", "StopPlay",
];

const SAMPLE = { base: { Cut: "Ctrl+C", In: "I", Out: "O", Esc: "Ctrl+Z", Snap: "page edit" }, edit: { Cut: "X", In: "Ctrl+V" } };
const MODES = { Jog: "Jog", Shtl: "Shuttle", Scrl: "Scroll" };

/** A fresh fake session; `invoke(cmd, args)` answers like the Tauri bridge. */
export function createSimFake() {
  const s = {
    connection: "Absent", bus: null, battery: null, charging: false, low_battery: false, page: "base",
    jog_mode: "Scroll", lit_leds: [], held_keys: [], enabled: true, send_for_real: false, toasts: [],
    keys: KEYS, resolve_open: false, permissions: "Granted", update: null, first_run: false, mapped_keys: 0,
    wheel_override: null,
  };
  const trayMenu = { on: true, start_at_login: false, status_line: null };
  const trayClicks = [];
  let map = {};
  const jumps = [];
  // As Engine::leds: the key that went to this page, then the wheel button.
  const leds = () => {
    if (s.connection !== "Connected") return [];
    const pageKey = Object.keys(map.base ?? {}).find((k) => s.page !== "base" && map.base[k] === `page ${s.page}`);
    return [pageKey, { Scroll: "Scrl", Jog: "Jog", Shuttle: "Shtl" }[s.jog_mode]].filter(Boolean);
  };
  const toast = (t) => s.toasts.push(t);
  const connected = () => (s.connection === "Connected" ? true : (toast("no device connected"), false));

  const apply = (c) => {
    switch (c.type) {
      case "Status":
        break;
      case "Connect":
        Object.assign(s, { connection: s.enabled ? "Connected" : "Off", bus: c.bus, battery: null, low_battery: false, held_keys: [] });
        break;
      case "Disconnect":
        Object.assign(s, { connection: s.enabled ? "Absent" : "Off", bus: null, battery: null, low_battery: false, held_keys: [] });
        break;
      case "Battery":
        if (connected()) Object.assign(s, { battery: c.level, charging: c.charging, low_battery: c.level < 15 });
        break;
      case "Key": {
        if (!connected()) break;
        s.held_keys = s.held_keys.filter((k) => k !== c.key).concat(c.down ? [c.key] : []);
        if (!c.down) break;
        if (MODES[c.key]) s.jog_mode = MODES[c.key];
        const action = map[s.page]?.[c.key];
        if (action?.startsWith("page ")) s.page = action.slice(5);
        else if (action) toast(`would press ${action}`);
        break;
      }
      case "Wheel":
        if (connected()) toast(s.jog_mode === "Scroll" ? `would scroll ${c.value}` : `would turn ${s.jog_mode} ${c.value}`);
        break;
      case "SetEnabled":
        s.enabled = c.on;
        s.connection = c.on ? (s.bus ? "Connected" : "Absent") : "Off";
        break;
      case "SendForReal":
        s.send_for_real = c.on;
        break;
      case "LoadSampleMappings":
        map = structuredClone(SAMPLE);
        toast("sample mappings loaded");
        break;
      case "ClearMappings":
        map = {};
        s.page = "base";
        toast("empty setup loaded");
        break;
      case "Resolve":
        s.resolve_open = c.open;
        if (c.open && s.enabled) {
          toast("DaVinci Resolve is open, so freewheel stepped aside.");
          apply({ type: "SetEnabled", on: false });
        } else if (!c.open && !s.enabled) apply({ type: "SetEnabled", on: true });
        break;
      case "Permissions":
        s.permissions = c.state;
        break;
      case "StepUpdate": {
        // As simulator.rs: each press publishes a newer version (src/simUpdate.js runs the rest).
        s.update = { status: "available", version: "99.0.0" };
        break;
      }
      case "ExitOverride":
        if (s.wheel_override) toast("the wheel is back to normal");
        s.wheel_override = null;
        break;
      default:
        toast(`unknown control ${c.type}`);
    }
    s.mapped_keys = Object.values(map).reduce((n, l) => n + Object.keys(l).length, 0);
    s.lit_leds = leds();
    const out = structuredClone(s);
    s.toasts = [];
    return out;
  };

  const invoke = async (cmd, args) => {
    if (cmd === "simulate") return apply(args.control);
    if (cmd === "sim_jump") return void jumps.push(args.to);
    if (cmd === "sim_tray") {
      trayMenu.on = s.enabled;
      trayMenu.status_line = s.permissions === "Missing" ? "Needs permission (click to fix)" : null;
      if (args.item === "toggle") (trayMenu.on = !trayMenu.on), apply({ type: "SetEnabled", on: trayMenu.on });
      if (args.item === "start-at-login") trayMenu.start_at_login = !trayMenu.start_at_login;
      if (args.item !== "State") trayClicks.push(args.item);
      return structuredClone(trayMenu);
    }
    throw new Error(`no command ${cmd}`);
  };
  return { invoke, jumps, trayClicks, state: s };
}
