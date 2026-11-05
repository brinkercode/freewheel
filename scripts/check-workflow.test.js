import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { checkWorkflow } from "./check-workflow.mjs";

const ci = readFileSync(new URL("./fixtures/proposed-ci.yml", import.meta.url), "utf8");
const clean = "on:\n  pull_request:\npermissions:\n  contents: read\njobs: {}\n";

test("ci.yml passes the workflow lint", () => {
  expect(checkWorkflow(ci)).toEqual([]);
});

test("a clean workflow passes", () => {
  expect(checkWorkflow(clean)).toEqual([]);
});

test.each([
  ["secrets.", clean + "env:\n  T: ${{ secrets.TOKEN }}\n", "uses 'secrets.'"],
  ["pull_request_target", clean.replace("pull_request:", "pull_request_target:"), "pull_request_target"],
  ["a write permission", clean + "  pull-requests: write\n", "write permission"],
  ["write-all", clean + "  x: write-all\n", "write-all"],
])("the lint fails on %s", (_name, text, msg) => {
  expect(checkWorkflow(text).join("\n")).toContain(msg);
});

test("the lint fails without 'permissions: contents: read'", () => {
  expect(checkWorkflow("on: push\njobs: {}\n")).toEqual([
    "missing top-level 'permissions: contents: read'",
  ]);
});

test("ci.yml matrix is exactly the hosted Windows and macOS runners", () => {
  expect(ci).toMatch(/os: \[windows-2025, macos-15, macos-15-intel\]/);
  expect(ci).toMatch(/^\s+runs-on: \$\{\{ matrix\.os \}\}$/m);
  expect(ci.match(/runs-on:/g)).toHaveLength(1);
  expect(ci).not.toMatch(/ubuntu|self-hosted/);
  expect(ci).toMatch(/^\s+workflow_call:/m);
});

test("ci.yml uses the pinned toolchain and runs every check", () => {
  expect(ci).not.toMatch(/\bstable\b|dtolnay\/rust-toolchain/);
  for (const step of [
    "cargo fmt --all -- --check",
    "cargo clippy --workspace --all-targets -- -D warnings",
    "cargo test --workspace",
    "npm ci",
    "npm test",
    "npm run build",
  ]) {
    expect(ci).toContain(`run: ${step}`);
  }
  for (const uses of ci.match(/uses: \S+/g)) {
    expect(uses).toMatch(/@[0-9a-f]{40}$/);
  }
  const toolchain = readFileSync(new URL("../rust-toolchain.toml", import.meta.url), "utf8");
  expect(toolchain).toMatch(/channel = "1\.99\.1"/);
  expect(toolchain).toMatch(/components = \["rustfmt", "clippy"\]/);
});
