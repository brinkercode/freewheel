// Startup update check through tauri-plugin-updater. The plugin verifies the
// minisign signature against plugins.updater.pubkey before installing.

/** Checks once; a failed check is logged and never thrown. With autoUpdate the banner starts at downloading. */
export async function startupCheck({ check, autoUpdate = true, onState, log = console.error }) {
  let update;
  try {
    update = await check();
  } catch (e) {
    log("update check failed", e);
    return null;
  }
  if (!update) return null;
  if (autoUpdate) await download(update, onState, log);
  else onState({ status: "available", version: update.version });
  return update;
}

/** Downloads the update, reporting progress (0-100, or null when the size is unknown), then marks it ready. */
export async function download(update, onState, log = console.error) {
  let total = 0;
  let received = 0;
  const progress = () => (total ? Math.round((received / total) * 100) : null);
  onState({ status: "downloading", version: update.version, progress: 0 });
  try {
    await update.download((event) => {
      if (event.event === "Started") total = event.data.contentLength ?? 0;
      if (event.event === "Progress") received += event.data.chunkLength;
      onState({ status: "downloading", version: update.version, progress: progress() });
    });
  } catch (e) {
    log("update download failed", e);
    onState({ status: "available", version: update.version });
    return;
  }
  onState({ status: "ready", version: update.version });
}

/** Installs the downloaded update (Windows runs the passive NSIS installer) and relaunches. */
export async function restart(update, relaunch, onState) {
  onState({ status: "restarting", version: update.version });
  await update.install();
  await relaunch();
}
