import { useEffect, useState } from "react";
import Icon from "./Icon.jsx";

// The app draws its own title bar, Logitech G Hub style: no OS caption, no
// title text, no icon. Windows and Linux open undecorated (tauri.conf.json,
// simulate.rs) and get these minimise / maximise / close buttons; macOS keeps
// its native traffic lights over the header (tauri.macos.conf.json). The
// window title is still set, for the taskbar, but never drawn.

/** The current Tauri window, or null outside the app (tests, browser harness). */
const currentWindow = () => globalThis.window?.__TAURI__?.window?.getCurrentWindow?.() ?? null;

/** True on macOS, where the OS draws the window controls. */
export const isMac = (nav = globalThis.navigator) => /Mac/i.test(nav?.platform || nav?.userAgent || "");

/** The window's maximised state, kept current as it resizes. */
function useMaximized(win) {
  const [max, setMax] = useState(false);
  useEffect(() => {
    if (!win) return undefined;
    const sync = () => win.isMaximized().then(setMax, () => {});
    sync();
    const stop = win.onResized(sync);
    return () => Promise.resolve(stop).then((f) => f(), () => {});
  }, [win]);
  return [max, setMax];
}

/**
 * Minimise, maximise / restore and close, top-right, in the app's colours.
 * Close asks the window to close, which hides it to the tray (main.rs).
 */
export function WindowControls({ win: given, mac = isMac() }) {
  const [win] = useState(() => given ?? currentWindow());
  const [max, setMax] = useMaximized(win);
  if (mac) return null;
  const act = (what) => () => {
    if (!win) return;
    const done = win[what]();
    if (what === "toggleMaximize") Promise.resolve(done).then(() => win.isMaximized().then(setMax, () => {}), () => {});
  };
  return (
    <div className="window-controls" data-tauri-drag-region="false">
      <button className="window-control" aria-label="Minimise" title="Minimise" onClick={act("minimize")}>
        <Icon name="remove" className="icon" />
      </button>
      <button
        className="window-control"
        aria-label={max ? "Restore" : "Maximise"}
        title={max ? "Restore" : "Maximise"}
        data-state={max ? "maximized" : "normal"}
        onClick={act("toggleMaximize")}
      >
        <Icon name={max ? "filter_none" : "crop_square"} className="icon" />
      </button>
      <button className="window-control window-close" aria-label="Close" title="Close (freewheel keeps running)" onClick={act("close")}>
        <Icon name="close" className="icon" />
      </button>
    </div>
  );
}

/** The bare header for screens without the toolbar (first run, permission fix): drag area plus controls. */
export function TitleBar() {
  return (
    <header className="title-bar" data-tauri-drag-region="deep">
      <WindowControls />
    </header>
  );
}
