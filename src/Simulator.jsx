import { useEffect, useRef, useState } from "react";
import { LABELS } from "./keys.js";
import { SIM_LOG_EVENT, SIM_TRAY_EVENT } from "./simulateEvents.js";
import { WindowControls, isMac } from "./TitleBar.jsx";
import "./styles/simulator.css";

// The `freewheel --simulate` window: the prototype's dark "Prototype controls"
// column. It drives the simulated Speed Editor (`simulate`, core simulator.rs)
// and jumps the settings window to a screen (`sim_jump`, src-tauri simulate.rs).
const tauri = () => globalThis.window?.__TAURI__;
const simulate = (control) => tauri().core.invoke("simulate", { control });
const jump = (to) => tauri().core.invoke("sim_jump", { to });
const trayClick = (item) => tauri().core.invoke("sim_tray", { item });
const simNotes = (choice, text) => tauri().core.invoke("sim_notes", { choice, text: text ?? null });
// `to`: Settings, FirstRun, RecordKey, Light, Dark, Reload or Quit.

/** The prototype's quick keys, by binding name. */
export const QUICK_KEYS = ["Cut", "In", "Esc", "Trans", "FullView", "Cam1", "Jog", "Shtl", "Scrl"];

const label = (key) => LABELS[key] ?? key;

/** Which "Speed Editor" option the status matches; null when none does. */
export function deviceOption(s) {
  if (!s || s.connection == null || s.connection === "Absent") return "none";
  if (s.connection !== "Connected") return null;
  if (s.bus === "Usb") return "usb";
  if (s.bus === "Bluetooth") return s.battery === 12 ? "btlow" : s.battery === 64 ? "bt" : null;
  return null;
}

/** The tray icon the app shows now: on, off, absent or attention (the tray's own, `menu.icon`, when known). */
export function trayState(s, menu = null) {
  if (menu?.icon) return menu.icon;
  if (!s) return "absent";
  if (!s.enabled) return "off";
  if (s.connection !== "Connected") return "absent";
  if (s.low_battery || s.permissions === "Missing") return "attention";
  return "on";
}

const DEVICE = [
  ["none", "Not found", [{ type: "Disconnect" }]],
  ["usb", "USB", [{ type: "Connect", bus: "Usb" }]],
  ["bt", "Bluetooth 64%", [{ type: "Connect", bus: "Bluetooth" }, { type: "Battery", level: 64, charging: false }]],
  ["btlow", "Bluetooth 12%", [{ type: "Connect", bus: "Bluetooth" }, { type: "Battery", level: 12, charging: false }]],
];

const TRAY = [
  ["on", "On"],
  ["off", "Off / paused"],
  ["absent", "Not connected"],
  ["attention", "Needs attention"],
];

/**
 * The tray menu's items, by id (src-tauri tray.rs), in its order and as it
 * words them: [id, text, checked?, enabled?]. A disabled item is still
 * clickable here, and the app logs that nothing happened, as the OS menu
 * ignores the click.
 */
export const TRAY_ITEMS = [
  ["toggle", (m) => (m?.on === false ? "freewheel is Off" : "freewheel is On"), (m) => m?.on !== false],
  ["status", (m) => m?.status_line || "Looking for your Speed Editor…", null, (m) => m?.status_fix === true],
  ["open-settings", () => "Open settings…"],
  ["start-at-login", () => "Start at login", (m) => m?.start_at_login === true],
  ["download-logs", () => "Download logs for support…"],
  ["update", (m) => m?.update || "Check for updates", null, (m) => m?.update_enabled !== false],
  ["quit", () => "Quit freewheel"],
];
/** The status line: 'Needs permission (click to fix)' is the only clickable one. */
export const TRAY_STATUS = "status";

// "New version" publishes a newer version on the simulated update server;
// the settings window finds it by itself, as after a real release.
const UPDATE_TEXT = {
  available: () => "New version published: freewheel finds it by itself, or with Check for updates.",
};

/**
 * The notes the next "New version" carries (src-tauri selfupdate.rs
 * NotesChoice): the next release's real notes (embedded at build from
 * scripts/release-notes.mjs preview), the sample ones, or pasted ones.
 */
