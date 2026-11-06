import { useState } from "react";
import { exportMappings, importErrorText, importMappings } from "./settingsApi.js";

/** Back up: resolves to the toast text, or null when the user cancelled. */
export async function backUp(exportFn = exportMappings) {
  return (await exportFn()) ? "Your keys are backed up." : null;
}

/**
 * Restore: src-tauri asks "Replace your keys?" and backs up config.json
 * first. Resolves to the toast text, or null when the user cancelled.
 */
export async function restore(allowApps, importFn = importMappings) {
  return (await importFn(allowApps === true)) ? "Your keys are restored." : null;
}

/** The Help & backup tab's Back up… and Restore from a backup… buttons. */
export default function Backup({ onToast = () => {}, initialAllowApps = false }) {
  const [allowApps, setAllowApps] = useState(initialAllowApps);
  const [error, setError] = useState(null);
  const run = (action) => {
    setError(null);
    action().then(
      (text) => text && onToast(text),
      (e) => setError(importErrorText(e)),
    );
  };
  return (
    <section id="backup">
      <h2>Back up your keys</h2>
      <button onClick={() => run(() => backUp())}>Back up…</button>{" "}
      <button onClick={() => run(() => restore(allowApps))}>Restore from a backup…</button>
      <p>
        <label>
          <input type="checkbox" checked={allowApps} onChange={(e) => setAllowApps(e.target.checked)} /> Let restored
          keys open apps
        </label>
      </p>
      <p>Leave this off for a file someone sent you: a key that opens an app can run any program.</p>
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
