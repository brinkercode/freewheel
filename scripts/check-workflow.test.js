import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { checkWorkflow } from "./check-workflow.mjs";

const ci = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
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

const job = (name) => {
  const m = ci.match(new RegExp(`^  ${name}:\\r?\\n((?:    .*\\r?\\n|\\r?\\n)*)`, "m"));
  return m ? m[1] : "";
};

test("ci.yml matrix is exactly the hosted Windows and macOS runners, after a Linux lint job", () => {
  const test_ = job("test");
  expect(test_).toMatch(/os: \[windows-2025, macos-15, macos-15-intel\]/);
  expect(test_).toMatch(/^    runs-on: \$\{\{ matrix\.os \}\}$/m);
  expect(test_).toMatch(/^    needs: lint$/m);
  expect(job("lint")).toMatch(/^    runs-on: ubuntu-latest$/m);
  expect(ci.match(/runs-on:/g)).toHaveLength(2);
  expect(ci).not.toMatch(/self-hosted/);
  for (const j of ["lint", "test"]) expect(job(j)).toMatch(/^    timeout-minutes: \d+$/m);
});

test("the lint job fails formatting first", () => {
  const steps = job("lint").match(/run: (cargo|npm) .*/g);
  expect(steps[0]).toBe("run: cargo fmt --all -- --check");
  expect(steps).toEqual(expect.arrayContaining([
    "run: npm ci", "run: npm test", "run: npm run build",
    "run: cargo clippy --workspace --all-targets -- -D warnings",
  ]));
});

test("ci.yml uses the pinned toolchain, caches, and runs every check", () => {
  expect(ci).not.toMatch(/\bstable\b|dtolnay\/rust-toolchain/);
  expect(ci.match(/^\s+rustup toolchain install$/gm)).toHaveLength(2);
  for (const step of [
    "cargo fmt --all -- --check",
    "cargo clippy --workspace --all-targets -- -D warnings",
    "cargo test --workspace --locked",
    "npm ci",
    "npm test",
    "npm run build",
  ]) {
    expect(ci).toContain(`run: ${step}`);
  }
  for (const uses of ci.match(/uses: \S+/g)) {
    expect(uses).toMatch(/@[0-9a-f]{40}$/);
  }
  for (const j of ["lint", "test"]) {
    expect(job(j)).toMatch(/uses: Swatinem\/rust-cache@[0-9a-f]{40} # v2\r?\n\s+with:\r?\n\s+workspaces: "\. -> target"/);
    expect(job(j)).toMatch(/uses: actions\/setup-node@[0-9a-f]{40}.*\r?\n\s+with:\r?\n(?:\s+.*\r?\n)*?\s+cache: npm/);
  }
  const toolchain = readFileSync(new URL("../rust-toolchain.toml", import.meta.url), "utf8");
  expect(toolchain).toMatch(/channel = "1\.99\.1"/);
  expect(toolchain).toMatch(/components = \["rustfmt", "clippy"\]/);
});

test("ci.yml triggers exactly as written, and cancels superseded runs", () => {
  const on = ci.match(/^on:\r?\n((?:[ \t]+.*\r?\n|\r?\n)*)/m)[1];
  expect(on.trimEnd()).toBe([
    "  pull_request:",
    '    paths-ignore: ["docs/**", "**.md"]',
    "  push:",
    "    branches: [main]",
    '    tags: ["v*"]',
    '    paths-ignore: ["docs/**", "**.md"]',
    "  workflow_dispatch:",
    "  workflow_call:",
  ].join("\n"));
  expect(ci).not.toMatch(/pull_request_target|schedule:/);
  expect(ci).toMatch(/^concurrency:\r?\n  group: ci-\$\{\{ github\.ref \}\}\r?\n  cancel-in-progress: true$/m);
});
