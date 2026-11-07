import { useEffect, useState } from "react";
import DeviceTest from "./DeviceTest.jsx";
import Icon from "./Icon.jsx";
import { Switch } from "./ResolveTab.jsx";
import { exportLogs, exportMappings, importErrorText, importMappings, showLogsFile } from "./settingsApi.js";

// The Help & backup tab (screens-and-components 3f): Download logs, Back up /
// Restore and, under More, the detailed logs switch and the Device test view.

/** Back up: resolves to the confirmation, or null when the user cancelled. */
export async function backUp(exportFn = exportMappings) {
  return (await exportFn()) ? "Your keys are backed up." : null;
}

/**
 * Restore: src-tauri asks "Replace your keys?" and backs up config.json
 * first. Resolves to the confirmation, or null when the user cancelled.
 */
export async function restore(allowApps, importFn = importMappings) {
  return (await importFn(allowApps === true)) ? "Your keys are restored." : null;
}

/** The confirmation naming the saved log file. */
export function logsSavedText(path) {
  const file = String(path ?? "").split(/[\\/]/).pop() || "freewheel-logs.zip";
  return `Downloaded “${file}” to your Downloads folder.`;
}

/** Download logs: resolves to the confirmation naming the saved file. */
export async function downloadLogs(exportFn = exportLogs) {
  return logsSavedText(await exportFn());
}

function Done({ text, onShow }) {
  return (
    <div className="success-line" role="status">
      <Icon name="check_circle" className="icon success-icon" />
      <span className="grow">{text}</span>
      {onShow && (
        <a
          href="#"
          className="show-file"
          onClick={(e) => {
            e.preventDefault();
            onShow();
          }}
        >
          Show file
        </a>
      )}
    </div>
  );
}

/** The Help & backup tab. */
export default function Backup({
  initialAllowApps = false,
  initialMessage = null,
  initialMore = false,
  initialLogs = null,
  initialDeviceTest = false,
  version = "",
  trayLogs = null,
  detailedLogs,
  onDetailedLogs,
  showFile = showLogsFile,
}) {
  const [allowApps, setAllowApps] = useState(initialAllowApps);
  const [error, setError] = useState(null);
  const [message, setMessage] = useState(initialMessage);
  const [logs, setLogs] = useState(initialLogs);
  const [logsError, setLogsError] = useState(null);
  const [more, setMore] = useState(initialMore);
  const [deviceTest, setDeviceTest] = useState(initialDeviceTest);
  // The tray's 'Download logs for support…' saved them (or could not): say so here.
  useEffect(() => {
    if (trayLogs?.path) {
      setLogsError(null);
      setLogs(logsSavedText(trayLogs.path));
    } else if (trayLogs?.error) setLogsError(`The logs could not be saved: ${trayLogs.error}`);
  }, [trayLogs]);
  const run = (action) => {
    setError(null);
    action().then(
      (text) => text && setMessage(text),
      (e) => setError(importErrorText(e)),
    );
  };
  const saveLogs = () => {
    setLogsError(null);
    downloadLogs().then(setLogs, (e) => {
      console.error("freewheel: downloading logs failed:", String(e));
      setLogsError(`The logs could not be saved: ${String(e)}`);
    });
  };
  if (deviceTest) return <DeviceTest version={version} onBack={() => setDeviceTest(false)} />;
  return (
    <div className="tab-page" data-screen-label="Help and backup">
      <div className="tab-column">
        <h2 className="tab-title">Help &amp; backup</h2>
        <section id="logs" className="card card-stack">
          <div className="row">
            <div className="grow">
              <div className="card-title">Download logs for support</div>
              <div className="card-sub">Saves one file you can attach to an email.</div>
            </div>
            <button className="btn-primary" onClick={saveLogs}>
              <Icon name="download" />
              Download logs
            </button>
          </div>
          <div className="privacy-line">
            <Icon name="lock" />
            Logs never include what you type, your shortcuts, or the apps you open.
          </div>
          {logs && (
            <Done
              text={logs}
              onShow={() =>
                showFile().then(
                  () => setLogsError(null),
                  (e) => {
                    console.error("freewheel: showing the log file failed:", String(e));
                    setLogsError(String(e));
                  },
                )
              }
            />
          )}
          {logsError && <p role="alert">{logsError}</p>}
        </section>
        <section id="backup" className="card card-stack">
          <div>
            <div className="card-title">Back up my keys</div>
            <div className="card-sub relaxed">Keep a copy of all your keys and pages in one file — handy for a new computer.</div>
          </div>
          <div className="button-row">
            <button onClick={() => run(() => backUp())}>
              Back up…
            </button>
            <button onClick={() => run(() => restore(allowApps))}>
              Restore from a backup…
            </button>
          </div>
          {message && <Done text={message} />}
          {error && <p role="alert">{error}</p>}
        </section>
        <button className="more" aria-expanded={more} onClick={() => setMore(!more)}>
          More
          <Icon name={more ? "expand_less" : "expand_more"} />
        </button>
        {more && (
          <>
            <section className="card card-row">
              <div className="grow">
                <div className="card-title">Turn on detailed logs for troubleshooting</div>
                <div className="card-sub small">Only if support asks. Turns itself off after a day.</div>
              </div>
              <Switch label="Turn on detailed logs for troubleshooting" on={detailedLogs ?? null} onChange={onDetailedLogs} />
            </section>
            <section className="card card-row">
              <div className="grow">
                <div className="card-title">Let restored keys open apps</div>
                <div className="card-sub small">Leave this off for a file someone sent you: a key that opens an app can run any program.</div>
              </div>
              <Switch label="Let restored keys open apps" on={allowApps} onChange={setAllowApps} />
            </section>
            <section id="device-test" className="card card-row">
              <div className="grow">
                <div className="card-title">Device test</div>
                <div className="card-sub small">See what your Speed Editor sends: connection, battery, keys and the wheel.</div>
              </div>
              <button onClick={() => setDeviceTest(true)}>Open device test</button>
            </section>
          </>
        )}
      </div>
    </div>
  );
}
