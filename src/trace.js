// A short in-memory trace of the settings window's update flow, its page
// readiness and the tray's actions (window.__freewheelTrace). Nothing
// leaves the window: the e2e reads it when a step fails, so a stuck update
// says which transition did not happen.

/** At most this many entries are kept (the oldest go first). */
export const TRACE_MAX = 200;

/** Adds `what` (and `data`) to the trace, timestamped. */
export function trace(what, data = {}, target = globalThis.window) {
  if (!target) return;
  const list = (target.__freewheelTrace ??= []);
  list.push({ at: new Date().toISOString(), what, ...data });
  if (list.length > TRACE_MAX) list.splice(0, list.length - TRACE_MAX);
}
