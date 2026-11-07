import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { checkWorkflow } from "./check-workflow.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const wf = read("../.github/workflows/test-build.yml");
const workflows = ["ci.yml", "release.yml", "test-build.yml", "release-please.yml", "update-e2e.yml", "e2e-windows.yml"].map((f) => [f, read(`../.github/workflows/${f}`)]);

// Repo rules: SHA pinning required, selected actions only.
test.each(workflows)("%s pins every action to a commit SHA from an allowed owner", (_file, text) => {
  const uses = text.match(/uses: \S+/g);
  expect(uses.length).toBeGreaterThan(0);
  for (const u of uses) {
    if (u === "uses: ./.github/workflows/ci.yml" || u === "uses: ./.github/workflows/update-e2e.yml") continue;
    expect(u).toMatch(
      /^uses: (actions\/[\w-]+|Swatinem\/rust-cache|tauri-apps\/tauri-action|googleapis\/release-please-action)@[0-9a-f]{40}$/,
    );
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

test("test-build job builds the portable exe on hosted Windows, no installer, no pubkey and no signing", () => {
  expect(wf.match(/runs-on: \S+/g)).toEqual(["runs-on: windows-latest"]);
  expect(wf).not.toMatch(/self-hosted/);
  expect(wf).toContain("run: npx tauri build --no-bundle -- --locked");
  const code = wf.split(/\r?\n/).map((l) => l.replace(/(^|\s)#.*$/, "")).join("\n");
  expect(code).not.toMatch(/npm run tauri|tauri-release|TAURI_SIGNING|PRIVATE_KEY|PASSWORD|PUBKEY|pubkey|secrets|nsis|setup\.exe/);
  for (const uses of wf.match(/uses: \S+/g)) expect(uses).toMatch(/@[0-9a-f]{40}$/);
});

test("test-build job uploads only the portable freewheel.exe and freewheel-simulate.cmd, kept 1 day", () => {
  const upload = wf.match(/- uses: actions\/upload-artifact@[0-9a-f]{40}.*\r?\n((?:\s{8,}.*\r?\n)*)/);
  expect(upload).not.toBeNull();
  expect(upload[1]).toMatch(/^\s+retention-days: 1$/m);
  expect(upload[1]).toMatch(/^\s+if-no-files-found: error$/m);
  expect(upload[1]).toMatch(/^\s+path: test-build$/m);
  expect(wf).toContain("cp target/release/freewheel.exe src-tauri/windows/freewheel-simulate.cmd test-build/");
  expect(upload[1]).not.toMatch(/bundle|setup/);
});

test("freewheel-simulate.cmd starts the exe next to it with --simulate, CRLF", () => {
  const cmd = read("../src-tauri/windows/freewheel-simulate.cmd");
  expect(cmd).toMatch(/^@echo off\r\n/);
  expect(cmd).toContain('start "" "%~dp0freewheel.exe" --simulate\r\n');
  expect(cmd.replace(/\r\n/g, "")).not.toContain("\n");
});

test("test-build runs release.yml's checks after the build and before the upload", () => {
  const release = read("../.github/workflows/release.yml");
  expect(release).toContain('node scripts/check-windows-exe.mjs "$bin"');
  expect(release).toContain("node scripts/check-release-binary.mjs");
  const build = wf.indexOf("run: npx tauri build");
  const gate = wf.indexOf("run: node scripts/check-windows-exe.mjs target/release/freewheel.exe");
  const e2e = wf.indexOf("run: node scripts/check-release-binary.mjs target/release/freewheel.exe");
  const upload = wf.indexOf("uses: actions/upload-artifact");
  expect(gate).toBeGreaterThan(build);
  expect(e2e).toBeGreaterThan(gate);
  expect(upload).toBeGreaterThan(e2e);
  expect(upload).toBe(wf.lastIndexOf("uses: actions/upload-artifact"));
});

test("a failed check fails the job, so nothing is uploaded", () => {
  expect(wf).not.toMatch(/continue-on-error|always\(\)|^\s+if:/m);
});

test("README explains how to get and run an unsigned test build", () => {
  const readme = read("../README.md");
  const section = readme.split(/^## /m).find((s) => s.startsWith("Test builds"));
  expect(section).toBeDefined();
  expect(section).toMatch(/Actions/);
  expect(section).toContain("](../../actions/workflows/test-build.yml)");
  expect(section).toMatch(/artifact/i);
  expect(section).toMatch(/unsigned/i);
  expect(section).toContain("`freewheel.exe`");
  expect(section).not.toMatch(/setup\.exe|NSIS installer/);
  expect(section).toMatch(/SmartScreen[\s\S]*\*\*More info\*\*[\s\S]*\*\*Run anyway\*\*/);
});
