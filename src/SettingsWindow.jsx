import Icon from "./Icon.jsx";
import { TABS, deriveNotice, deriveStatus } from "./shell.js";

// The prototype's tab icons.
const TAB_ICON = { Keys: "keyboard", Wheel: "trip_origin", "DaVinci Resolve": "movie", General: "tune", "Help & backup": "help" };
// The notice bar's icon per notice; warn notices are tinted, the rest muted.
const NOTICE_ICON = { permission: "lock", off: "power_settings_new", looking: "usb", paused: "pause_circle", battery: "battery_alert" };

/** Tabs, the status dot and text (opens the permission fix when one is missing), and the On/Off switch. */
export function Toolbar({ tab, onTab, state, onToggle, onFix }) {
  const status = deriveStatus(state);
  const fix = state.enabled && state.permissionMissing ? onFix : undefined;
  return (
    <nav className="toolbar">
      <div className="tabs">
        {TABS.map((t) => (
          <button key={t} className="tab" data-icon={TAB_ICON[t]} aria-pressed={t === tab} onClick={() => onTab(t)}>
            {t}
          </button>
        ))}
      </div>
      <button className={`status${fix ? " is-link" : ""}`} onClick={fix} tabIndex={fix ? 0 : -1}>
        <span className={`status-dot dot-${status.dot}`} />
        {status.text}
      </button>
      <span className="toolbar-sep" />
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
      <Icon name={NOTICE_ICON[notice.id]} className="icon notice-icon" />
      <div className="grow">
        <strong>{notice.title}</strong> <span>{notice.text}</span>
      </div>
      {notice.action && (
        <button className="btn-sm" onClick={onAction}>
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
