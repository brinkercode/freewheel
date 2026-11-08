// The profile icons: every icon of Google's Material icon font (Rounded), bundled
// (src/assets/symbols/symbols.woff2, drawn by codepoint, never fetched),
// by its Material name. The name list is the one the Rust side checks
// (crates/freewheel-core/src/icons.rs); scripts/icons/build-icon-font.py
// makes both.
import list from "../crates/freewheel-core/src/icon_names.txt?raw";

/** Google's catalogue of every icon, in the Rounded style the app draws. */
export const ICON_CATALOGUE = "https://fonts.google.com/icons?icon.style=Rounded";

const CODEPOINTS = new Map(
  list
    .split("\n")
    .filter(Boolean)
    .map((line) => {
      const [name, cp] = line.trim().split(" ");
      return [name, parseInt(cp, 16)];
    }),
);

/** Every icon name, sorted. */
export const ICON_NAMES = [...CODEPOINTS.keys()];

/** `text` without its spaces and `:` marks, in lower case: ":Edit:" -> "edit". */
export const bareIcon = (text) =>
  String(text ?? "")
    .trim()
    .replace(/^:/, "")
    .replace(/:$/, "")
    .trim()
    .toLowerCase();

/** The bare icon name `text` refers to (`edit`, `:edit:`), or null if the font lacks it. */
export const iconName = (text) => {
  const bare = bareIcon(text);
  return CODEPOINTS.has(bare) ? bare : null;
};

/** How the mappings file writes icon `name`. */
export const writtenIcon = (name) => `:${name}:`;

/** The character that draws icon `name` in the bundled font ("" if none). */
export const iconChar = (name) => {
  const cp = CODEPOINTS.get(name);
  return cp == null ? "" : String.fromCodePoint(cp);
};

/** "sports_esports" -> "sports esports". */
export const iconLabel = (name) => name.replaceAll("_", " ");

/**
 * The icons matching `query` (words, or a :name:), best first: the name
 * itself, names starting with it, names whose words start with each typed
 * word, then names containing every word. Shorter names first in each.
 */
export function searchIcons(query, limit = 48) {
  const bare = bareIcon(query);
  const words = bare.split(/[\s_-]+/).filter(Boolean);
  if (!words.length) return [];
  const joined = words.join("_");
  const scored = [];
  for (const name of ICON_NAMES) {
    let score = -1;
    if (name === joined) score = 0;
    else if (name.startsWith(joined)) score = 1;
    else {
      const parts = name.split("_");
      if (words.every((w) => parts.some((p) => p.startsWith(w)))) score = 2;
      else if (words.every((w) => name.includes(w))) score = 3;
    }
    if (score >= 0) scored.push([score, name]);
  }
  scored.sort((a, b) => a[0] - b[0] || a[1].length - b[1].length || (a[1] < b[1] ? -1 : 1));
  return scored.slice(0, limit).map(([, name]) => name);
}
