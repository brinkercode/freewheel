import Icon from "./Icon.jsx";
import { WindowControls } from "./TitleBar.jsx";
import { TABS, deriveNotice, deriveStatus } from "./shell.js";

// The prototype's tab icons.
const TAB_ICON = { Keys: "keyboard", Wheel: "trip_origin", "DaVinci Resolve": "movie", General: "tune", "Help & backup": "help" };
// The notice bar's icon per notice; warn notices are tinted, the rest muted.
const NOTICE_ICON = { permission: "lock", off: "power_settings_new", looking: "usb", paused: "pause_circle", battery: "battery_alert" };

/**
 * The window's header (the OS title bar is gone, TitleBar.jsx): tabs, the
 * status text (opens the permission fix when one is missing), the
 * On/Off switch and the window controls. Dragging anywhere but a control
 * moves the window; a double-click maximises.
 */
export function Toolbar({ tab, onTab, state, onToggle, onFix }) {
  const status = deriveStatus(state);
  const fix = state.enabled && state.permissionMissing ? onFix : undefined;
  return (
    <nav className="toolbar" data-tauri-drag-region="deep">
      <div className="tabs">
        {TABS.map((t) => (
          <button key={t} className="tab" data-icon={TAB_ICON[t]} aria-pressed={t === tab} onClick={() => onTab(t)}>
            {t}
          </button>
        ))}
      </div>
      <div className="toolbar-right">
        {/* The status says it short, so it fits the default 1000px window
            whole; narrower, it ends in an ellipsis (the stylesheet's
            .status-slot). The tooltip and the notice bar say it in full. */}
        <div className="status-slot">
          <button
            className={`status${fix ? " is-link" : ""}`}
            onClick={fix}
            tabIndex={fix ? 0 : -1}
            title={status.text}
            aria-label={status.text}
            data-tone={status.tone}
          >
            <span className="status-text">{status.short}</span>
          </button>
        </div>
        <span className="toolbar-sep" />
        <strong className="switch-text">{state.enabled ? "On" : "Off"}</strong>
        <button
          className="switch"
          role="switch"
          aria-label="Turn freewheel on or off"
          aria-checked={state.enabled}
          title="Turn freewheel on or off"
          disabled={!onToggle}
          onClick={onToggle}
        >
          <span className="switch-knob" />
        </button>
      </div>
      <WindowControls />
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
