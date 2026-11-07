#!/usr/bin/env node
// freewheel's user-facing release notes, one script for the release and for
// every preview of it, so they can't diverge:
//
//   node scripts/release-notes.mjs filter [--in notes.md]
//     release-please's notes (the GitHub release body) -> the notes the app's
//     What's new shows (release.yml writes them into latest.json):
//     developer-only bullets dropped (scripts/release-notes.json
//     hidden_scopes; for GitHub's generated notes also the hidden commit
//     types), commit hashes stripped, internal ticket scopes unlabelled,
//     PR links kept, hidden and empty sections dropped.
//   node scripts/release-notes.mjs preview [--dir .] [--version X.Y.Z] [--raw] [--out file]
//     the notes the NEXT release will have: the commits since the last v*
//     tag (or the root) grouped as release-please groups them
//     (release-please-config.json changelog-sections), then `filter`.
//     --raw prints the release-please-style notes before the filter.
//
// CHANGELOG.md and the GitHub release keep release-please's full notes.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const read = (rel) => JSON.parse(readFileSync(new URL(rel, import.meta.url), "utf8"));

/** What What's new leaves out (scripts/release-notes.json). */
export const CONFIG = read("./release-notes.json");
/** release-please's config: its changelog sections decide the grouping. */
export const RELEASE_PLEASE = read("../release-please-config.json");
export const SECTIONS = RELEASE_PLEASE["changelog-sections"];
export const REPO_URL = "https://github.com/brinkercode/freewheel";

/** What a version without any user-visible change says. */
export const NOTHING_VISIBLE = "This version has no changes you’ll notice. Everything that changed is listed on GitHub.";

const HIDDEN_TYPES = () => new Set(SECTIONS.filter((s) => s.hidden).map((s) => s.type));
/** Sections release-please hides now (older notes, or GitHub's, may still have them). */
const HIDDEN_SECTIONS = () => new Set(SECTIONS.filter((s) => s.hidden).map((s) => s.section));

/** A conventional commit header: { type, scope, breaking, subject } or null. */
export function parseHeader(header) {
  const m = String(header).match(/^(\w+)(?:\(([^)]*)\))?(!)?:\s+(.+)$/);
  if (!m) return null;
  return { type: m[1].toLowerCase(), scope: m[2] || null, breaking: Boolean(m[3]), subject: m[4].trim() };
}

/** A commit as release-please reads it; null when not conventional. */
export function parseCommit({ sha, subject, body = "" }) {
  const head = parseHeader(subject);
  if (!head) return null;
  const footer = String(body).match(/(?:^|\n)BREAKING[ -]CHANGE:\s*([\s\S]+?)(?:\n\n|\s*$)/);
  return {
    ...head,
    sha,
    breaking: head.breaking || Boolean(footer),
    breakingNote: footer ? footer[1].trim().replace(/\s*\n\s*/g, " ") : head.subject,
  };
}

