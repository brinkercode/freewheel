#!/usr/bin/env node
// freewheel's user-facing release notes, one script for the release and for
// every preview of it, so they can't diverge. Two cuts of release-please's
// notes:
//
//   node scripts/release-notes.mjs release [--in notes.md] [--version X.Y.Z]
//     the GitHub release body (release.yml replaces release-please's with it):
//     developer-only bullets dropped (scripts/release-notes.json
//     hidden_scopes; for GitHub's generated notes also the hidden commit
//     types), commit hashes stripped, internal ticket scopes unlabelled,
//     scope labels and PR links kept, hidden and empty sections dropped.
//   node scripts/release-notes.mjs filter [--in notes.md] [--version X.Y.Z]
//     the notes the app's What's new shows (release.yml writes them into
//     latest.json): the release cut without the `**scope:**` labels and the
//     PR links, each line starting with a capital, the version heading plain.
//   node scripts/release-notes.mjs preview [--dir .] [--version X.Y.Z] [--raw | --release] [--out file]
//     the app's notes for the NEXT release: the commits since the last v*
//     tag (or the root) grouped as release-please groups them
//     (release-please-config.json changelog-sections), then `filter`.
//     --raw prints the release-please-style notes, --release the release cut.
//   node scripts/release-notes.mjs curated --version X.Y.Z [--out file]
//     the hand-written notes in release-notes/vX.Y.Z.md; exits 1 when there are none.
//
// Hand-written notes win: when release-notes/v<version>.md exists, `release
// --version`, `filter --version` and `preview` give it as is, in place of
// the generated notes.
//
// CHANGELOG.md keeps release-please's full notes.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  appBullet,
  appNotes,
  CONFIG,
  NOTHING_VISIBLE,
  NOTHING_VISIBLE_RELEASE,
  parseHeader,
  RELEASE_PLEASE,
  REPO_URL,
  SECTIONS,
  userBullet,
  userNotes,
} from "../src/releaseNotes.js";

// The cleanup itself lives in src/releaseNotes.js (the app's What's new runs it too).
export { appBullet, appNotes, CONFIG, NOTHING_VISIBLE, NOTHING_VISIBLE_RELEASE, parseHeader, RELEASE_PLEASE, REPO_URL, SECTIONS, userBullet, userNotes };

/** Where hand-written notes live, one file per version: release-notes/v0.1.0.md. */
export const CURATED_DIR = "release-notes";
const ROOT = fileURLToPath(new URL("..", import.meta.url));

/** The hand-written notes for `version` ("0.1.0" or "v0.1.0") in `dir`, or null when there are none. */
export function curatedNotes(version, dir = ROOT) {
  if (!version) return null;
  const file = path.join(dir, CURATED_DIR, `v${String(version).replace(/^v/, "")}.md`);
  if (!existsSync(file)) return null;
  const text = readFileSync(file, "utf8").replace(/\r\n?/g, "\n").trim();
  return text ? `${text}\n` : null;
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

/** The next release's notes from the repo in `dir`: { version, raw, release, notes } (notes: What's new's). */
export function preview({ dir = ".", version, date = new Date().toISOString().slice(0, 10) } = {}) {
  const tag = lastTag(dir);
  const commits = commitsSince(dir, tag);
  const v = version ?? nextVersion(tag, commits);
  const raw = releasePleaseNotes(commits, { version: v, previous: tag, date });
  const curated = curatedNotes(v, dir);
  return {
    version: v,
    tag,
    commits: commits.length,
    raw,
    release: curated ?? userNotes(raw),
    notes: curated ?? appNotes(raw),
    curated: Boolean(curated),
  };
}

export function parseArgs(argv) {
  const [command, ...rest] = argv;
  const opts = { command };
  for (let i = 0; i < rest.length; i++) {
    if (!rest[i].startsWith("--")) throw new Error(`unexpected ${rest[i]}`);
    const name = rest[i].slice(2);
    if (name === "raw" || name === "release") opts[name] = true;
    else opts[name] = rest[++i];
  }
  return opts;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const opts = parseArgs(process.argv.slice(2));
  let text;
  if (opts.command === "filter" || opts.command === "release") {
    const curated = curatedNotes(opts.version);
    if (curated) console.error(`release-notes: using ${CURATED_DIR}/v${opts.version.replace(/^v/, "")}.md`);
    const cut = opts.command === "release" ? userNotes : appNotes;
    text = curated ?? cut(readFileSync(opts.in ?? 0, "utf8"));
  } else if (opts.command === "curated") {
    text = curatedNotes(opts.version);
    if (!text) process.exit(1);
  } else if (opts.command === "preview") {
    const p = preview({ dir: opts.dir ?? ".", version: opts.version });
    console.error(`release-notes: ${p.commits} commits since ${p.tag ?? "the root"}, next version ${p.version}${p.curated ? " (hand-written notes)" : ""}`);
    text = opts.raw ? p.raw : opts.release ? p.release : p.notes;
  } else {
    console.error("usage: release-notes.mjs release|filter [--in notes.md] [--version X.Y.Z] | curated --version X.Y.Z [--out file] | preview [--dir .] [--version X.Y.Z] [--raw | --release] [--out file]");
    process.exit(1);
  }
  if (opts.out) writeFileSync(opts.out, text);
  else process.stdout.write(text);
}
