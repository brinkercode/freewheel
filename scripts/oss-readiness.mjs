#!/usr/bin/env node
// Open-source readiness: fails when anything private or unfit for a public
// repository is in the tree, in new commits or in the PR text, and when a
// community file, the licence metadata or a workflow's hardening is missing.
//
//   node scripts/oss-readiness.mjs tree                 # tracked files: contents and paths
//   node scripts/oss-readiness.mjs range <base> <head>  # commits in base..head: messages, identities, added lines
//   node scripts/oss-readiness.mjs text <ENV_VAR>...    # e.g. the PR title and body (the squash message);
//                                                       # PR_TITLE also gets the pr-title rule (prTitleProblems)
//   node scripts/oss-readiness.mjs history              # every blob, path and message of every ref
//   node scripts/oss-readiness.mjs community            # community files, LICENSE, workflow pins
//   node scripts/oss-readiness.mjs npm-licenses         # package-lock.json licences against the allow list
//
// The rules are in scripts/oss-denylist.txt. Exit code 1 on any failure.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const DENYLIST = "scripts/oss-denylist.txt";

/**
 * Parses the denylist text into rules: { category, scope, kind: "re" | "sha256", re?, hash?, source }.
 * scope "messages" (a `<category>@messages` rule) applies only to commit messages and PR text.
 */
export function parseRules(text) {
  const rules = [];
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const m = line.match(/^(\w+)(@messages)?\s+(re|sha256):(.+)$/);
    if (!m) throw new Error(`${DENYLIST}: bad rule: ${line}`);
    const [, category, messagesOnly, kind, body] = m;
    const scope = messagesOnly ? "messages" : "all";
    if (kind === "re") rules.push({ category, scope, kind, re: new RegExp(body, "i"), source: line });
    else rules.push({ category, scope, kind, hash: body.toLowerCase(), source: line });
  }
  return rules;
}

