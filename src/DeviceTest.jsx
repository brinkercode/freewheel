import { useEffect, useState } from "react";
import Icon from "./Icon.jsx";
import { getDeviceDiagnostics, onDeviceDiagnostics } from "./settingsApi.js";

// Help & backup > More > Device test: a read-only view of what the Speed
// Editor sends (connection, auth, battery, keys, the wheel and the last raw
// reports), live from the `device-diagnostics` event. In --simulate it shows
// the simulated device, fed through the same connection manager.

const STATUS_TEXT = {
  Connected: "Connected",
  Absent: "Not connected",
  Off: "Off",
  AuthFailed: "Found, but it did not unlock (auth failed)",
};

const AUTH_TEXT = { NotYet: "Not yet", Passed: "Passed", Failed: "Failed" };

const hex4 = (n) => `0x${Number(n ?? 0).toString(16).padStart(4, "0")}`;

/** "USB", "Bluetooth" or "—". */
export const busText = (bus) => (bus === "Usb" ? "USB" : bus === "Bluetooth" ? "Bluetooth" : "—");

/** The status dot class, as the toolbar uses them. */
export const dotClass = (status) => (status === "Connected" ? "dot-green" : status === "AuthFailed" ? "dot-orange" : "dot-gray");

/** Seconds as "9 min 50 s" / "42 s". */
export function secondsText(s) {
  if (s == null) return "—";
  return s >= 60 ? `${Math.floor(s / 60)} min ${s % 60} s` : `${s} s`;
}

/** The diagnostics as plain text, for support ("Copy diagnostics"). */
export function diagnosticsText(d, { version = "", platform = globalThis.navigator?.userAgent ?? "" } = {}) {
  if (!d) return `freewheel ${version} device test: no data yet`;
  const i = d.device;
  const a = d.auth;
  const w = d.wheel;
  const lines = [
    `freewheel ${version} device test`,
    `platform: ${platform}`,
    ...(d.simulated ? ["SIMULATED device (--simulate)"] : []),
    `status: ${d.status ?? "none yet"}`,
    i
      ? `device: bus ${busText(i.bus)}, vid ${hex4(i.vendor_id)} pid ${hex4(i.product_id)}, usage page ${hex4(i.usage_page)} usage ${hex4(i.usage)}, interface ${i.interface}, release ${hex4(i.release)}, product ${i.product ?? "—"}, ${i.candidates} interface(s) listed`
      : "device: none opened yet",
    ...(i?.report_descriptor ? [`report descriptor: ${i.report_descriptor}`] : []),
    `auth: ${a.state}, passed ${a.passed}, failed ${a.failed}, session ${a.session_s ?? "—"} s, next in ${a.next_in_s ?? "—"} s, last error ${a.last_error ?? "none"}`,
    `battery: ${d.battery ? `${d.battery.level}%${d.battery.charging ? " charging" : ""}` : "no report yet"}`,
    `held: ${d.held.join(", ") || "none"}`,
    `presses (newest first): ${d.presses.join(", ") || "none"}`,
    `wheel: mode ${w.device_mode ?? "—"}, last ${w.last_value ?? "—"}, total ${w.total}, reports ${w.reports}`,
    `reports by id: ${Object.entries(d.counts).map(([id, n]) => `${id}=${n}`).join(" ") || "none"}; undecoded ${d.undecoded}`,
    `opens ${d.open_attempts}, connects ${d.connects}, disconnects ${d.disconnects}, last open error ${d.last_open_error ?? "none"}, uptime ${d.uptime_ms} ms`,
    "reports (newest first):",
    ...d.reports.map((r) => `  #${r.seq} +${r.at_ms}ms ${r.direction === "In" ? "in " : "out"} ${r.hex}  (${r.decoded})${r.failed ? " FAILED" : ""}`),
  ];
  return lines.join("\n") + "\n";
}

/** Copies `text`: the Clipboard API, else a selected textarea. */
export async function copyText(text, doc = globalThis.document, nav = globalThis.navigator) {
  if (nav?.clipboard?.writeText) {
    try {
      await nav.clipboard.writeText(text);
      return;
    } catch {
      // fall through to the textarea copy
    }
  }
  const area = doc.createElement("textarea");
  area.value = text;
  doc.body.appendChild(area);
  area.select();
  const ok = doc.execCommand?.("copy");
  area.remove();
  if (!ok) throw new Error("the clipboard is not available");
}

function Row({ label, children }) {
  return (
    <div className="row diag-row">
      <span className="diag-label">{label}</span>
      <span className="grow diag-value">{children}</span>
    </div>
  );
}

