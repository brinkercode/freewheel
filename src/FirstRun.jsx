import { useState } from "react";
import Icon from "./Icon.jsx";
import { PermissionsScreen } from "./Permissions.jsx";
import { openBluetoothSettings } from "./settingsApi.js";
import { connectionText } from "./shell.js";

const openBluetooth = () =>
  openBluetoothSettings().catch((error) => console.error("freewheel: opening Bluetooth settings failed:", String(error)));

/** Progress dots, the current step wider. */
function Dots({ step, steps }) {
  return (
    <div className="first-run-dots">
      {Array.from({ length: steps }, (_, i) => (
        <span key={i} className={`dot${i <= step ? " is-done" : ""}`} aria-current={i === step} />
      ))}
    </div>
  );
}

/**
 * The first run, in place of the window's content: Welcome, then Plug in or
 * pair (live from `state`, the shell's view of the StatusEvent), then on
 * macOS (where `permissions` is not empty) the permissions screen.
 */
export default function FirstRun({ state, permissions, step: first = 0, initialPermissions, onBluetooth = openBluetooth, onFinish }) {
  const [step, setStep] = useState(first);
  const asksPermissions = Object.keys(permissions ?? {}).length > 0;
  const next = () => (step === 1 && !asksPermissions ? onFinish() : setStep(step + 1));
  return (
    <main className="first-run">
      <Dots step={step} steps={asksPermissions ? 3 : 2} />
      {step === 0 && (
        <section className="first-run-step">
          <span className="logo-disc logo-welcome" aria-hidden="true" />
          <h1 className="welcome">Welcome to freewheel</h1>
          <p className="lead-lg">
            Use your Speed Editor for everyday things — copy and paste, volume, opening your favorite apps — whenever
            DaVinci Resolve is closed.
          </p>
          <button className="btn-primary btn-lg get-started" onClick={next}>
            Get started
          </button>
          <div className="hint">Takes about a minute</div>
        </section>
      )}
      {step === 1 && (
        <section className="first-run-step is-wide">
          <h1>Plug in or pair your Speed Editor</h1>
          <p>Connect it with its USB cable. Or switch it on and pair it in your computer’s Bluetooth settings.</p>
          {state.connected ? (
            <div className="found" role="status">
              <Icon name="check" className="icon found-icon" />
              <div>
                <strong>Found it!</strong>
                <div className="card-sub">{connectionText(state.bus, state.battery)}</div>
              </div>
            </div>
          ) : (
            <div className="looking" role="status">
              <span className="spinner spinner-lg" aria-hidden="true" />
              <div>
                <strong>Looking for your Speed Editor…</strong>
                <div className="card-sub">This usually takes a few seconds.</div>
              </div>
            </div>
          )}
          <div className="button-row first-run-buttons">
            <button className="btn-lg" onClick={onBluetooth}>
              Open Bluetooth settings
            </button>
            <button className="btn-primary btn-lg" disabled={!state.connected} onClick={next}>
              Continue
            </button>
          </div>
        </section>
      )}
      {step === 2 && <PermissionsScreen initial={initialPermissions} button="Start using freewheel" onDone={onFinish} />}
    </main>
  );
}
