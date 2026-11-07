// What's new after an update: before the updater installs, the new version's
// release notes (release-please's notes, from latest.json) are kept in the
// webview's storage; after the restart into a newer version the settings
// window offers them once. The storage also keeps the last version the user
// saw, so a first install shows nothing and each update shows once.

export const LAST_SEEN_VERSION = "freewheel.last-seen-version";
export const UPDATE_NOTES = "freewheel.update-notes";
export const RELEASES = "https://github.com/brinkercode/freewheel/releases";

/** The GitHub release page of a version's tag (v0.1.1). */
export const releasePage = (version) => `${RELEASES}/tag/v${String(version).replace(/^v/, "")}`;

/** Opens a version's release page in the default browser (src-tauri updates.rs). */
export const openReleasePage = (version) => globalThis.window.__TAURI__.core.invoke("open_release_page", { version });

/** Compares two semver versions (pre-release tags sort before the release); <0, 0 or >0. */
export function compareVersions(a, b) {
  const parse = (v) => {
    const [core, pre = ""] = String(v).replace(/^v/, "").split("-", 2);
    return { nums: core.split(".").map((n) => Number.parseInt(n, 10) || 0), pre };
  };
  const x = parse(a);
  const y = parse(b);
  for (let i = 0; i < 3; i++) {
    const d = (x.nums[i] ?? 0) - (y.nums[i] ?? 0);
    if (d) return d;
  }
  if (x.pre === y.pre) return 0;
  if (!x.pre) return 1;
  if (!y.pre) return -1;
  return x.pre < y.pre ? -1 : 1;
}

/** Keeps an update's notes for after the restart; call right before installing it. */
export function rememberNotes(storage, update) {
  if (!update?.version) return;
  try {
    storage?.setItem(UPDATE_NOTES, JSON.stringify({ version: update.version, notes: update.body ?? "" }));
  } catch {
    // Storage full or unavailable: the notice then links to GitHub only.
  }
}

/**
 * What to show for the running `version`: { version, notes, url } once after
 * an update into it, else null. A first install (nothing seen yet) and a
 * downgrade only record the version.
 */
export function whatsNew(storage, version) {
  if (!version || !storage) return null;
  const seen = storage.getItem(LAST_SEEN_VERSION);
  if (seen && compareVersions(version, seen) > 0) {
    let kept = null;
    try {
      kept = JSON.parse(storage.getItem(UPDATE_NOTES) ?? "null");
    } catch {
      kept = null;
    }
    const notes = kept?.version === version ? kept.notes || null : null;
    return { version, notes, url: releasePage(version) };
  }
  if (seen !== version) markSeen(storage, version);
  return null;
}

/** Records that the user saw (or dismissed) what is new in `version`. */
export function markSeen(storage, version) {
  storage?.setItem(LAST_SEEN_VERSION, version);
  storage?.removeItem(UPDATE_NOTES);
}
