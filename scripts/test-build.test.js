import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { checkWorkflow } from "./check-workflow.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const wf = read("../.github/workflows/test-build.yml");
const workflows = ["ci.yml", "release.yml", "test-build.yml"].map((f) => [f, read(`../.github/workflows/${f}`)]);

// Repo rules: SHA pinning required, selected actions only.
test.each(workflows)("%s pins every action to a commit SHA from an allowed owner", (_file, text) => {
  const uses = text.match(/uses: \S+/g);
  expect(uses.length).toBeGreaterThan(0);
  for (const u of uses) {
    if (u === "uses: ./.github/workflows/ci.yml") continue;
    expect(u).toMatch(/^uses: (actions\/[\w-]+|Swatinem\/rust-cache|tauri-apps\/tauri-action)@[0-9a-f]{40}$/);
    expect(u).not.toMatch(/@0{40}$/);
  }
});

test("test-build workflow passes the workflow lint (no secrets, contents: read)", () => {
  expect(checkWorkflow(wf)).toEqual([]);
});

test("test-build workflow runs only on manual dispatch and pushes to main", () => {
  const on = wf.match(/^on:\r?\n((?:[ \t]+.*\r?\n|\r?\n)*)/m)[1];
  expect(on.trimEnd()).toBe(["  workflow_dispatch:", "  push:", "    branches: [main]"].join("\n"));
  expect(wf).not.toMatch(/pull_request|workflow_call|schedule:|tags:/);
});

test("test-build job runs tauri build on hosted Windows with updater artifacts off and no signing", () => {
  expect(wf.match(/runs-on: \S+/g)).toEqual(["runs-on: windows-latest"]);
  expect(wf).not.toMatch(/self-hosted/);
  expect(wf).toMatch(/run: npx tauri build --bundles nsis --config '\{"bundle":\{"createUpdaterArtifacts":false\}\}' -- --locked/);
  const code = wf.split(/\r?\n/).map((l) => l.replace(/(^|\s)#.*$/, "")).join("\n");
  expect(code).not.toMatch(/npm run tauri|tauri-release|TAURI_SIGNING|PRIVATE_KEY|PASSWORD|PUBKEY|pubkey|secrets/);
  for (const uses of wf.match(/uses: \S+/g)) expect(uses).toMatch(/@[0-9a-f]{40}$/);
});

test("test-build job uploads the NSIS setup and the portable freewheel.exe, kept 1 day", () => {
  const upload = wf.match(/- uses: actions\/upload-artifact@[0-9a-f]{40}.*\r?\n((?:\s{8,}.*\r?\n)*)/);
  expect(upload).not.toBeNull();
  expect(upload[1]).toMatch(/^\s+retention-days: 1$/m);
  expect(upload[1]).toMatch(/^\s+if-no-files-found: error$/m);
  expect(upload[1]).toContain("target/release/freewheel.exe");
  expect(upload[1]).toContain("target/release/bundle/nsis/*-setup.exe");
});

test("README explains how to get and run an unsigned test build", () => {
  const readme = read("../README.md");
  const section = readme.split(/^## /m).find((s) => s.startsWith("Test builds"));
  expect(section).toBeDefined();
  expect(section).toMatch(/Actions/);
  expect(section).toContain("](../../actions/workflows/test-build.yml)");
  expect(section).toMatch(/artifact/i);
  expect(section).toMatch(/unsigned/i);
  expect(section).toMatch(/SmartScreen[\s\S]*\*\*More info\*\*[\s\S]*\*\*Run anyway\*\*/);
});
