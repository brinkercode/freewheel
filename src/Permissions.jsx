import { useEffect, useState } from "react";
import Icon from "./Icon.jsx";

// The prototype's row icons.
const ROW_ICON = { Accessibility: "keyboard", InputMonitoring: "usb" };

// macOS only: get_permissions is empty elsewhere (src-tauri permissions.rs).
const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);

export const LABELS = { Accessibility: "Accessibility", InputMonitoring: "Input Monitoring" };

// The prototype's row titles and helper text; the pane name closes the helper text.
const TITLES = {
  Accessibility: "Allow freewheel to press keys for you",
  InputMonitoring: "Allow freewheel to notice your Speed Editor",
};
const ABOUT = {
  Accessibility: "So your Speed Editor can send shortcuts and change the volume. Opens Privacy & Security → ",
  InputMonitoring: "So it knows when you press a key. Opens Privacy & Security → ",
};

// Runs `check` now, every `poll` ms and whenever `target` (the window) regains
// focus, e.g. on coming back from System Settings; returns the unsubscribe.
export function watch(check, target, poll) {
  check();
  const timer = setInterval(check, poll);
  target.addEventListener("focus", check);
  return () => {
    clearInterval(timer);
    target.removeEventListener("focus", check);
  };
}

/**
 * Polls P1's checks (get_permissions) unless `initial` is given;
 * a given `initial` is followed as it changes (the status event's live view).
 */
function usePermissions(initial, poll) {
  const [polled, setPolled] = useState(null);
  const given = initial != null;
  useEffect(() => {
    if (given) return undefined;
    const check = () =>
      invoke("get_permissions").then(setPolled, (error) =>
        console.error("freewheel: checking permissions failed:", String(error)),
      );
    return watch(check, window, poll);
  }, [given, poll]);
  return initial ?? polled;
}

const openPane = (name) =>
  invoke("open_permission_pane", { permission: name }).catch((error) =>
    console.error("freewheel: opening the", name, "pane failed:", String(error)),
  );

export default function Permissions({ initial = null, poll = 2000 }) {
  const permissions = usePermissions(initial, poll);
  const names = Object.keys(permissions ?? {});
  if (names.length === 0) return null;
  return (
    <section id="permissions" className="permissions-section">
      <h2 className="section-title">Permissions</h2>
      {Object.values(permissions).includes("Missing") && (
        <p className="permissions-status">
          <span className="status-dot dot-orange" aria-hidden="true" />
          Needs permission
        </p>
      )}
      {names.map((name) => (
        <div key={name} className={`card card-row permission-card${permissions[name] === "Granted" ? " granted" : ""}`}>
          <Icon name={ROW_ICON[name] ?? "lock"} className="icon permission-icon" />
          <div className="grow">
            <div className="card-title">{TITLES[name] ?? LABELS[name] ?? name}</div>
            <div className="card-sub">
              {ABOUT[name]}
              <span>{LABELS[name] ?? name}</span>.
            </div>
          </div>
          {permissions[name] === "Granted" ? (
            <span className="allowed">
              <Icon name="check_circle" /> <span>Allowed</span>
            </span>
          ) : (
            <button className="btn-primary" onClick={() => invoke("open_permission_pane", { permission: name })}>
              Open System Settings
            </button>
          )}
        </div>
      ))}
    </section>
  );
}

/** What a row shows: Allowed once granted, Waiting… once its pane was opened, else its button. */
export const rowState = (grant, asked) => (grant === "Granted" ? "granted" : asked ? "waiting" : "idle");

/**
 * The first run's permissions screen, reused by the permission fix with
 * `button="Done"`; `onDone` is enabled only when every permission is allowed.
 */
export function PermissionsScreen({ initial = null, initialAsked = [], poll = 2000, button, onDone }) {
  const permissions = usePermissions(initial, poll);
  const [asked, setAsked] = useState(initialAsked);
  const names = Object.keys(permissions ?? {});
  const allowed = names.length > 0 && names.every((name) => permissions[name] === "Granted");
  return (
    <section className="first-run permissions">
      <h1>{button === "Done" ? "freewheel needs your permission" : "Let freewheel do its job"}</h1>
      <p className="lead">
        macOS asks you to allow these once. Click a button, switch freewheel on in System Settings, then come back here.
      </p>
      {names.map((name) => {
        const state = rowState(permissions[name], asked.includes(name));
        return (
          <div key={name} className={`permission-row ${state}`}>
            <Icon name={ROW_ICON[name] ?? "lock"} className="icon permission-icon" />
            <div className="grow">
              <strong>{TITLES[name] ?? LABELS[name] ?? name}</strong>
              <div className="card-sub">
                {ABOUT[name]}
                <span>{LABELS[name] ?? name}</span>.
              </div>
            </div>
            {state === "granted" && (
              <span className="allowed">
                <Icon name="check_circle" /> <span>Allowed</span>
              </span>
            )}
            {state === "waiting" && (
              <span className="waiting">
                <span className="spinner" aria-hidden="true" /> Waiting…
              </span>
            )}
            {state === "idle" && (
              <button
                className="btn-primary"
                onClick={() => {
                  setAsked((a) => [...a, name]);
                  openPane(name);
                }}
              >
                Open System Settings
              </button>
            )}
          </div>
        );
      })}
      <button className="btn-primary btn-lg" disabled={!allowed} onClick={onDone}>
        {button}
      </button>
    </section>
  );
}
