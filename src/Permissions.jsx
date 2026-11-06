import { useEffect, useState } from "react";

// macOS only: get_permissions is empty elsewhere (src-tauri permissions.rs).
const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);

export const LABELS = { Accessibility: "Accessibility", InputMonitoring: "Input Monitoring" };

const ABOUT = {
  Accessibility: "Lets freewheel press keys and shortcuts for you.",
  InputMonitoring: "Lets freewheel hear the keys on your Speed Editor.",
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

/** Polls freewheel-m5-5's checks (get_permissions) unless `initial` is given. */
function usePermissions(initial, poll) {
  const [permissions, setPermissions] = useState(initial);
  useEffect(() => {
    if (initial) return undefined;
    const check = () =>
      invoke("get_permissions").then(setPermissions, (error) =>
        console.error("freewheel: checking permissions failed:", String(error)),
      );
    return watch(check, window, poll);
  }, [initial, poll]);
  return permissions;
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
    <section id="permissions">
      <h2>Permissions</h2>
      {Object.values(permissions).includes("Missing") && <p>Needs permission</p>}
      {names.map((name) => (
        <div key={name}>
          <span>{LABELS[name] ?? name}</span>{" "}
          {permissions[name] === "Granted" ? (
            <span>✓ Allowed</span>
          ) : (
            <button onClick={() => invoke("open_permission_pane", { permission: name })}>
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
    <section className="first-run" style={{ maxWidth: 540, margin: "0 auto", display: "flex", flexDirection: "column", gap: 12 }}>
      <h1 style={{ textAlign: "center" }}>Allow freewheel on your Mac</h1>
      <p style={{ textAlign: "center" }}>
        macOS asks you to allow these once. Click a button, switch freewheel on in System Settings, then come back here.
      </p>
      {names.map((name) => {
        const state = rowState(permissions[name], asked.includes(name));
        return (
          <div key={name} className={`permission-row ${state}`} style={{ display: "flex", alignItems: "center", gap: 14, padding: 16 }}>
            <div style={{ flex: 1 }}>
              <strong>{LABELS[name] ?? name}</strong>
              <div>{ABOUT[name]}</div>
            </div>
            {state === "granted" && (
              <span className="allowed">
                <span aria-hidden="true">✓</span> <span>Allowed</span>
              </span>
            )}
            {state === "waiting" && (
              <span className="waiting">
                <span className="spinner" aria-hidden="true" /> Waiting…
              </span>
            )}
            {state === "idle" && (
              <button
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
      <button style={{ alignSelf: "center" }} disabled={!allowed} onClick={onDone}>
        {button}
      </button>
    </section>
  );
}
