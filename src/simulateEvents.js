// Simulate-only events from the Simulator window to the settings window
// (src-tauri simulate.rs emits them only in `freewheel --simulate`, which also
// sets window.__FREEWHEEL_SIMULATE__ on each page load). Outside --simulate the
// flag is unset, so these handlers never act.

export const SIM_RECORD_KEY_EVENT = "sim-record-key";
export const SIM_UPDATE_EVENT = "sim-update";
/** To the Simulator window: lines for its log, from what the app did. */
export const SIM_LOG_EVENT = "sim-log";

/** True only in the settings window of `freewheel --simulate`. */
export const simulating = (win = globalThis.window) => Boolean(win?.__FREEWHEEL_SIMULATE__);

/**
 * Listens for the Simulator's "Record a key" (`onRecordKey(keyName)`) and
 * "New version" (`onUpdate(state or null)`); returns the stop function.
 * Each event is ignored unless simulating() when it arrives.
 */
export function listenSimulate({ onRecordKey, onUpdate }, win = globalThis.window) {
  const listen = win?.__TAURI__?.event?.listen;
  if (!listen) return () => {};
  const gated = (fn) => (event) => simulating(win) && fn(event.payload);
  const stops = [listen(SIM_RECORD_KEY_EVENT, gated(onRecordKey)), listen(SIM_UPDATE_EVENT, gated(onUpdate))];
  return () => stops.forEach((p) => Promise.resolve(p).then((stop) => stop(), () => {}));
}