/** The Device test view. */
export default function DeviceTest({
  onBack,
  version = "",
  initial = null,
  get = getDeviceDiagnostics,
  listen = onDeviceDiagnostics,
  copy = copyText,
}) {
  const [d, setD] = useState(initial);
  const [copied, setCopied] = useState(null);
  useEffect(() => {
    if (!globalThis.window?.__TAURI__) return undefined;
    get().then(setD, (e) => console.error("freewheel: device diagnostics:", String(e)));
    const stop = listen(setD);
    return () => Promise.resolve(stop).then((s) => s?.(), () => {});
  }, [get, listen]);
  const onCopy = () =>
    copy(diagnosticsText(d, { version })).then(
      () => setCopied("Diagnostics copied. Paste them into your reply."),
      (e) => setCopied(`Could not copy: ${String(e?.message ?? e)}`),
    );
  const i = d?.device;
  const a = d?.auth;
  const w = d?.wheel;
  return (
    <div className="tab-page" data-screen-label="Device test">
      <div className="tab-column">
        <button className="more" onClick={onBack}>
          <Icon name="chevron_left" />
          Help &amp; backup
        </button>
        <div className="row">
          <h2 className="tab-title grow">Device test</h2>
          <button className="btn-primary" onClick={onCopy} disabled={!d}>
            Copy diagnostics
          </button>
        </div>
        <div className="card-sub">Shows what your Speed Editor sends, as it sends it. Nothing here changes your keys.</div>
        {copied && (
          <div className="success-line" role="status">
            <Icon name="check_circle" className="icon success-icon" />
            <span className="grow">{copied}</span>
          </div>
        )}
        {d?.simulated && (
          <div className="tip-card">
            <Icon name="info" />
            <span>Simulated Speed Editor: these reports come from the Simulator window.</span>
          </div>
        )}
        {!d && <div className="card-text">Waiting for the device loop…</div>}
        {d && (
          <>
            <section className="card card-stack" aria-label="Connection">
              <div className="card-title">Connection</div>
              <Row label="Status">
                <span className="row diag-status">
                  <span className={`status-dot ${dotClass(d.status)}`} />
                  {STATUS_TEXT[d.status] ?? "Starting…"}
                </span>
              </Row>
              <Row label="Connected by">{busText(i?.bus)}</Row>
              <Row label="Device">
                {i ? `${hex4(i.vendor_id)}:${hex4(i.product_id)} · usage page ${hex4(i.usage_page)} · interface ${i.interface} · ${i.candidates} listed` : "None opened yet"}
              </Row>
              <Row label="Tries">{`${d.open_attempts} opens · ${d.connects} connects · ${d.disconnects} disconnects`}</Row>
              {d.last_open_error && <Row label="Last open error">{d.last_open_error}</Row>}
            </section>
            <section className="card card-stack" aria-label="Unlock">
              <div className="card-title">Unlock (auth)</div>
              <Row label="Last handshake">{AUTH_TEXT[a.state]}</Row>
              <Row label="Handshakes">{`${a.passed} passed · ${a.failed} failed`}</Row>
              <Row label="Session">{a.session_s == null ? "—" : secondsText(a.session_s)}</Row>
              <Row label="Next check in">{secondsText(a.next_in_s)}</Row>
              {a.last_error && <Row label="Last error">{a.last_error}</Row>}
            </section>
            <section className="card card-stack" aria-label="Battery">
              <div className="card-title">Battery</div>
              <Row label="Level">{d.battery ? `${d.battery.level}%${d.battery.charging ? " (charging)" : ""}` : "No battery report yet"}</Row>
            </section>
            <section className="card card-stack" aria-label="Keys">
              <div className="card-title">Keys</div>
              <Row label="Held now">{d.held.length ? d.held.join(", ") : "None"}</Row>
              <Row label="Pressed (newest first)">{d.presses.length ? d.presses.join(", ") : "Press a key on the Speed Editor"}</Row>
            </section>
            <section className="card card-stack" aria-label="Wheel">
              <div className="card-title">Wheel</div>
              <Row label="Device mode">{w.device_mode == null ? "—" : `${w.device_mode} (${w.device_mode === 1 || w.device_mode === 3 ? "absolute" : "relative"})`}</Row>
              <Row label="Last value">{w.last_value == null ? "Turn the wheel" : w.last_value > 0 ? `+${w.last_value}` : String(w.last_value)}</Row>
              <Row label="Total">{String(w.total)}</Row>
              <Row label="Reports">{String(w.reports)}</Row>
            </section>
            <section className="card card-stack" aria-label="Last reports">
              <div className="card-title">Last reports</div>
              {d.reports.length === 0 && <div className="card-sub small">No reports yet.</div>}
              <ol className="diag-reports">
                {d.reports.map((r) => (
                  <li key={r.seq} className="diag-report" data-direction={r.direction}>
                    <span className="diag-dir">{r.direction === "In" ? "in" : "out"}</span>
                    <code className="diag-hex">{r.hex}</code>
                    <span className="diag-decoded">
                      {r.decoded}
                      {r.failed ? " — write failed" : ""}
                    </span>
                  </li>
                ))}
              </ol>
            </section>
          </>
        )}
      </div>
    </div>
  );
}