export const NOTES_CHOICES = [
  ["Next", "Next release (real notes)"],
  ["Sample", "Sample notes"],
  ["Paste", "Paste notes…"],
];

/** A one-line preview of notes markdown: how many bullets, and the first one. */
export function notesPreview(markdown) {
  const items = String(markdown ?? "")
    .split(/\r?\n/)
    .map((l) => l.match(/^\s*[*-]\s+(.*)$/)?.[1])
    .filter(Boolean)
    .map((t) => t.replace(/\*\*|`/g, "").replace(/\[([^\]]*)\]\([^)]*\)/g, "$1"));
  if (!items.length) return String(markdown ?? "").trim() ? "Notes without bullets." : "No notes: What’s new links to GitHub only.";
  return `${items.length} ${items.length === 1 ? "change" : "changes"}: ${items[0]}`;
}

/** A titled row of options, one of them current (the prototype's simRows). */
function Choice({ title, options, current, onPick, disabled = [], children }) {
  return (
    <section className="fw-sim-group">
      <h2 className="fw-sim-label">{title}</h2>
      <div className="fw-sim-seg" role="group" aria-label={title}>
        {options.map(([value, text]) => (
          <button key={String(value)} aria-pressed={current === value} disabled={disabled.includes(value)} onClick={() => onPick(value)}>
            {text}
          </button>
        ))}
      </div>
      {children}
    </section>
  );
}

export default function Simulator({ initial = null }) {
  const [status, setStatus] = useState(initial);
  const [log, setLog] = useState([]);
  const [pick, setPick] = useState("Cut");
  const [dark, setDark] = useState(false);
  // What is plugged in, as picked: the status only names the bus while freewheel is On.
  const [plug, setPlug] = useState(null);
  const [trayMenu, setTrayMenu] = useState(null);
  // The next simulated update's notes (sim_notes): { choice, notes, next_available }.
  const [notes, setNotes] = useState(null);
  const [pasted, setPasted] = useState("");
  const turning = useRef(null);
  const down = useRef(new Set());

  const note = (lines, kind = "out") => {
    if (lines.length) setLog((l) => [...lines.map((t) => ({ t, kind })).reverse(), ...l].slice(0, 40));
  };

  // Controls run in order; each answer's toasts go to the log.
  const send = (...controls) =>
    controls
      .reduce(
        (p, c) =>
          p.then(() => simulate(c)).then((s) => {
            note(s.toasts);
            return s;
          }),
        Promise.resolve(),
      )
      .then(
        (s) => {
          setStatus(s);
          // The tray menu follows the device (its On/Off check, the permission line).
          trayClick("State").then(setTrayMenu, () => {});
        },
        (e) => note([String(e)], "error"),
      );

  const go = (to, said) =>
    jump(to).then(
      () => said && note([said], "info"),
      (e) => note([String(e)], "error"),
    );

  const tray = (item) =>
    trayClick(item).then(
      (m) => (setTrayMenu(m), item !== "State" && send({ type: "Status" })),
      (e) => note([String(e)], "error"),
    );

  useEffect(() => {
    if (!tauri()) return () => clearInterval(turning.current);
    send({ type: "Status" });
    tray("State");
    simNotes("State").then(setNotes, (e) => note([String(e)], "error"));
    // What the app did for the settings window, the tray and device
    // commands (src-tauri simulate.rs note): shown as it happens.
    const unlisten = [
      tauri().event?.listen(SIM_LOG_EVENT, (e) => note(e.payload ?? [], "app")),
      // The tray menu changed by itself (the device, the settings window's update flow).
      tauri().event?.listen(SIM_TRAY_EVENT, (e) => e.payload && setTrayMenu(e.payload)),
    ];
    return () => {
      clearInterval(turning.current);
      unlisten.forEach((u) => Promise.resolve(u).then((stop) => stop?.(), () => {}));
    };
  }, []);

  const pickNotes = (choice, text) =>
    simNotes(choice, text).then(
      (n) => {
        setNotes(n);
        note([`New version's notes: ${NOTES_CHOICES.find(([c]) => c === n.choice)?.[1] ?? n.choice}`], "info");
      },
      (e) => note([String(e)], "error"),
    );

  const keyDown = (key) => {
    down.current.add(key);
    note([`You pressed ${label(key)}`], "you");
    return send({ type: "Key", key, down: true });
  };
  const keyUp = (key) => {
    if (!down.current.delete(key)) return Promise.resolve();
    return send({ type: "Key", key, down: false });
  };
  const turnStart = (value) => {
    clearInterval(turning.current);
    note([value < 0 ? "You turned the wheel left" : "You turned the wheel right"], "you");
    send({ type: "Wheel", value });
    turning.current = setInterval(() => send({ type: "Wheel", value }), 120);
  };
  const turnStop = () => clearInterval(turning.current);

  const keys = status?.keys ?? Object.keys(LABELS);
  const held = status?.held_keys ?? [];
  const lit = status?.lit_leds ?? [];
  const update = status?.update;
  const trayIcon = trayState(status, trayMenu);

  return (
    <main className={`fw-sim${isMac() ? " is-mac" : ""}`}>
      {/* The window's header (no OS title bar): drag it to move the window. */}
      <header className="fw-sim-head" data-tauri-drag-region="deep">
        <h1>Simulator controls</h1>
        <WindowControls />
        <p>Pretend to be the computer and the Speed Editor.</p>
      </header>

      <section className="fw-sim-group fw-sim-press">
        <h2 className="fw-sim-label">Press a key on the Speed Editor</h2>
        <div className="fw-sim-quick">
          {QUICK_KEYS.map((key) => (
            <button
              key={key}
              aria-pressed={held.includes(key)}
              className={lit.includes(key) ? "is-lit" : undefined}
              onPointerDown={() => keyDown(key)}
              onPointerUp={() => keyUp(key)}
              onPointerLeave={() => keyUp(key)}
            >
              {label(key)}
              {lit.includes(key) && <span className="fw-sim-led" aria-hidden="true" />}
            </button>
          ))}
        </div>
        <div className="fw-sim-line">
          <select aria-label="Key to press" value={pick} onChange={(e) => setPick(e.target.value)}>
            {keys.map((key) => (
              <option key={key} value={key}>
                {label(key)}
              </option>
            ))}
          </select>
          <button className="fw-sim-primary" onClick={() => keyDown(pick).then(() => keyUp(pick))}>
            Press
          </button>
        </div>
        <div className="fw-sim-line">
          <button onPointerDown={() => turnStart(-1)} onPointerUp={turnStop} onPointerLeave={turnStop}>
            ↺ Turn wheel left
          </button>
          <button onPointerDown={() => turnStart(1)} onPointerUp={turnStop} onPointerLeave={turnStop}>
            Turn right ↻
          </button>
        </div>
        <ol className="fw-sim-log" aria-label="What the Speed Editor did" aria-live="polite">
          {log.length === 0 && <li className="is-empty">What the Speed Editor did shows up here.</li>}
          {log.map((line, i) => (
            <li key={log.length - i} className={`is-${line.kind}`}>
              {line.t}
            </li>
          ))}
        </ol>
        {status && (
          <p className="fw-sim-meta">
            {`Page ${status.page} · Wheel ${status.jog_mode} · Lit ${status.lit_leds.join(", ") || "none"}`}
          </p>
        )}
      </section>

      <Choice
        title="Computer appearance"
        options={[
          [false, "Light"],
          [true, "Dark"],
        ]}
        current={dark}
        onPick={(d) => go(d ? "Dark" : "Light").then(() => setDark(d))}
      />
      <Choice
        title="Speed Editor"
        options={DEVICE}
        current={plug ?? deviceOption(status)}
        onPick={(v) => {
          setPlug(v);
          send(...DEVICE.find(([d]) => d === v)[2]);
        }}
      />
      <Choice
        title="DaVinci Resolve"
        options={[
          [false, "Closed"],
          [true, "Open"],
        ]}
        current={status?.resolve_open ?? false}
        onPick={(open) => send({ type: "Resolve", open })}
      />
      <Choice
        title="freewheel"
        options={[
          [true, "On"],
          [false, "Off"],
        ]}
        current={status?.enabled ?? true}
        onPick={(on) => send({ type: "SetEnabled", on })}
      />
      <Choice
        title="macOS permission"
        options={[
          ["Granted", "Allowed"],
          ["Missing", "Missing"],
        ]}
        current={status?.permissions ?? "Granted"}
        onPick={(state) => send({ type: "Permissions", state })}
      />
      <Choice
        title="Keys"
        options={[
          ["sample", "Several set (2 pages)"],
          ["empty", "Empty setup"],
        ]}
        current={status ? (status.mapped_keys > 0 ? "sample" : "empty") : null}
        onPick={(v) => send({ type: v === "sample" ? "LoadSampleMappings" : "ClearMappings" })}
      />

      <section className="fw-sim-group">
        <h2 className="fw-sim-label">Jump to</h2>
        <div className="fw-sim-grid">
          <button onClick={() => go("FirstRun", "First run opened")}>First run</button>
          <button onClick={() => go("Settings")}>Settings window</button>
          <button onClick={() => go("RecordKey", "Recording a shortcut for CUT")}>Record a key</button>
          <button onClick={() => send({ type: "StepUpdate" })}>New version</button>
          <button onClick={() => go("Reload", "Settings window reloaded")}>Reload settings</button>
          <button onClick={() => go("Quit")}>Quit app</button>
        </div>
        {update && <p className="fw-sim-meta">{UPDATE_TEXT[update.status]?.(update.version)}</p>}
      </section>

      <Choice
        title="New version’s notes"
        options={NOTES_CHOICES}
        current={notes?.choice ?? null}
        disabled={notes && !notes.next_available ? ["Next"] : []}
        onPick={(choice) => pickNotes(choice, choice === "Paste" ? pasted : undefined)}
      >
        {notes?.choice === "Paste" && (
          <div className="fw-sim-paste">
            <textarea
              aria-label="Pasted notes"
              placeholder={"### Features\n\n* **keys:** what changed"}
              value={pasted}
              onChange={(e) => setPasted(e.target.value)}
              rows={5}
            />
            <button onClick={() => pickNotes("Paste", pasted)}>Use these notes</button>
          </div>
        )}
        {notes && (
          <p className="fw-sim-meta" data-notes-preview>
            {notes.next_available ? "" : "This build has no next-release notes. "}
            {notesPreview(notes.notes)}
          </p>
        )}
      </Choice>

      <section className="fw-sim-group">
        <h2 className="fw-sim-label">Tray icon states</h2>
        <div className="fw-sim-tray" role="list" aria-label="Tray icon states">
          {TRAY.map(([state, text]) => (
            <span key={state} role="listitem" className="fw-sim-tray-item" aria-current={trayIcon === state}>
              <span className={`fw-tray-icon is-${state}`} aria-hidden="true">
                <span className="ring" />
                <span className="dot" />
                {state === "attention" && <span className="badge" />}
              </span>
              {text}
            </span>
          ))}
        </div>
      </section>

      <section className="fw-sim-group">
        <h2 className="fw-sim-label">Tray menu</h2>
        <div className="fw-sim-grid fw-sim-tray-menu" role="group" aria-label="Tray menu">
          {TRAY_ITEMS.map(([id, text, checked, enabled]) => (
            <button
              key={id}
              data-tray={id}
              aria-pressed={checked ? checked(trayMenu) : undefined}
              aria-disabled={enabled ? !enabled(trayMenu) : undefined}
              onClick={() => tray(id)}
            >
              {text(trayMenu)}
            </button>
          ))}
        </div>
      </section>

      <label className="fw-sim-switch">
        <span className="fw-sim-switch-text">
          <span className="fw-sim-switch-title">Send keys for real</span>
          <span className="fw-sim-switch-sub">
            {status?.send_for_real ? "Shortcuts reach the apps you have open." : "Off: shortcuts only show up in the log."}
          </span>
        </span>
        <input
          type="checkbox"
          role="switch"
          checked={status?.send_for_real ?? false}
          onChange={(e) => send({ type: "SendForReal", on: e.target.checked })}
        />
      </label>
    </main>
  );
}
