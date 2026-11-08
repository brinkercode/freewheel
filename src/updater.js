// The update flow through freewheel's portable updater (src/selfUpdate.js,
// src-tauri/src/selfupdate.rs; in `freewheel --simulate` its simulated
// source: the same states, the same screens). The download verifies the
// package's minisign signature against the pubkey baked into the release
// build before anything is installed.
//
// The states the banner and the General tab draw (null is idle):
//   { status: "checking" }
//   { status: "current" }                                   up to date
//   { status: "available", version }
//   { status: "downloading", version, progress }            progress 0-100, or null when the size is unknown
//   { status: "ready", version }
//   { status: "restarting", version }
//   { status: "error", during: "check" | "download" | "install", version?, message,
//     notConfigured?, notWritable?, folder?, badSignature?, cantInstall?, reason?, noManifest? }

/** How long 'Checking…' shows at least, so a fast answer still reads as a check. */
export const MIN_CHECK_MS = 900;

const sleep = (ms) => new Promise((resolve) => globalThis.setTimeout(resolve, ms));

/**
 * The error state, from the Rust updater's error codes (selfupdate.rs
 * UpdateError): `notConfigured` when this build has no updater (dev and CI
 * builds have no pubkey), `notWritable` (+ `folder`) when the app's folder
 * can't be changed, `badSignature` when the download did not verify,
 * `cantInstall` (+ `reason`, and the newer `version` a check found) when this
 * copy of freewheel can't replace itself, e.g. on Linux when it doesn't run
 * from its AppImage. `noManifest` when the latest release has no latest.json
 * (204/404): the check failed, it is not "up to date".
 */
export function errorState(during, error, version) {
  const message = String(error?.message ?? error ?? "");
  const state = { status: "error", during, message };
  if (version) state.version = version;
  const code = message.match(/^([a-z-]+):\s?/);
  if (during === "check" && code?.[1] === "not-configured") state.notConfigured = true;
  if (code?.[1] === "not-writable") {
    state.notWritable = true;
    state.folder = message.slice(code[0].length);
  }
  if (code?.[1] === "bad-signature") state.badSignature = true;
  if (during === "check" && code?.[1] === "no-manifest") state.noManifest = true;
  if (code?.[1] === "cant-install") {
    state.cantInstall = true;
    state.reason = message.slice(code[0].length);
  }
  if (code?.[1] === "not-installable") {
    const rest = message.slice(code[0].length);
    const at = rest.indexOf(": ");
    state.cantInstall = true;
    if (at > 0) {
      state.version = rest.slice(0, at);
      state.reason = rest.slice(at + 2);
    } else state.reason = rest;
  }
  return state;
}

/** Checks once by itself (at startup); a failed check is logged and shows nothing. With autoUpdate the banner starts at downloading. */
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

/**
 * 'Check for updates': checking (for at least `minMs`), then up to date, an
 * error to retry, or as startupCheck. Resolves to the update, if any.
 */
export async function checkNow({ check, autoUpdate = true, onState, log = console.error, minMs = MIN_CHECK_MS, wait = sleep }) {
  onState({ status: "checking" });
  const shown = wait(minMs);
  let update;
  try {
    update = await check();
  } catch (e) {
    log("update check failed", e);
    await shown;
    onState(errorState("check", e));
    return null;
  }
  await shown;
  if (!update) {
    onState({ status: "current" });
    return null;
  }
  if (autoUpdate) await download(update, onState, log);
  else onState({ status: "available", version: update.version });
  return update;
}

/** Downloads the update, reporting progress, then marks it ready; a failure is an error to retry. Resolves true when ready. */
export async function download(update, onState, log = console.error) {
  let total = 0;
  let received = 0;
  const progress = () => (total ? Math.min(100, Math.round((received / total) * 100)) : null);
  onState({ status: "downloading", version: update.version, progress: 0 });
  try {
    await update.download((event) => {
      if (event.event === "Started") total = event.data?.contentLength ?? 0;
      if (event.event === "Progress") received += event.data?.chunkLength ?? 0;
      onState({ status: "downloading", version: update.version, progress: progress() });
    });
  } catch (e) {
    log("update download failed", e);
    onState(errorState("download", e, update.version));
    return false;
  }
  onState({ status: "ready", version: update.version });
  return true;
}

/**
 * Installs the downloaded update (the new version is swapped in place of
 * this one, started, and this app exits) and relaunches. `beforeInstall`
 * runs first: the app exits right after install(), so anything for after
 * the restart is kept before it. A failed install (the old version is put
 * back) is an error to retry.
 */
export async function restart(update, relaunch, onState, beforeInstall = () => {}, log = console.error) {
  onState({ status: "restarting", version: update.version });
  try {
    beforeInstall(update);
    await update.install();
    await relaunch();
  } catch (e) {
    log("update install failed", e);
    onState(errorState("install", e, update.version));
  }
}
