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

/**
 * A notes line as plain copy: no markdown links or emphasis, no leading
 * "**scope:**" and no trailing "(#129) (6a047c6)" references.
 */
const plain = (text) =>
  text
    .replace(/^\*\*[^*]+:\*\*\s*/, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/(\s*\((#\d+|[0-9a-f]{7,40})\))+\s*$/, "")
    .trim();

/**
 * Release-please's markdown notes as sections of plain items:
 * [{ title, items: [string] }]. The version heading is dropped.
 */
export function noteSections(markdown) {
  const sections = [];
  let current = null;
  for (const raw of String(markdown ?? "").split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    const heading = line.match(/^#{1,6}\s+(.*)$/);
    if (heading) {
      const title = plain(heading[1]);
      // "## [0.1.1](compare link) (2026-11-07)" is the version line, not a section.
      if (/^v?\d+\.\d+\.\d+/.test(title)) continue;
      current = { title, items: [] };
      sections.push(current);
      continue;
    }
    const item = line.match(/^[*-]\s+(.*)$/);
    const text = plain(item ? item[1] : line);
    if (!text) continue;
    if (!current) {
      current = { title: null, items: [] };
      sections.push(current);
    }
    current.items.push(text);
  }
  return sections.filter((s) => s.items.length);
}