const sha = (s) => createHash("sha256").update(s.toLowerCase()).digest("hex");
const WORD = /[A-Za-z0-9_-]+/g;
const EMAIL = /[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/g;

/** The rules a line breaks (empty when clean). Hashed and identity rules never print what they matched. */
export function lineHits(line, rules) {
  const hits = [];
  let hashes;
  for (const r of rules) {
    if (r.kind === "re") {
      const m = line.match(r.re);
      if (m) hits.push({ rule: r, match: r.category === "identity" ? "<redacted>" : m[0] });
    } else {
      hashes ??= new Set([...(line.match(WORD) ?? []), ...(line.match(EMAIL) ?? [])].map(sha));
      if (hashes.has(r.hash)) hits.push({ rule: r, match: "<hashed word>" });
    }
  }
  return hits;
}

/** Every hit in `text`, as { line, rule, match }. */
export function textHits(text, rules) {
  const out = [];
  text.split(/\r?\n/).forEach((line, i) => {
    for (const h of lineHits(line, rules)) out.push({ line: i + 1, ...h });
  });
  return out;
}

const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8", maxBuffer: 1 << 30 });
const gitBuf = (...args) => execFileSync("git", args, { cwd: root, maxBuffer: 1 << 30 });
const isBinary = (buf) => buf.subarray(0, 8000).includes(0);
const rules = () => parseRules(readFileSync(path.join(root, DENYLIST), "utf8"));
/** The rules for file contents and paths (not the messages-only ones). */
export const fileRules = (rs) => rs.filter((r) => r.scope !== "messages");

const failures = [];
const warnings = [];
const strictIdentity = process.env.OSS_STRICT_IDENTITY === "1";
const report = (where, hit, { identityWarns = false } = {}) => {
  const msg = `${where}: [${hit.rule.category}] ${hit.match}  (rule: ${hit.rule.source})`;
  if (identityWarns && hit.rule.category === "identity" && !strictIdentity) warnings.push(msg);
  else failures.push(msg);
};

function scanTree() {
  const rs = fileRules(rules());
  const files = git("ls-files", "-z").split("\0").filter(Boolean);
  for (const f of files) {
    for (const h of lineHits(f, rs)) report(`path ${f}`, h);
    if (f === DENYLIST) continue;
    const full = path.join(root, f);
    if (!existsSync(full)) continue;
    const buf = readFileSync(full);
    if (isBinary(buf)) continue;
    for (const h of textHits(buf.toString("utf8"), rs)) report(`${f}:${h.line}`, h);
  }
  if (files.some((f) => /^design\//.test(f))) failures.push("design/: the private design handoff is tracked");
  console.log(`tree: ${files.length} tracked files scanned`);
}

function scanRange(base, head) {
  const all = rules();
  const rs = fileRules(all);
  const commits = git("rev-list", `${base}..${head}`).split("\n").filter(Boolean);
  for (const c of commits) {
    const meta = git("show", "-s", "--format=%an <%ae>%n%cn <%ce>", c);
    // identities: file rules only (a name is not a message)
    for (const h of textHits(meta, rs)) report(`commit ${c.slice(0, 9)} identity`, h, { identityWarns: true });
    const body = git("show", "-s", "--format=%B", c);
    for (const h of textHits(body, all)) {
      const trailer = /^(Signed-off-by|Co-authored-by):/i.test(body.split(/\r?\n/)[h.line - 1]);
      report(`commit ${c.slice(0, 9)} message:${h.line}`, h, { identityWarns: trailer });
    }
    const diff = git("show", "--format=", "--no-color", "--unified=0", "--no-ext-diff", c);
    let file = "?";
    for (const line of diff.split("\n")) {
      if (line.startsWith("+++ ")) file = line.slice(6);
      else if (line.startsWith("+") && file !== DENYLIST) for (const h of lineHits(line.slice(1), rs)) report(`commit ${c.slice(0, 9)} ${file}`, h);
    }
  }
  console.log(`range ${base}..${head}: ${commits.length} commits scanned`);
}

/** Conventional commit types the history uses (release-please-config.json changelog-sections). */
export const PR_TYPES = ["feat", "fix", "perf", "revert", "docs", "style", "chore", "refactor", "test", "build", "ci"];
/** Words that mark a developer-facing change; user-visible titles (they are the release notes) avoid them. */
// "test" is not here: "Device test" is a screen users see.
export const PR_JARGON = ["CI", "e2e", "refactor", "lint", "clippy", "CodeQL", "workflow", "workflows", "script", "scripts", "bump", "bumps"];
const JARGON_RE = new RegExp(`(?<![\\w-])(${PR_JARGON.join("|")})(?![\\w-])`, "i");
const HIDDEN_SCOPES = () => JSON.parse(readFileSync(path.join(root, "scripts/release-notes.json"), "utf8")).hidden_scopes;

/**
 * The pr-title rule (empty when the title is fine): a conventional commit; a
 * feat/fix/perf title without a developer-only scope (scripts/release-notes.json
 * hidden_scopes) is a release note, so its description has at least 4 words,
 * has no closing period and no developer jargon (PR_JARGON). It may start with
 * a capital ("Speed Editor…", "AppImage…"): the notes capitalise it anyway.
 */
export function prTitleProblems(title, { hiddenScopes = HIDDEN_SCOPES() } = {}) {
  const t = String(title ?? "").trim();
  if (/^Revert "[^"]+"$/.test(t)) return [];
  const m = t.match(/^([a-z]+)(?:\(([^()\s]+)\))?(!)?: (\S.*)$/);
  if (!m || !PR_TYPES.includes(m[1]))
    return [`pr-title: "${t}" is not a conventional commit (type(scope): description, type one of ${PR_TYPES.join(", ")})`];
  const [, type, scope, , description] = m;
  if (!["feat", "fix", "perf"].includes(type) || (scope && hiddenScopes.includes(scope))) return [];
  const problems = [];
  const note = `(${type} titles are the release notes: write them for users, e.g. "fix: the Speed Editor reconnects after sleep"; developer-only changes take a hidden scope: ${hiddenScopes.join(", ")})`;
  if (description.split(/\s+/).filter(Boolean).length < 4) problems.push(`pr-title: "${description}" is under 4 words ${note}`);
  if (/\.$/.test(description)) problems.push(`pr-title: "${description}" ends with a period ${note}`);
  const jargon = description.match(JARGON_RE);
  if (jargon) problems.push(`pr-title: "${jargon[1]}" is developer jargon ${note}`);
  return problems;
}

function scanText(vars) {
  const rs = rules();
  for (const v of vars) for (const h of textHits(process.env[v] ?? "", rs)) report(`${v}:${h.line}`, h);
  if (vars.includes("PR_TITLE")) failures.push(...prTitleProblems(process.env.PR_TITLE));
  console.log(`text: ${vars.join(", ")} scanned`);
}

function scanHistory() {
  const all = rules();
  const rs = fileRules(all);
  const objects = git("rev-list", "--all", "--objects").split("\n").filter(Boolean);
  const paths = new Map();
  for (const line of objects) {
    const i = line.indexOf(" ");
    if (i < 0) continue;
    const [oid, p] = [line.slice(0, i), line.slice(i + 1)];
    if (!paths.has(oid)) paths.set(oid, new Set());
    paths.get(oid).add(p);
  }
  const seenPath = new Set();
  for (const [oid, ps] of paths) {
    for (const p of ps)
      if (!seenPath.has(p)) {
        seenPath.add(p);
        for (const h of lineHits(p, rs)) report(`path ${p}`, h);
      }
    if (git("cat-file", "-t", oid).trim() !== "blob") continue;
    if ([...ps].every((p) => p === DENYLIST)) continue;
    const buf = gitBuf("cat-file", "blob", oid);
    if (isBinary(buf)) continue;
    for (const h of textHits(buf.toString("utf8"), rs)) report(`blob ${oid.slice(0, 9)} (${[...ps][0]}):${h.line}`, h);
  }
  const log = git("log", "--all", "--format=%x1e%H%n%an <%ae>%n%cn <%ce>%n%B");
  for (const entry of log.split("\x1e").slice(1)) {
    const c = entry.slice(0, 9);
    for (const h of textHits(entry, all)) report(`commit ${c}:${h.line}`, h, { identityWarns: true });
  }
  console.log(`history: ${paths.size} objects, ${seenPath.size} paths scanned`);
}

/** Problems with the community files, the licence metadata and the workflows (empty when ready). */
export function communityProblems(dir = root) {
  const problems = [];
  const read = (f) => readFileSync(path.join(dir, f), "utf8");
  const has = (f) => existsSync(path.join(dir, f));
  for (const f of [
    "LICENSE",
    "NOTICE",
    "README.md",
    "SECURITY.md",
    "CONTRIBUTING.md",
    "CODE_OF_CONDUCT.md",
    "SUPPORT.md",
    ".github/PULL_REQUEST_TEMPLATE.md",
    ".github/ISSUE_TEMPLATE/bug_report.yml",
    ".github/ISSUE_TEMPLATE/feature_request.yml",
    ".github/ISSUE_TEMPLATE/config.yml",
    ".github/dependabot.yml",
    ".github/workflows/codeql.yml",
    ".gitleaks.toml",
    "deny.toml",
  ])
    if (!has(f)) problems.push(`${f} is missing`);
  if (has("LICENSE")) {
    const license = read("LICENSE");
    if (!/^MIT License$/m.test(license)) problems.push("LICENSE is not the MIT License");
    if (JSON.parse(read("package.json")).license !== "MIT") problems.push("package.json license is not MIT");
    for (const toml of ["src-tauri/Cargo.toml", "crates/freewheel-core/Cargo.toml"])
      if (has(toml) && !/^license = "MIT"$/m.test(read(toml))) problems.push(`${toml}: license is not "MIT"`);
  }
  if (has("NOTICE")) {
    const notice = read("NOTICE");
    if (!/smunaut\/blackmagic-misc[\s\S]*Apache License, Version 2\.0/.test(notice)) problems.push("NOTICE does not credit smunaut/blackmagic-misc (Apache-2.0)");
    if (!/Material Symbols[\s\S]*Apache License, Version 2\.0/.test(notice)) problems.push("NOTICE does not credit Material Symbols (Apache-2.0)");
    if (!has("LICENSES/Apache-2.0.txt")) problems.push("LICENSES/Apache-2.0.txt is missing (NOTICE credits Apache-2.0 work)");
  }
  if (has(".github/dependabot.yml")) {
    const dep = read(".github/dependabot.yml");
    for (const eco of ["cargo", "npm", "github-actions"])
      if (!new RegExp(`package-ecosystem: "?${eco}"?`).test(dep)) problems.push(`dependabot.yml does not update ${eco}`);
  }
  const wf = ".github/workflows";
  for (const f of has(wf) ? readdirSync(path.join(dir, wf)).filter((n) => /\.ya?ml$/.test(n)) : []) {
    const text = read(`${wf}/${f}`);
    if (!/^permissions:/m.test(text)) problems.push(`${wf}/${f}: no top-level permissions`);
    if (/write-all|pull_request_target/.test(text.replace(/#.*$/gm, ""))) problems.push(`${wf}/${f}: write-all or pull_request_target`);
    for (const m of text.matchAll(/^\s*(?:-\s*)?uses:\s*([^\s#]+)/gm)) {
      const ref = m[1];
      if (ref.startsWith("./")) continue;
      if (!/@[0-9a-f]{40}$/.test(ref)) problems.push(`${wf}/${f}: ${ref} is not pinned to a commit sha`);
    }
  }
  return problems;
}

/**
 * The licences npm packages may use: permissive, plus MPL-2.0 (weak file-level copyleft, allowed for the
 * Rust crates in deny.toml too; the bundler's lightningcss uses it).
 */
export const NPM_ALLOWED = ["MIT", "MIT-0", "ISC", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "0BSD", "CC0-1.0", "CC-BY-4.0", "Unlicense", "BlueOak-1.0.0", "Python-2.0", "Zlib", "MPL-2.0"];

/** The npm licences in package-lock.json that are not allowed (empty when all are). */
export function npmLicenseProblems(lock, allowed = NPM_ALLOWED) {
  const problems = [];
  const ok = (expr) => expr.replace(/[()]/g, " ").split(/\s+OR\s+/).some((alt) => alt.split(/\s+AND\s+/).every((l) => allowed.includes(l.trim())));
  for (const [name, pkg] of Object.entries(lock.packages ?? {})) {
    if (!name) continue;
    const lic = typeof pkg.license === "string" ? pkg.license : pkg.license?.type;
    if (!lic) problems.push(`${name}: no licence in package-lock.json`);
    else if (!ok(lic)) problems.push(`${name}: ${lic} is not allowed`);
  }
  return problems;
}

function main(argv) {
  const [mode, ...rest] = argv;
  if (mode === "tree") scanTree();
  else if (mode === "range") scanRange(rest[0], rest[1]);
  else if (mode === "text") scanText(rest);
  else if (mode === "history") scanHistory();
  else if (mode === "community") failures.push(...communityProblems());
  else if (mode === "npm-licenses") failures.push(...npmLicenseProblems(JSON.parse(readFileSync(path.join(root, "package-lock.json"), "utf8"))));
  else {
    console.error("usage: oss-readiness.mjs tree | range <base> <head> | text <VAR>... | history | community | npm-licenses");
    process.exit(2);
  }
  for (const w of warnings) console.log(`::warning::${w}`);
  for (const f of failures) console.log(`::error::${f}`);
  console.log(failures.length ? `${mode}: ${failures.length} problem(s)` : `${mode}: ok`);
  process.exit(failures.length ? 1 : 0);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main(process.argv.slice(2));
