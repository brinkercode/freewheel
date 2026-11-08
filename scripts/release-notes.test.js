// The user-facing release notes (scripts/release-notes.mjs): what
// release-please writes, what What's new shows, and the preview of the next
// release from the repo's history that the Simulator and the update e2e use.
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, expect, test } from "vitest";
import {
  CONFIG,
  CURATED_DIR,
  curatedNotes,
  NOTHING_VISIBLE,
  NOTHING_VISIBLE_RELEASE,
  RELEASE_PLEASE,
  appBullet,
  appNotes,
  nextVersion,
  parseCommit,
  preview,
  releasePleaseNotes,
  userBullet,
  userNotes,
} from "./release-notes.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");

test("release-please shows New, Fixed and Faster, and hides the developer sections", () => {
  const sections = RELEASE_PLEASE["changelog-sections"];
  expect(sections.filter((s) => !s.hidden).map((s) => [s.type, s.section])).toEqual([
    ["feat", "New"],
    ["fix", "Fixed"],
    ["perf", "Faster"],
  ]);
  expect(sections.filter((s) => s.hidden).map((s) => s.type).sort()).toEqual(
    ["build", "chore", "ci", "docs", "refactor", "revert", "style", "test"].sort(),
  );
  // CHANGELOG.md stays release-please's own, unfiltered.
  expect(RELEASE_PLEASE.packages["."]["changelog-path"]).toBe("CHANGELOG.md");
});

test("the developer-only scopes are a config file", () => {
  expect(CONFIG.hidden_scopes.sort()).toEqual(["ci", "deps-dev", "e2e", "footprint", "release", "scripts", "test", "workflow"]);
});

