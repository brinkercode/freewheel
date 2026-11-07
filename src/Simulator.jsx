import { useEffect, useState } from "react";
import UpdateBanner from "./UpdateBanner.jsx";

// Drives the `simulate` command of `freewheel --simulate` (core simulator.rs).
const invoke = (control) => window.__TAURI__.core.invoke("simulate", { control });

export default function Simulator({ initial = null }) {
  const [status, setStatus] = useState(initial);
  const [log, setLog] = useState([]);
  const [battery, setBattery] = useState(80);

  const send = (control) =>
    invoke(control).then((s) => {
      setStatus(s);
      setLog((l) => [...s.toasts, ...l].slice(0, 50));
    });

  useEffect(() => {
    send({ type: "Status" });
  }, []);

  if (!status) return <h1>freewheel Simulator</h1>;
  const connected = status.connection === "Connected";
  // The prototype's controls column: always dark, titled groups of small buttons.
  return (
    <main className="simulator" data-theme="dark">
      <div>
        <h1>freewheel Simulator</h1>
        <p className="sim-sub">Pretend to be the computer and the Speed Editor.</p>
      </div>
      <p className="sim-status">
        {connected ? `Connected over ${status.bus}` : status.connection === "Off" ? "Off" : "Device not found"}
        {status.battery != null && ` · battery ${status.battery}%${status.charging ? " charging" : ""}`}
        {status.low_battery && " · low battery"}
        {` · page ${status.page} · LEDs ${status.lit_leds.join(", ") || "none"}`}
      </p>
      <h2 className="sim-title">Press a key on the Speed Editor</h2>
      <section className="sim-row">
        {status.keys.map((key) => (
          <button
            key={key}
            className="sim-key"
            aria-pressed={status.held_keys.includes(key)}
            onMouseDown={() => send({ type: "Key", key, down: true })}
            onMouseUp={() => send({ type: "Key", key, down: false })}
          >
            {key}
          </button>
        ))}
      </section>
      <ul className="sim-log">
        {log.length === 0 && <li className="sim-log-empty">What the Speed Editor did shows up here.</li>}
        {log.map((line, i) => (
          <li key={i}>{line}</li>
        ))}
      </ul>
      <h2 className="sim-title">Speed Editor</h2>
      <section className="sim-row">
        <button onClick={() => send({ type: "Connect", bus: "Usb" })}>Connect USB</button>
        <button onClick={() => send({ type: "Connect", bus: "Bluetooth" })}>Connect Bluetooth</button>
        <button onClick={() => send({ type: "Disconnect" })}>Disconnect</button>
        <button onClick={() => send({ type: "SetEnabled", on: !status.enabled })}>
          {status.enabled ? "Turn Off" : "Turn On"}
        </button>
        <button onClick={() => send({ type: "LoadSampleMappings" })}>Load sample mappings</button>
      </section>
      <h2 className="sim-title">Computer</h2>
      <section className="sim-row">
        <button onClick={() => send({ type: "Resolve", open: !status.resolve_open })}>
          {status.resolve_open ? "Close Resolve" : "Open Resolve"}
        </button>
        {["Granted", "Missing"].map((state) => (
          <button key={state} disabled={status.permissions === state} onClick={() => send({ type: "Permissions", state })}>
            {`Permissions ${state.toLowerCase()}`}
          </button>
        ))}
        <button onClick={() => send({ type: "StepUpdate" })}>Next update state</button>
        <button onClick={() => send({ type: "ResetFirstRun" })}>Reset to first run</button>
        {status.first_run && " · first run"}
      </section>
      <UpdateBanner update={status.update} />
      <h2 className="sim-title">Battery</h2>
      <section className="sim-row">
        <label>
          Battery {battery}%
          <input type="range" min="0" max="100" value={battery} onChange={(e) => setBattery(Number(e.target.value))} />
        </label>
        <button onClick={() => send({ type: "Battery", level: battery, charging: false })}>Send battery</button>
        <button onClick={() => send({ type: "Battery", level: battery, charging: true })}>Send charging</button>
      </section>
      <h2 className="sim-title">Wheel</h2>
      <section className="sim-row">
        {["Scroll", "Jog", "Shuttle"].map((mode) => (
          <button key={mode} disabled={status.jog_mode === mode} onClick={() => send({ type: "WheelMode", mode })}>
            {mode}
          </button>
        ))}
        <button onClick={() => send({ type: "Wheel", value: -1 })}>Wheel left</button>
        <button onClick={() => send({ type: "Wheel", value: 1 })}>Wheel right</button>
      </section>
      <label className="check-row">
        <input
          type="checkbox"
          checked={status.send_for_real}
          onChange={(e) => send({ type: "SendForReal", on: e.target.checked })}
        />
        Send for real
      </label>
    </main>
  );
}
