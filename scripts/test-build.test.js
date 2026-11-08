import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { checkWorkflow } from "./check-workflow.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const wf = read("../.github/workflows/test-build.yml");
const workflows = ["ci.yml", "release.yml", "test-build.yml", "release-please.yml", "update-e2e.yml", "e2e-windows.yml"].map((f) => [f, read(`../.github/workflows/${f}`)])
  .concat([["actions/appimage-tools/action.yml", read("../.github/actions/appimage-tools/action.yml")]]);

// Repo rules: SHA pinning required, selected actions only.
test.each(workflows)("%s pins every action to a commit SHA from an allowed owner", (_file, text) => {
  const uses = text.match(/uses: \S+/g);
  expect(uses.length).toBeGreaterThan(0);
  for (const u of uses) {
    if (u === "uses: ./.github/workflows/ci.yml" || u === "uses: ./.github/workflows/update-e2e.yml") continue;
    if (u === "uses: ./.github/actions/appimage-tools") continue;
    expect(u).toMatch(
      /^uses: (actions\/[\w-]+(?:\/[\w-]+)?|Swatinem\/rust-cache|tauri-apps\/tauri-action|googleapis\/release-please-action)@[0-9a-f]{40}$/,
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

test("test-build uploads freewheel.exe and freewheel-simulate.cmd unzipped, one artifact each, kept 1 day", () => {
  const uploads = [...wf.matchAll(/- name: (Upload .*)\r?\n\s+uses: (actions\/upload-artifact@[0-9a-f]{40}) # (v\S+)\r?\n\s+with:\r?\n((?:\s{10,}.*\r?\n?)*)/g)];
  expect(uploads.map((u) => u[1])).toEqual(["Upload freewheel.exe", "Upload freewheel-simulate.cmd"]);
  for (const [, , uses, version, body] of uploads) {
    // `archive: false` (one file, not zipped) needs upload-artifact v7.
    expect(uses).toBe("actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9");
    expect(version).toBe("v7.0.2");
    expect(body).toMatch(/^\s+archive: false$/m);
    expect(body).toMatch(/^\s+retention-days: 1$/m);
    expect(body).toMatch(/^\s+if-no-files-found: error$/m);
    expect(body).not.toMatch(/bundle|setup|name:/);
  }
  expect(uploads[0][4]).toMatch(/^\s+path: target\/release\/freewheel\.exe$/m);
  expect(uploads[1][4]).toMatch(/^\s+path: src-tauri\/windows\/freewheel-simulate\.cmd$/m);
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
  // Both uploads come after every check.
  expect(wf.lastIndexOf("uses: actions/upload-artifact")).toBeGreaterThan(upload);
});

test("a failed check fails the job, so nothing is uploaded", () => {
  expect(wf).not.toMatch(/continue-on-error|always\(\)|^\s+if:/m);
});

test("docs/releasing.md explains how to get and run an unsigned test build", () => {
  const doc = read("../docs/releasing.md");
  const section = doc.split(/^## /m).find((s) => s.startsWith("Test builds"));
  expect(section).toBeDefined();
  expect(section).toMatch(/Actions/);
  expect(section).toContain("](https://github.com/brinkercode/freewheel/actions/workflows/test-build.yml)");
  expect(section).toMatch(/artifact/i);
  expect(section).toMatch(/unsigned/i);
  expect(section).toContain("`freewheel.exe`");
  expect(section).not.toMatch(/setup\.exe|NSIS installer/);
  expect(section).toMatch(/SmartScreen[\s\S]*\*\*More info\*\*[\s\S]*\*\*Run anyway\*\*/);
});
