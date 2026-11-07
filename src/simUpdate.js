// `freewheel --simulate` only: the simulated restart after a simulated
// update. The update itself (check, download with progress, signature
// check) runs in the Rust updater's simulated source (src-tauri
// src/selfupdate.rs), published by the Simulator's "New version"; it
// installs nothing, so the settings window draws the restart here and then
// shows the new version and What's new for it. Nothing here is used outside
// --simulate (src/App.jsx picks it only when simulating()).

/** How long the simulated restart takes, like the prototype's. */
export const SIM_RESTART_MS = 1600;

/** The simulated relaunch: after SIM_RESTART_MS, `onRestarted()` (the window then runs the new version). */
export function simRelaunch(onRestarted, { timers = globalThis, ms = SIM_RESTART_MS } = {}) {
  return new Promise((resolve) =>
    timers.setTimeout(() => {
      onRestarted();
      resolve();
    }, ms),
  );
}
