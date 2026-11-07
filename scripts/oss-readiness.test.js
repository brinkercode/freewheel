import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test } from "vitest";
import { DENYLIST, communityProblems, fileRules, lineHits, npmLicenseProblems, parseRules, textHits } from "./oss-readiness.mjs";

const rules = parseRules(readFileSync(new URL(`../${DENYLIST}`, import.meta.url), "utf8"));
const cats = (line) => lineHits(line, rules).map((h) => h.rule.category);
// Built from parts so this file never holds the values it tests for.
const j = (...parts) => parts.join("");

test("private hosts, VPN names and addresses, and the cards port are caught", () => {
  expect(cats(j("ssh brinker-", "node01"))).toContain("infra");
  expect(cats(j("https://box.tail1234", ".ts", ".net/x"))).toContain("infra");
  expect(cats(j("100.", "101.7.9"))).toContain("infra");
  expect(cats(j("http://cards", ":8765/api"))).toContain("infra");
  expect(cats("freewheel 0.1.0, 1.10.4, tauri 2.12.5")).toEqual([]);
});

test("home paths and machine-local tool paths are caught; example paths are not", () => {
  expect(cats(j("/home/", "brinkercode/repos"))).toContain("path");
  expect(cats(j("~/.her", "mes/tools/chrome"))).toContain("path");
  expect(cats("/home/a/.local/share/applications")).toEqual([]);
  expect(cats("C:\\Users\\you\\AppData\\Roaming")).toEqual([]);
});

test("card, goal and record ids are caught; version strings and crate names are not", () => {
  for (const id of [j("freewheel-", "m6-2"), j("freewheel-", "goal-7"), j("freewheel-", "15"), j("freewheel-", "records")]) expect(cats(`feat(${id}): x`)).toContain("process");
  for (const ok of ["freewheel-0.1.0-windows-x64.zip", "freewheel-core", "freewheel-logs-20261106.txt", "freewheel-mappings.json"]) expect(cats(ok)).toEqual([]);
  expect(cats(j("Card", ": x"))).toContain("process");
});

test("messages-only rules: ticket scopes, the owner and the handoff in commit messages and PR text, never in files", () => {
  const msg = (line) => lineHits(line, rules).map((h) => h.rule.category);
  expect(msg(j("feat(freewheel-", "apply-workflows): x"))).toContain("process");
  expect(msg("fix(freewheel-core): x")).toEqual([]);
  expect(msg(j("Fixes what the ", "owner hit"))).toContain("process");
  expect(msg(j("rebuilt to the design hand", "off"))).toContain("design");
  expect(fileRules(rules).some((r) => r.scope === "messages")).toBe(false);
});

test("design handoff paths are caught", () => {
  expect(cats(j("see design/", "handoff/tokens.css"))).toContain("design");
  expect(cats(j("freewheel", ".dc.html"))).toContain("design");
});

test("hashed rules match whole words and never print them", () => {
  const word = "sekritword";
  const hashed = parseRules(`path sha256:${createHash("sha256").update(word).digest("hex")}`);
  expect(textHits(`C:\\Users\\${word.toUpperCase()}\\x`, hashed)).toEqual([expect.objectContaining({ line: 1, match: "<hashed word>" })]);
  expect(textHits(`${word}s`, hashed)).toEqual([]);
});

test("personal e-mail addresses are caught and redacted; the GitHub no-reply address is fine", () => {
  const hits = lineHits(j("Signed-off-by: x <someone@", "gmail.com>"), rules);
  expect(hits.map((h) => h.rule.category)).toContain("identity");
  expect(hits.every((h) => !h.match.includes("@"))).toBe(true);
  expect(cats("Signed-off-by: brinkercode <brinkercode@users.noreply.github.com>")).toEqual([]);
});

test("the denylist holds no private literal it hashes", () => {
  const text = readFileSync(new URL(`../${DENYLIST}`, import.meta.url), "utf8");
  expect(textHits(text, rules.filter((r) => r.kind === "sha256"))).toEqual([]);
});

test("the repository has its community files, MIT metadata and pinned workflows", () => {
  expect(communityProblems()).toEqual([]);
});

test("community problems name what is missing or unpinned", () => {
  const dir = mkdtempSync(join(tmpdir(), "fw-oss-"));
  mkdirSync(join(dir, ".github/workflows"), { recursive: true });
  writeFileSync(join(dir, "LICENSE"), "Apache License\n");
  writeFileSync(join(dir, "package.json"), '{"license":"ISC"}');
  writeFileSync(join(dir, ".github/workflows/x.yml"), "on: push\njobs:\n  a:\n    steps:\n      - uses: actions/checkout@v4\n      - uses: ./local\n");
  const p = communityProblems(dir);
  expect(p).toContain("SECURITY.md is missing");
  expect(p).toContain("LICENSE is not the MIT License");
  expect(p).toContain("package.json license is not MIT");
  expect(p).toContain(".github/workflows/x.yml: no top-level permissions");
  expect(p).toContain(".github/workflows/x.yml: actions/checkout@v4 is not pinned to a commit sha");
  expect(p.join("\n")).not.toMatch(/\.\/local/);
});

test("npm licences: SPDX OR/AND expressions against the allow list", () => {
  const lock = { packages: { "": {}, "node_modules/a": { license: "MIT" }, "node_modules/b": { license: "(MIT OR GPL-3.0)" }, "node_modules/c": { license: "GPL-3.0" }, "node_modules/d": {} } };
  expect(npmLicenseProblems(lock)).toEqual(["node_modules/c: GPL-3.0 is not allowed", "node_modules/d: no licence in package-lock.json"]);
});
