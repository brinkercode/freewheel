import { TABS, deriveNotice, deriveStatus } from "./shell.js";

const DOT = { green: "#34c759", orange: "#ff9f0a", gray: "#8e8e93" };

/** Tabs, the status dot and text, and the On/Off switch. */
export function Toolbar({ tab, onTab, state, onToggle }) {
  const status = deriveStatus(state);
  return (
    <nav className="toolbar" style={{ display: "flex", alignItems: "center", gap: 4, padding: "0 14px", height: 52 }}>
      {TABS.map((t) => (
        <button key={t} aria-pressed={t === tab} onClick={() => onTab(t)} style={{ fontWeight: t === tab ? 600 : 400 }}>
          {t}
        </button>
      ))}
      <span style={{ marginLeft: "auto", display: "flex", alignItems: "center", gap: 8 }}>
        <span className="status-dot" style={{ width: 8, height: 8, borderRadius: "50%", background: DOT[status.dot] }} />
        {status.text}
      </span>
      <strong>{state.enabled ? "On" : "Off"}</strong>
      <button role="switch" aria-checked={state.enabled} title="Turn freewheel on or off" disabled={!onToggle} onClick={onToggle}>
        <span className="switch-knob" />
      </button>
    </nav>
  );
}

/** The one notice of the highest priority, or nothing. */
export function NoticeBar({ state, onFix, onTurnOn }) {
  const notice = deriveNotice(state);
  if (!notice) return null;
  const onAction = notice.id === "permission" ? onFix : onTurnOn;
  return (
    <div role="alert" className={`notice notice-${notice.id}`} style={{ display: "flex", gap: 12, padding: "10px 12px 10px 16px" }}>
      <div style={{ flex: 1 }}>
        <strong>{notice.title}</strong> <span>{notice.text}</span>
      </div>
      {notice.action && <button onClick={onAction}>{notice.action}</button>}
    </div>
  );
}

/** A confirmation, bottom-centre; showToast (shell.js) clears it. */
export function Toast({ text }) {
  if (!text) return null;
  return (
    <div
      role="status"
      className="toast"
      style={{ position: "fixed", bottom: 24, left: "50%", transform: "translateX(-50%)" }}
    >
      {text}
    </div>
  );
}
