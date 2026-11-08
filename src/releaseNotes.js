// The release notes' cleanup, one copy for the release scripts and the app:
// scripts/release-notes.mjs cuts release-please's notes into the GitHub
// release's body (userNotes) and What's new's plainer notes (appNotes); the
// app's What's new runs the same appNotes over a GitHub release's body when
// it has no notes stored from the update (src/whatsNew.js). Pure: no Node
// or DOM APIs, so Vite bundles it into the settings window.
import RELEASE_NOTES from "../scripts/release-notes.json" with { type: "json" };
import RELEASE_PLEASE_CONFIG from "../release-please-config.json" with { type: "json" };

/** What What's new leaves out (scripts/release-notes.json). */
export const CONFIG = RELEASE_NOTES;
/** release-please's config: its changelog sections decide the grouping. */
export const RELEASE_PLEASE = RELEASE_PLEASE_CONFIG;
export const SECTIONS = RELEASE_PLEASE["changelog-sections"];
export const REPO_URL = "https://github.com/brinkercode/freewheel";

/** What a version without any user-visible change says in What's new. */
export const NOTHING_VISIBLE = "This version has no changes you’ll notice. Everything that changed is listed on GitHub.";
/** ... and on its GitHub release page. */
export const NOTHING_VISIBLE_RELEASE = "This version has no changes you’ll notice. Everything that changed is in CHANGELOG.md.";

const HIDDEN_TYPES = () => new Set(SECTIONS.filter((s) => s.hidden).map((s) => s.type));
/** Sections release-please hides now (older notes, or GitHub's, may still have them). */
const HIDDEN_SECTIONS = () => new Set(SECTIONS.filter((s) => s.hidden).map((s) => s.section));

/** A conventional commit header: { type, scope, breaking, subject } or null. */
export function parseHeader(header) {
  const m = String(header).match(/^(\w+)(?:\(([^)]*)\))?(!)?:\s+(.+)$/);
  if (!m) return null;
  return { type: m[1].toLowerCase(), scope: m[2] || null, breaking: Boolean(m[3]), subject: m[4].trim() };
}

const COMMIT_LINK = /\s*\(\[[0-9a-f]{7,40}\]\([^)\s]*\/commit\/[0-9a-f]{7,40}\)\)/g;
const BARE_HASH = /\s*\(([0-9a-f]{7,40})\)(?=\s*$)/;
/** GitHub's generated notes: "* feat: x by @someone in https://github.com/o/r/pull/12". */
const GH_GENERATED = /\s+by @[\w-]+(?:\[bot\])? in (https:\/\/github\.com\/[^\s]+\/pull\/(\d+))\s*$/;

/** One bullet's text for What's new, or null when it is developer-only. */
export function userBullet(text, config = CONFIG) {
  const hidden = new Set(config.hidden_scopes);
  const unlabelled = (config.unlabelled_scopes ?? []).map((r) => new RegExp(r));
  let t = text.replace(COMMIT_LINK, "").replace(BARE_HASH, "");
  t = t.replace(GH_GENERATED, (_, url, n) => ` ([#${n}](${url}))`);
  // GitHub's generated notes keep the commit header: "fix(scripts): …".
  const head = parseHeader(t);
  if (head) {
    if (HIDDEN_TYPES().has(head.type)) return null;
    t = head.scope ? `**${head.scope}:** ${head.subject}` : head.subject;
  }
  const label = t.match(/^\*\*([^*:]+):\*\*\s*/);
  if (label) {
    const scope = label[1].trim();
    if (hidden.has(scope)) return null;
    if (unlabelled.some((r) => r.test(scope))) t = t.slice(label[0].length);
  }
  t = t.trim();
  return t || null;
}

/** Release-please's notes -> the GitHub release's cleaned notes (see the header). */
export function userNotes(markdown, config = CONFIG, { nothing = NOTHING_VISIBLE_RELEASE } = {}) {
  const blocks = [];
  let section = null;
  let lastBullet = null;
  for (const raw of String(markdown ?? "").replace(/\r\n?/g, "\n").split("\n")) {
    const line = raw.replace(/\s+$/, "");
    if (/^#{1,6}\s/.test(line)) {
      const title = line.replace(/^#+\s+/, "").trim();
      section = { heading: line, items: [], version: /^##?\s+\[?v?\d+\.\d+\.\d+/.test(line), hidden: HIDDEN_SECTIONS().has(title) };
      blocks.push(section);
      lastBullet = null;
      continue;
    }
    if (!section) {
      section = { heading: null, items: [] };
      blocks.push(section);
    }
    const item = line.match(/^[*-]\s+(.*)$/);
    if (item) {
      const text = userBullet(item[1], config);
      lastBullet = text ? { text, more: [] } : false;
      if (lastBullet) section.items.push(lastBullet);
      continue;
    }
    if (/^\s+\S/.test(line) && lastBullet !== null) {
      // A wrapped bullet's continuation follows its bullet (dropped with it).
      if (lastBullet) lastBullet.more.push(line.trim());
      continue;
    }
    if (!line.trim()) continue;
    lastBullet = null;
    section.items.push({ para: line.trim() });
  }
  const out = [];
  let any = false;
  for (const b of blocks) {
    if (b.version) {
      out.push(b.heading, "");
      continue;
    }
    if (!b.items.length || b.hidden) continue;
    any = true;
    if (b.heading) out.push(b.heading, "");
    let inList = false;
    for (const i of b.items) {
      if (i.para) {
        if (inList) out.push("");
        out.push(i.para, "");
        inList = false;
      } else {
        out.push(`* ${[i.text, ...i.more].join(" ").replace(COMMIT_LINK, "").replace(BARE_HASH, "")}`);
        inList = true;
      }
    }
    if (inList) out.push("");
  }
  if (!any) out.push(nothing, "");
  return out.join("\n").replace(/\n{3,}/g, "\n\n").replace(/\n+$/, "\n");
}

const PR_LINK = /\s*\(\[#\d+\]\([^)\s]*\)\)|\s*\(#\d+\)|\s*\[#\d+\]\([^)\s]*\)/g;
const SCOPE_LABEL = /^\*\*[^*:]+:\*\*\s*/;
/** "## [0.1.1](…/compare/v0.1.0...v0.1.1) (2026-11-20)" -> "## 0.1.1 (2026-11-20)". */
const VERSION_LINK = /^(#{1,6}\s+)\[(v?\d+\.\d+\.\d+[^\]]*)\]\([^)]*\)/;

/** One bullet as What's new shows it: no scope label, no PR link, a capital first letter (freewheel stays lowercase). */
export function appBullet(text) {
  let t = String(text).replace(SCOPE_LABEL, "").replace(PR_LINK, "").trim();
  if (!/^freewheel\b/.test(t)) t = t.replace(/^[a-z]/, (c) => c.toUpperCase());
  return t;
}

/** Release-please's notes -> What's new's notes: the release cut, then the app's plainer form (see the header). */
export function appNotes(markdown, config = CONFIG) {
  return userNotes(markdown, config, { nothing: NOTHING_VISIBLE })
    .split("\n")
    .map((line) => {
      if (VERSION_LINK.test(line)) return line.replace(VERSION_LINK, "$1$2");
      const item = line.match(/^([*-]\s+)(.*)$/);
      return item ? `${item[1]}${appBullet(item[2])}` : line;
    })
    .join("\n");
}
