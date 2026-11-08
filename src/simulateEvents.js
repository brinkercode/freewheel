// Simulate-only events from the Simulator window to the settings window
// (src-tauri simulate.rs emits them only in `freewheel --simulate`, which also
// sets window.__FREEWHEEL_SIMULATE__ on each page load). Outside --simulate the
// flag is unset, so these handlers never act.

export const SIM_RECORD_KEY_EVENT = "sim-record-key";
export const SIM_UPDATE_EVENT = "sim-update";
/** To the Simulator window: lines for its log, from what the app did. */
export const SIM_LOG_EVENT = "sim-log";
/** To the Simulator window: its status after a change it did not ask for (the app in front). */
export const SIM_STATUS_EVENT = "sim-status";
/** To the Simulator window: the tray menu changed (src-tauri tray.rs TrayMenuState). */
export const SIM_TRAY_EVENT = "sim-tray";

/** True only in the settings window of `freewheel --simulate`. */
export const simulating = (win = globalThis.window) => Boolean(win?.__FREEWHEEL_SIMULATE__);

/**
 * Listens for the Simulator's "Record a key" (`onRecordKey(keyName)`) and
 * "New version" (`onUpdate(state or null)`); returns the stop function,
 * whose `ready` promise settles once both listeners are attached. Each
 * event is ignored unless simulating() when it arrives.
 */
export function listenSimulate({ onRecordKey, onUpdate }, win = globalThis.window) {
  const listen = win?.__TAURI__?.event?.listen;
  if (!listen) return Object.assign(() => {}, { ready: Promise.resolve() });
  const gated = (fn) => (event) => simulating(win) && fn(event.payload);
  const stops = [listen(SIM_RECORD_KEY_EVENT, gated(onRecordKey)), listen(SIM_UPDATE_EVENT, gated(onUpdate))];
  const stop = () => stops.forEach((p) => Promise.resolve(p).then((s) => s(), () => {}));
  return Object.assign(stop, { ready: Promise.allSettled(stops) });
}