/** "subject (#145)" -> "subject ([#145](…/issues/145))", as release-please links them. */
const linkIssues = (text, repo) => text.replace(/\(#(\d+)\)/g, (_, n) => `([#${n}](${repo}/issues/${n}))`);

const bullet = (c, text, repo) => {
  const scope = c.scope ? `**${c.scope}:** ` : "";
  return `* ${scope}${linkIssues(text, repo)} ([${c.sha.slice(0, 7)}](${repo}/commit/${c.sha}))`;
};

/** Sorted by scope then subject, unscoped last within a section (release-please's order). */
const byScope = (a, b) => {
  const ka = a.scope == null ? "\uffff" : `${a.scope}:`;
  const kb = b.scope == null ? "\uffff" : `${b.scope}:`;
  if (ka !== kb) return ka < kb ? -1 : 1;
  return a.subject < b.subject ? -1 : a.subject > b.subject ? 1 : 0;
};

/**
 * Release-please-style notes for `commits` (newest first, as git log lists
 * them): the version heading, then each visible section of
 * changelog-sections with a bullet per commit.
 */
export function releasePleaseNotes(commits, { version, previous = null, date, repo = REPO_URL, sections = SECTIONS }) {
  const parsed = commits.map(parseCommit).filter(Boolean);
  const out = [`## [${version}](${repo}/compare/${previous ?? `v${version}`}...v${version}) (${date})`, "", ""];
  const breaking = parsed.filter((c) => c.breaking);
  const groups = [];
  if (breaking.length) groups.push(["⚠ BREAKING CHANGES", breaking.sort(byScope).map((c) => bullet(c, c.breakingNote, repo))]);
  const seen = new Set();
  for (const s of sections) {
    if (s.hidden || seen.has(s.section)) continue;
    seen.add(s.section);
    const types = new Set(sections.filter((x) => x.section === s.section && !x.hidden).map((x) => x.type));
    const items = parsed.filter((c) => types.has(c.type)).sort(byScope);
    if (items.length) groups.push([s.section, items.map((c) => bullet(c, c.subject, repo))]);
  }
  for (const [title, items] of groups) out.push(`### ${title}`, "", ...items, "");
  return out.join("\n").replace(/\n+$/, "\n");
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

/** Release-please's notes -> What's new's notes (see the header). */
export function userNotes(markdown, config = CONFIG) {
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
  if (!any) out.push(NOTHING_VISIBLE, "");
  return out.join("\n").replace(/\n{3,}/g, "\n\n").replace(/\n+$/, "\n");
}

const git = (dir, args) => execFileSync("git", args, { cwd: dir, encoding: "utf8", maxBuffer: 64 * 1024 * 1024, stdio: ["ignore", "pipe", "pipe"] });

/** The last v* tag reachable from HEAD, or null. */
export function lastTag(dir) {
  try {
    return git(dir, ["describe", "--tags", "--abbrev=0", "--match", "v[0-9]*", "HEAD"]).trim() || null;
  } catch {
    return null;
  }
}

/** The commits after `tag` (all of them without one), newest first. */
export function commitsSince(dir, tag) {
  const range = tag ? [`${tag}..HEAD`] : ["HEAD"];
  return git(dir, ["log", "--format=%H%x1f%s%x1f%b%x1e", ...range])
    .split("\x1e")
    .map((r) => r.replace(/^\n/, ""))
    .filter(Boolean)
    .map((r) => {
      const [sha, subject, body] = r.split("\x1f");
      return { sha, subject, body: body ?? "" };
    });
}

/** The version release-please proposes after `tag` for `commits` (its pre-1.0 rules, release-as first). */
export function nextVersion(tag, commits, config = RELEASE_PLEASE) {
  const releaseAs = config.packages?.["."]?.["release-as"];
  const last = tag ? tag.replace(/^v/, "") : null;
  const num = (v) => v.split("-")[0].split(".").map((n) => Number.parseInt(n, 10) || 0);
  const newer = (a, b) => {
    const [x, y] = [num(a), num(b)];
    for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i];
    return false;
  };
  if (releaseAs && (!last || newer(releaseAs, last))) return releaseAs;
  if (!last) return "1.0.0";
  const [major, minor, patch] = num(last);
  const parsed = commits.map(parseCommit).filter(Boolean);
  const breaking = parsed.some((c) => c.breaking);
  const feat = parsed.some((c) => c.type === "feat");
  if (major === 0) {
    if (breaking && config["bump-minor-pre-major"]) return `0.${minor + 1}.0`;
    if (breaking) return `1.0.0`;
    if (feat && !config["bump-patch-for-minor-pre-major"]) return `0.${minor + 1}.0`;
    return `0.${minor}.${patch + 1}`;
  }
  if (breaking) return `${major + 1}.0.0`;
  if (feat) return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

/** The next release's notes from the repo in `dir`: { version, raw, notes }. */
export function preview({ dir = ".", version, date = new Date().toISOString().slice(0, 10) } = {}) {
  const tag = lastTag(dir);
  const commits = commitsSince(dir, tag);
  const v = version ?? nextVersion(tag, commits);
  const raw = releasePleaseNotes(commits, { version: v, previous: tag, date });
  return { version: v, tag, commits: commits.length, raw, notes: userNotes(raw) };
}

export function parseArgs(argv) {
  const [command, ...rest] = argv;
  const opts = { command };
  for (let i = 0; i < rest.length; i++) {
    if (!rest[i].startsWith("--")) throw new Error(`unexpected ${rest[i]}`);
    const name = rest[i].slice(2);
    if (name === "raw") opts.raw = true;
    else opts[name] = rest[++i];
  }
  return opts;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const opts = parseArgs(process.argv.slice(2));
  let text;
  if (opts.command === "filter") {
    text = userNotes(readFileSync(opts.in ?? 0, "utf8"));
  } else if (opts.command === "preview") {
    const p = preview({ dir: opts.dir ?? ".", version: opts.version });
    console.error(`release-notes: ${p.commits} commits since ${p.tag ?? "the root"}, next version ${p.version}`);
    text = opts.raw ? p.raw : p.notes;
  } else {
    console.error("usage: release-notes.mjs filter [--in notes.md] | preview [--dir .] [--version X.Y.Z] [--raw] [--out file]");
    process.exit(1);
  }
  if (opts.out) writeFileSync(opts.out, text);
  else process.stdout.write(text);
}
