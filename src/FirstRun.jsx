import { useState } from "react";
import { PermissionsScreen } from "./Permissions.jsx";
import { openBluetoothSettings } from "./settingsApi.js";
import { connectionText } from "./shell.js";

const openBluetooth = () =>
  openBluetoothSettings().catch((error) => console.error("freewheel: opening Bluetooth settings failed:", String(error)));

/** Progress dots, the current step wider. */
function Dots({ step, steps }) {
  return (
    <div className="first-run-dots" style={{ display: "flex", gap: 6, marginBottom: 30 }}>
      {Array.from({ length: steps }, (_, i) => (
        <span key={i} aria-current={i === step} style={{ width: i === step ? 22 : 6, height: 6, borderRadius: 3 }} />
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
    <main className="first-run" style={{ display: "flex", flexDirection: "column", alignItems: "center", padding: 32 }}>
      <Dots step={step} steps={asksPermissions ? 3 : 2} />
      {step === 0 && (
        <section style={{ maxWidth: 440, textAlign: "center" }}>
          <h1>Welcome to freewheel</h1>
          <p>
            Use your Speed Editor for everyday things — copy and paste, volume, opening your favorite apps — whenever
            DaVinci Resolve is closed.
          </p>
          <button onClick={next}>Get started</button>
          <div>Takes about a minute</div>
        </section>
      )}
      {step === 1 && (
        <section style={{ maxWidth: 460, textAlign: "center" }}>
          <h1>Plug in or pair your Speed Editor</h1>
          <p>Connect it with its USB cable. Or switch it on and pair it in your computer’s Bluetooth settings.</p>
          {state.connected ? (
            <div className="found" role="status">
              <span aria-hidden="true">✓</span> <strong>Found it!</strong> <span>{connectionText(state.bus)}</span>
            </div>
          ) : (
            <div className="looking" role="status">
              <span className="spinner" aria-hidden="true" /> <strong>Looking for your Speed Editor…</strong>{" "}
              <span>This usually takes a few seconds.</span>
            </div>
          )}
          <button onClick={onBluetooth}>Open Bluetooth settings</button>{" "}
          <button disabled={!state.connected} onClick={next}>
            Continue
          </button>
        </section>
      )}
      {step === 2 && <PermissionsScreen initial={initialPermissions} button="Start using freewheel" onDone={onFinish} />}
    </main>
  );
}
