import { useEffect, useState } from "react";

// macOS only: get_permissions is empty elsewhere (src-tauri permissions.rs).
const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);

export const LABELS = { Accessibility: "Accessibility", InputMonitoring: "Input Monitoring" };

export default function Permissions({ initial = null, poll = 2000 }) {
  const [permissions, setPermissions] = useState(initial);
  useEffect(() => {
    if (initial) return undefined;
    const check = () => invoke("get_permissions").then(setPermissions, () => {});
    check();
    const timer = setInterval(check, poll);
    return () => clearInterval(timer);
  }, [initial, poll]);

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
