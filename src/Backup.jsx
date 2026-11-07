import { useState } from "react";
import Icon from "./Icon.jsx";
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
export default function Backup({ initialAllowApps = false, initialMessage = null }) {
  const [allowApps, setAllowApps] = useState(initialAllowApps);
  const [error, setError] = useState(null);
  const [message, setMessage] = useState(initialMessage);
  const run = (action) => {
    setError(null);
    action().then(
      (text) => text && setMessage(text),
      (e) => setError(importErrorText(e)),
    );
  };
  // Download logs and detailed logs from the prototype are not built yet (GAPS in the PR).
  return (
    <div className="tab-page" data-screen-label="Help and backup">
      <div className="tab-column">
        <h2 className="tab-title">Help &amp; backup</h2>
        <section id="backup" className="card card-stack">
          <div>
            <div className="card-title">Back up my keys</div>
            <div className="card-sub">Keep a copy of all your keys and pages in one file — handy for a new computer.</div>
          </div>
          <div className="button-row">
            <button className="btn" onClick={() => run(() => backUp())}>
              Back up…
            </button>
            <button className="btn" onClick={() => run(() => restore(allowApps))}>
              Restore from a backup…
            </button>
          </div>
          {message && (
            <div className="success-line" role="status">
              <Icon name="check_circle" className="icon success-icon" />
              {message}
            </div>
          )}
          <label className="check-row">
            <input type="checkbox" checked={allowApps} onChange={(e) => setAllowApps(e.target.checked)} /> Let restored keys
            open apps
          </label>
          <p className="card-sub">Leave this off for a file someone sent you: a key that opens an app can run any program.</p>
          {error && <p role="alert">{error}</p>}
        </section>
      </div>
    </div>
  );
}
