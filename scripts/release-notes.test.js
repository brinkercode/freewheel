// The user-facing release notes (scripts/release-notes.mjs): what
// release-please writes, what What's new shows, and the preview of the next
// release from the repo's history that the Simulator and the update e2e use.
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, expect, test } from "vitest";
import {
  CONFIG,
  CURATED_DIR,
  curatedNotes,
  NOTHING_VISIBLE,
  RELEASE_PLEASE,
  nextVersion,
  parseCommit,
  preview,
  releasePleaseNotes,
  userBullet,
  userNotes,
} from "./release-notes.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");

test("release-please shows Features, Bug Fixes and Performance, and hides the developer sections", () => {
  const sections = RELEASE_PLEASE["changelog-sections"];
  expect(sections.filter((s) => !s.hidden).map((s) => [s.type, s.section])).toEqual([
    ["feat", "Features"],
    ["fix", "Bug Fixes"],
    ["perf", "Performance Improvements"],
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

test("release-please's notes become What's new's: developer bullets, hashes and hidden sections go, PR links stay", () => {
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
  const dev = "## [0.1.1](x) (2026-11-08)\n\n\n### Bug Fixes\n\n* **ci:** retry ([abc1234](https://github.com/o/r/commit/abc1234))\n";
  expect(userNotes(dev)).toBe(`## [0.1.1](x) (2026-11-08)\n\n${NOTHING_VISIBLE}\n`);
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
      "### Features",
      "",
      `* **config:** a new mappings format ([fffffff](${R}/commit/${"f".repeat(40)}))`,
      `* **updates:** portable self-update ([#145](${R}/issues/145)) ([bbbbbbb](${R}/commit/${"b".repeat(40)}))`,
      "",
      "### Bug Fixes",
      "",
      `* **scripts:** resolve the three CodeQL alerts ([#148](${R}/issues/148)) ([aaaaaaa](${R}/commit/${"a".repeat(40)}))`,
      `* the wheel wakes up ([eeeeeee](${R}/commit/${"e".repeat(40)}))`,
      "",
      "### Performance Improvements",
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
  expect(p.notes).toContain("* **keys:** before the tag ([#1](");
  git("tag", "v0.1.0");
  commit("fix(ci): only CI (#2)");
  commit("fix(wheel): smoother (#3)");
  p = preview({ dir: repo, date: "2026-11-20" });
  expect(p).toMatchObject({ tag: "v0.1.0", version: "0.1.1", commits: 2 });
  expect(p.notes).toBe(
    [
      "## [0.1.1](https://github.com/brinkercode/freewheel/compare/v0.1.0...v0.1.1) (2026-11-20)",
      "",
      "### Bug Fixes",
      "",
      "* **wheel:** smoother ([#3](https://github.com/brinkercode/freewheel/issues/3))",
      "",
    ].join("\n"),
  );
  // One filter for the release and the preview: the preview is the release's notes filtered.
  expect(p.notes).toBe(userNotes(p.raw));
});

test("release.yml gives latest.json the filtered notes and keeps the release's own (unless hand-written notes replace both)", () => {
  const release = read("../.github/workflows/release.yml");
  expect(release).toContain('node scripts/release-notes.mjs filter --version "${GITHUB_REF_NAME#v}" --in "$RUNNER_TEMP/release-notes.md" --out "$RUNNER_TEMP/user-notes.md"');
  expect(release).toContain('cat "$RUNNER_TEMP/user-notes.md"');
  // The release body is only ever replaced by hand-written notes.
  expect(release.match(/gh release edit[^\n]*/g)).toEqual(['gh release edit "$GITHUB_REF_NAME" --repo "$GITHUB_REPOSITORY" --notes-file "$RUNNER_TEMP/curated.md"']);
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

test("release-notes/v0.1.0.md is the first release's plain, user-facing notes", () => {
  const notes = curatedNotes("0.1.0");
  expect(notes).toMatch(/^## First release\n/);
  for (const part of ["### Download", "### Updates", "portable", "--simulate"]) expect(notes).toContain(part);
  expect(notes).not.toMatch(/freewheel-[a-z]*\d|\bscope\b|\bcard\b|commit|\bCI\b|e2e|handoff|prototype|owner/i);
  expect(userNotes(notes)).toBe(notes);
});

test("release.yml makes the hand-written notes the release body and What's new", () => {
  const release = readFileSync(new URL("../.github/workflows/release.yml", import.meta.url), "utf8");
  expect(release).toContain('node scripts/release-notes.mjs curated --version "${GITHUB_REF_NAME#v}" --out "$RUNNER_TEMP/curated.md"');
  expect(release).toContain('gh release edit "$GITHUB_REF_NAME" --repo "$GITHUB_REPOSITORY" --notes-file "$RUNNER_TEMP/curated.md"');
  expect(release).toContain('node scripts/release-notes.mjs filter --version "${GITHUB_REF_NAME#v}" --in');
});
