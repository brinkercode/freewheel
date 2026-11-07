import { TABS, deriveNotice, deriveStatus } from "./shell.js";

/** Tabs, the status dot and text, and the On/Off switch. */
export function Toolbar({ tab, onTab, state, onToggle }) {
  const status = deriveStatus(state);
  return (
    <nav className="toolbar">
      <div className="tabs">
        {TABS.map((t) => (
          <button key={t} className="tab" aria-pressed={t === tab} onClick={() => onTab(t)}>
            {t}
          </button>
        ))}
      </div>
      <span className="status">
        <span className={`status-dot dot-${status.dot}`} />
        {status.text}
      </span>
      <strong className="switch-text">{state.enabled ? "On" : "Off"}</strong>
      <button className="switch" role="switch" aria-checked={state.enabled} title="Turn freewheel on or off" disabled={!onToggle} onClick={onToggle}>
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
    <div role="alert" className={`notice notice-${notice.id}`}>
      <div className="grow">
        <strong>{notice.title}</strong> <span>{notice.text}</span>
      </div>
      {notice.action && (
        <button className="btn" onClick={onAction}>
          {notice.action}
        </button>
      )}
    </div>
  );
}

/** A confirmation, bottom-centre; showToast (shell.js) clears it. */
export function Toast({ text }) {
  if (!text) return null;
  return (
    <div role="status" className="toast">
      {text}
    </div>
  );
}