test("release-please's notes become the release's: developer bullets, hashes and hidden sections go, PR links stay", () => {
  const input = read("./fixtures/release-notes/release-please.md");
  const expected = read("./fixtures/release-notes/user.md");
  const out = userNotes(input);
  expect(out).toBe(expected);
  expect(out).not.toMatch(/\/commit\/|\(\[[0-9a-f]{7}\]/);
  for (const scope of CONFIG.hidden_scopes) expect(out).not.toContain(`**${scope}:**`);
  expect(out).not.toContain("### Reverts");
  expect(out).toContain("([#145](https://github.com/brinkercode/freewheel/issues/145))");
  // Running it again changes nothing.
  expect(userNotes(out)).toBe(out);
});

test("internal ticket scopes lose their label, the bullet stays", () => {
  expect(userBullet("**freewheel-ab1:** Start at login ([#99](u))")).toBe("Start at login ([#99](u))");
  expect(userBullet("**keys:** a held Shift is released")).toBe("**keys:** a held Shift is released");
  expect(userBullet("**scripts:** resolve the CodeQL alerts")).toBeNull();
});

test("GitHub's generated notes (a hand-pushed tag) are filtered by commit type and scope too", () => {
  const generated = [
    "## What's Changed",
    "* feat(keys): a key can open a folder by @brinkercode in https://github.com/brinkercode/freewheel/pull/151",
    "* ci: cache cargo by @brinkercode in https://github.com/brinkercode/freewheel/pull/152",
    "* fix(scripts): CodeQL by @brinkercode in https://github.com/brinkercode/freewheel/pull/153",
    "* fix: the wheel wakes up by @dependabot[bot] in https://github.com/brinkercode/freewheel/pull/154",
    "",
    "**Full Changelog**: https://github.com/brinkercode/freewheel/compare/v0.1.0...v0.1.1",
  ].join("\n");
  expect(userNotes(generated)).toBe(
    [
      "## What's Changed",
      "",
      "* **keys:** a key can open a folder ([#151](https://github.com/brinkercode/freewheel/pull/151))",
      "* the wheel wakes up ([#154](https://github.com/brinkercode/freewheel/pull/154))",
      "",
      "**Full Changelog**: https://github.com/brinkercode/freewheel/compare/v0.1.0...v0.1.1",
      "",
    ].join("\n"),
  );
});

test("a release with nothing a user notices still says something (latest.json needs notes)", () => {
  const dev = "## [0.1.1](x) (2026-11-08)\n\n\n### Fixed\n\n* **ci:** retry ([abc1234](https://github.com/o/r/commit/abc1234))\n";
  expect(userNotes(dev)).toBe(`## [0.1.1](x) (2026-11-08)\n\n${NOTHING_VISIBLE_RELEASE}\n`);
  expect(appNotes(dev)).toBe(`## 0.1.1 (2026-11-08)\n\n${NOTHING_VISIBLE}\n`);
});

test("commits group as release-please groups them", () => {
  const commits = [
    { sha: "a".repeat(40), subject: "fix(scripts): resolve the three CodeQL alerts (#148)" },
    { sha: "b".repeat(40), subject: "feat(updates): portable self-update (#145)" },
    { sha: "c".repeat(40), subject: "perf: smaller exe (#144)" },
    { sha: "d".repeat(40), subject: "chore: tidy (#143)" },
    { sha: "e".repeat(40), subject: "fix: the wheel wakes up" },
    { sha: "f".repeat(40), subject: "feat(config)!: a new mappings format" },
    { sha: "0".repeat(40), subject: "not conventional" },
  ];
  const raw = releasePleaseNotes(commits, { version: "0.2.0", previous: "v0.1.0", date: "2026-11-20" });
  const R = "https://github.com/brinkercode/freewheel";
  expect(raw).toBe(
    [
      `## [0.2.0](${R}/compare/v0.1.0...v0.2.0) (2026-11-20)`,
      "",
      "",
      "### ⚠ BREAKING CHANGES",
      "",
      `* **config:** a new mappings format ([fffffff](${R}/commit/${"f".repeat(40)}))`,
      "",
      "### New",
      "",
      `* **config:** a new mappings format ([fffffff](${R}/commit/${"f".repeat(40)}))`,
      `* **updates:** portable self-update ([#145](${R}/issues/145)) ([bbbbbbb](${R}/commit/${"b".repeat(40)}))`,
      "",
      "### Fixed",
      "",
      `* **scripts:** resolve the three CodeQL alerts ([#148](${R}/issues/148)) ([aaaaaaa](${R}/commit/${"a".repeat(40)}))`,
      `* the wheel wakes up ([eeeeeee](${R}/commit/${"e".repeat(40)}))`,
      "",
      "### Faster",
      "",
      `* smaller exe ([#144](${R}/issues/144)) ([ccccccc](${R}/commit/${"c".repeat(40)}))`,
      "",
    ].join("\n"),
  );
  const user = userNotes(raw);
  expect(user).not.toContain("CodeQL");
  expect(user).not.toContain("tidy");
  expect(user).toContain("* **updates:** portable self-update ([#145](");
});

test("a BREAKING CHANGE footer is the breaking note", () => {
  const c = parseCommit({ sha: "1".repeat(40), subject: "feat(keys): pages", body: "More.\n\nBREAKING CHANGE: old pages\nare gone\n" });
  expect(c).toMatchObject({ type: "feat", scope: "keys", breaking: true, breakingNote: "old pages are gone" });
});

test("the next version follows release-please's pre-1.0 rules", () => {
  const cfg = { "bump-minor-pre-major": true, "bump-patch-for-minor-pre-major": false, packages: { ".": {} } };
  const c = (subject) => [{ sha: "1".repeat(40), subject }];
  expect(nextVersion("v0.1.0", c("fix: x"), cfg)).toBe("0.1.1");
  expect(nextVersion("v0.1.0", c("feat: x"), cfg)).toBe("0.2.0");
  expect(nextVersion("v0.1.3", c("feat!: x"), cfg)).toBe("0.2.0");
  expect(nextVersion("v1.2.3", c("feat: x"), cfg)).toBe("1.3.0");
  expect(nextVersion("v1.2.3", c("fix!: x"), cfg)).toBe("2.0.0");
  // release-as wins until a release reaches it.
  expect(nextVersion(null, c("fix: x"), { ...cfg, packages: { ".": { "release-as": "0.1.0" } } })).toBe("0.1.0");
  expect(nextVersion("v0.1.0", c("fix: x"), { ...cfg, packages: { ".": { "release-as": "0.1.0" } } })).toBe("0.1.1");
});

let repo;
afterEach(() => repo && rmSync(repo, { recursive: true, force: true }));

test("the preview reads the commits after the last tag (all of them before the first)", () => {
  repo = mkdtempSync(join(tmpdir(), "freewheel-notes-"));
  const git = (...args) =>
    execFileSync("git", args, {
      cwd: repo,
      encoding: "utf8",
      env: { ...process.env, GIT_AUTHOR_NAME: "t", GIT_AUTHOR_EMAIL: "t@t", GIT_COMMITTER_NAME: "t", GIT_COMMITTER_EMAIL: "t@t" },
    });
  git("init", "-q");
  const commit = (subject) => {
    writeFileSync(join(repo, "f"), subject);
    git("add", "f");
    git("commit", "-q", "--no-gpg-sign", "-m", subject);
  };
  commit("feat(keys): before the tag (#1)");
  let p = preview({ dir: repo, date: "2026-11-20" });
  expect(p.tag).toBeNull();
  expect(p.release).toContain("* **keys:** before the tag ([#1](");
  expect(p.notes).toContain("* Before the tag\n");
  git("tag", "v1.0.0");
  commit("fix(ci): only CI (#2)");
  commit("fix(wheel): smoother (#3)");
  p = preview({ dir: repo, date: "2026-11-20" });
  expect(p).toMatchObject({ tag: "v1.0.0", version: "1.0.1", commits: 2 });
  expect(p.release).toBe(
    [
      "## [1.0.1](https://github.com/brinkercode/freewheel/compare/v1.0.0...v1.0.1) (2026-11-20)",
      "",
      "### Fixed",
      "",
      "* **wheel:** smoother ([#3](https://github.com/brinkercode/freewheel/issues/3))",
      "",
    ].join("\n"),
  );
  expect(p.notes).toBe(["## 1.0.1 (2026-11-20)", "", "### Fixed", "", "* Smoother", ""].join("\n"));
  // One filter for the release and the preview: the preview is the release's notes filtered.
  expect(p.release).toBe(userNotes(p.raw));
  expect(p.notes).toBe(appNotes(p.raw));
});

test("release.yml puts the release cut on the GitHub release and the app's cut in latest.json (hand-written notes win for both)", () => {
  const release = read("../.github/workflows/release.yml");
  expect(release).toContain('gh release view "$GITHUB_REF_NAME" --repo "$GITHUB_REPOSITORY" --json body --jq .body > "$RUNNER_TEMP/release-please.md"');
  expect(release).toContain('node scripts/release-notes.mjs release --version "${GITHUB_REF_NAME#v}" --in "$RUNNER_TEMP/release-please.md" --out "$RUNNER_TEMP/release-notes.md"');
  expect(release).toContain('node scripts/release-notes.mjs filter --version "${GITHUB_REF_NAME#v}" --in "$RUNNER_TEMP/release-please.md" --out "$RUNNER_TEMP/user-notes.md"');
  expect(release).toContain('cat "$RUNNER_TEMP/user-notes.md"');
  // The release body is replaced by the cleaned notes only (never the app's cut).
  // (the other edit only publishes the draft, its notes untouched).
  expect(release.match(/gh release edit[^\n]*/g)).toEqual([
    'gh release edit "$GITHUB_REF_NAME" --repo "$GITHUB_REPOSITORY" --notes-file "$RUNNER_TEMP/release-notes.md"',
    'gh release edit "$GITHUB_REF_NAME" --repo "$GITHUB_REPOSITORY" --draft=false --latest',
  ]);
  // Both cuts come from release-please's body, read before it is replaced.
  expect(release.indexOf("release-please.md")).toBeLessThan(release.indexOf("gh release edit"));
});

test("test builds embed the next release's notes and the e2e proves them in What's new", () => {
  const e2e = read("../.github/workflows/e2e-windows.yml");
  const testBuild = read("../.github/workflows/test-build.yml");
  for (const wf of [e2e, testBuild]) {
    expect(wf).toContain('node scripts/release-notes.mjs preview --out "$RUNNER_TEMP/next-notes.md"');
    expect(wf).toContain('echo "FREEWHEEL_NEXT_NOTES_FILE=$RUNNER_TEMP/next-notes.md" >> "$GITHUB_ENV"');
    expect(wf).toContain("fetch-depth: 0");
  }
  // The simulate e2e compares the app's embedded notes with the same file.
  expect(e2e).toContain('--next-notes "$env:FREEWHEEL_NEXT_NOTES_FILE"');
  // The update e2e publishes B with the next release's real notes.
  expect(e2e).toContain('node scripts/release-notes.mjs preview --version "$TO_VERSION" --out "$RUNNER_TEMP/notes-$TO_VERSION.md"');
  expect(e2e).toContain('--notes-file "$RUNNER_TEMP/notes-$TO_VERSION.md"');
  expect(read("./e2e-update-windows.mjs")).toContain("NOTES_ON_PAGE_JS, [JSON.stringify(expectedNotes)]");
  const sim = read("./e2e.mjs");
  for (const label of ["Next release (real notes)", "Sample notes", "Paste notes…"]) expect(sim).toContain(label);
  expect(read("../src-tauri/build.rs")).toContain('const NEXT_NOTES_ENV: &str = "FREEWHEEL_NEXT_NOTES_FILE";');
});

test("hand-written notes in release-notes/v<version>.md win over the generated ones", () => {
  const dir = mkdtempSync(join(tmpdir(), "fw-notes-"));
  mkdirSync(join(dir, CURATED_DIR));
  writeFileSync(join(dir, CURATED_DIR, "v1.2.3.md"), "## Hello\r\n\r\n* a thing\r\n\r\n");
  expect(curatedNotes("1.2.3", dir)).toBe("## Hello\n\n* a thing\n");
  expect(curatedNotes("v1.2.3", dir)).toBe("## Hello\n\n* a thing\n");
  expect(curatedNotes("1.2.4", dir)).toBeNull();
  expect(curatedNotes(undefined, dir)).toBeNull();
});

test("release-notes/v1.0.0.md is the first release's plain, user-facing notes", () => {
  const notes = curatedNotes("1.0.0");
  expect(notes).toMatch(/^## First release\n/);
  for (const part of ["### What you can do", "### Download", "### Updates", "portable", "SCHEMA.md", "Device test"]) expect(notes).toContain(part);
  for (const file of ["windows-x64.exe", "macos-universal.dmg", "linux-x86_64.AppImage"]) expect(notes).toContain(`freewheel-1.0.0-${file}`);
  expect(notes).not.toMatch(/freewheel-[a-z]*\d(?!\.\d)|\bscope\b|\bcard\b|commit|\bCI\b|e2e|handoff|prototype|owner/i);
  expect(userNotes(notes)).toBe(notes);
});

const RP_011 = read("./fixtures/release-notes/release-please-0.1.1.md");

test("v0.1.1: the GitHub release keeps scope labels and PR links, loses the developer fixes and the hashes", () => {
  const out = userNotes(RP_011);
  expect(out).toBe(read("./fixtures/release-notes/release-0.1.1.md"));
  expect(out).toMatch(/^## \[0\.1\.1\]\(https:\/\/github\.com\/brinkercode\/freewheel\/compare\/v0\.1\.0\.\.\.v0\.1\.1\) \(2026-11-20\)\n/);
  for (const h of ["### New", "### Fixed", "### Faster"]) expect(out).toContain(h);
  expect(out).toContain("* **device:** the Speed Editor reconnects by itself after the computer wakes from sleep ([#188](https://github.com/brinkercode/freewheel/issues/188))");
  expect(out).not.toMatch(/\*\*ci:\*\*|\*\*scripts:\*\*|#193|#194|\/commit\//);
  expect(userNotes(out)).toBe(out);
});

test("v0.1.1: What's new has no scope labels or PR numbers, a capital on each line and a plain version heading", () => {
  const out = appNotes(RP_011);
  expect(out).toBe(read("./fixtures/release-notes/app-0.1.1.md"));
  expect(out).toBe(
    [
      "## 0.1.1 (2026-11-20)",
      "",
      "### New",
      "",
      "* A key can open a folder",
      "* Smoother scrolling when you turn the wheel fast",
      "",
      "### Fixed",
      "",
      "* The Speed Editor reconnects by itself after the computer wakes from sleep",
      "* The tray icon no longer shows Off after Resolve closes",
      "",
      "### Faster",
      "",
      "* Use less memory while freewheel sits in the tray",
      "",
    ].join("\n"),
  );
  expect(out).not.toMatch(/\*\*|#\d|\]\(|github\.com/);
  // The app's cut of the release's cut is the same.
  expect(appNotes(userNotes(RP_011))).toBe(out);
  expect(appNotes(out)).toBe(out);
});

test("an app bullet: label and PR links go, the first letter is a capital, freewheel stays lowercase", () => {
  expect(appBullet("**keys:** a key can open a folder ([#190](https://github.com/o/r/issues/190))")).toBe("A key can open a folder");
  expect(appBullet("the wheel wakes up ([#154](https://github.com/o/r/pull/154))")).toBe("The wheel wakes up");
  expect(appBullet("the wheel wakes up (#154)")).toBe("The wheel wakes up");
  expect(appBullet("freewheel starts at login")).toBe("freewheel starts at login");
  expect(appBullet("`Shift` is released")).toBe("`Shift` is released");
  expect(appBullet("a [link](https://github.com/o/r) stays")).toBe("A [link](https://github.com/o/r) stays");
});

test("GitHub's generated notes get the app's cut too", () => {
  const generated = "## What's Changed\n* feat(keys): a key can open a folder by @someone in https://github.com/brinkercode/freewheel/pull/151\n";
  expect(appNotes(generated)).toBe("## What's Changed\n\n* A key can open a folder\n");
});

test("the CLI: `release` and `filter` give the two cuts, hand-written notes win for both", () => {
  const script = fileURLToPath(new URL("./release-notes.mjs", import.meta.url));
  const fixture = fileURLToPath(new URL("./fixtures/release-notes/release-please-0.1.1.md", import.meta.url));
  const run = (...args) => execFileSync(process.execPath, [script, ...args], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
  expect(run("release", "--in", fixture)).toBe(userNotes(RP_011));
  expect(run("filter", "--in", fixture)).toBe(appNotes(RP_011));
  expect(run("release", "--version", "1.0.0", "--in", fixture)).toBe(curatedNotes("1.0.0"));
  expect(run("filter", "--version", "1.0.0", "--in", fixture)).toBe(curatedNotes("1.0.0"));
});

test("release.yml makes the hand-written notes the release body and What's new", () => {
  const release = readFileSync(new URL("../.github/workflows/release.yml", import.meta.url), "utf8");
  // `release` and `filter` both give release-notes/v<version>.md when it exists.
  expect(release).toContain('node scripts/release-notes.mjs release --version "${GITHUB_REF_NAME#v}" --in');
  expect(release).toContain('node scripts/release-notes.mjs filter --version "${GITHUB_REF_NAME#v}" --in');
  expect(release).not.toContain("release-notes.mjs curated");
});

test("one cleanup: the app's What's new runs this script's appNotes (src/releaseNotes.js), and the shared GitHub-release fixture cleans as expected", async () => {
  const app = await import("../src/releaseNotes.js");
  const script = await import("./release-notes.mjs");
  for (const name of ["appNotes", "userNotes", "appBullet", "userBullet", "parseHeader"]) expect(script[name], name).toBe(app[name]);
  const fixture = (f) => readFileSync(new URL(`./fixtures/release-notes/${f}`, import.meta.url), "utf8");
  expect(app.appNotes(fixture("github-release.md"))).toBe(fixture("github-release.app.md"));
  // The release body a curated version publishes (release-notes/v1.0.0.md) keeps its content.
  const curated = readFileSync(new URL("../release-notes/v1.0.0.md", import.meta.url), "utf8");
  expect(app.appNotes(curated)).toContain("### What you can do");
});

test("app bullets start with a capital, but names keep their case (freewheel, macOS)", async () => {
  const { appBullet } = await import("../src/releaseNotes.js");
  expect(appBullet("**keys:** a key opens a folder ([#3](https://github.com/brinkercode/freewheel/issues/3))")).toBe("A key opens a folder");
  expect(appBullet("freewheel starts faster")).toBe("freewheel starts faster");
  expect(appBullet("macOS 15 (Apple Silicon and Intel): x.dmg")).toBe("macOS 15 (Apple Silicon and Intel): x.dmg");
  expect(appBullet("iPhone-style toggles")).toBe("iPhone-style toggles");
});
