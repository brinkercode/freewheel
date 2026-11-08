import { readdirSync, readFileSync } from "node:fs";
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

test("ci.yml matrix is exactly the hosted Windows, macOS and Linux runners, after a Linux lint job", () => {
  const test_ = job("test");
  expect(test_).toMatch(/os: \[windows-2025, macos-15, macos-15-intel, ubuntu-latest\]/);
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
  ]));
  expect(job("lint")).toContain("node scripts/check-workflow.mjs .github/workflows/ci.yml");
  expect(job("lint")).toContain("node scripts/check-workflow.mjs --release .github/workflows/release.yml");
});

test("the lint job needs no system packages: no apt step and no clippy", () => {
  const lint = job("lint");
  expect(lint).not.toContain("Install Tauri Linux dependencies");
  expect(lint).not.toMatch(/\bapt(-get)?\b/);
  expect(lint).not.toMatch(/cargo clippy/);
});

test("the Linux test job installs WebKitGTK and proves real key injection", () => {
  const test_ = job("test");
  expect(test_).toMatch(/- name: Install Tauri Linux dependencies\n\s+if: runner\.os == 'Linux'\n/);
  expect(test_).toContain("libwebkit2gtk-4.1-dev libayatana-appindicator3-dev");
  expect(test_).toMatch(/- name: Real key injection \(Linux\)\n\s+if: runner\.os == 'Linux'\n/);
  expect(test_).toContain("udevadm verify --resolve-names=never src-tauri/linux/60-freewheel.rules");
  expect(test_).toContain("sudo modprobe uinput");
  expect(test_).toContain("xvfb-run -a cargo test -p freewheel --locked -- --ignored --nocapture xtest_ uinput_");
});

test("clippy runs in the test job on every matrix runner, windows-2025 included", () => {
  const test_ = job("test");
  expect(test_).toMatch(/os: \[windows-2025, macos-15, macos-15-intel, ubuntu-latest\]/);
  expect(test_).toMatch(/^      - run: cargo clippy --workspace --all-targets -- -D warnings$/m);
  expect(test_).not.toMatch(/if: .*\r?\n\s+run: cargo clippy/);
});

test("the required job names stay exactly", () => {
  expect(job("lint")).toMatch(/^    name: lint \(ubuntu-latest\)$/m);
  expect(job("test")).toMatch(/^    name: test \(\$\{\{ matrix\.os \}\}\)$/m);
  const names = job("test").match(/os: \[(.*)\]/)[1].split(", ").map((os) => `test (${os})`);
  expect(["lint (ubuntu-latest)", ...names]).toEqual([
    "lint (ubuntu-latest)", "test (windows-2025)", "test (macos-15)", "test (macos-15-intel)", "test (ubuntu-latest)",
  ]);
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
  expect(job("test")).toMatch(/uses: Swatinem\/rust-cache@[0-9a-f]{40} # v2\r?\n\s+with:\r?\n\s+workspaces: "\. -> target"/);
  for (const j of ["lint", "test"]) {
    // Line-based (no backtracking regex): the setup-node step's `with:` block sets `cache: npm`.
    const lines = job(j).split(/\r?\n/);
    const at = lines.findIndex((l) => /uses: actions\/setup-node@[0-9a-f]{40}/.test(l));
    expect(at).toBeGreaterThanOrEqual(0);
    expect(lines[at + 1]?.trim()).toBe("with:");
    const indent = (l) => l.length - l.trimStart().length;
    const block = [];
    for (let i = at + 2; i < lines.length && indent(lines[i]) > indent(lines[at + 1]); i++) block.push(lines[i].trim());
    expect(block).toContain("cache: npm");
  }
  const toolchain = readFileSync(new URL("../rust-toolchain.toml", import.meta.url), "utf8");
  expect(toolchain).toMatch(/channel = "1\.99\.1"/);
  expect(toolchain).toMatch(/components = \["rustfmt", "clippy"\]/);
});

test("ci.yml triggers exactly as written, and cancels superseded runs", () => {
  const on = ci.match(/^on:\r?\n((?:[ \t]+.*\r?\n|\r?\n)*)/m)[1];
  expect(on.trimEnd()).toBe([
    "  pull_request:",
    "  push:",
    "    branches: [main]",
    "  workflow_dispatch:",
    "  workflow_call:",
  ].join("\n"));
  expect(ci).not.toMatch(/pull_request_target|schedule:/);
  expect(ci).toMatch(/^concurrency:\r?\n  group: ci-\$\{\{ github\.workflow \}\}-\$\{\{ github\.ref \}\}\r?\n  cancel-in-progress: \$\{\{ github\.event_name == 'pull_request' \}\}$/m);
});

test("a push to main never cancels a run: only a pull request's newer push does", () => {
  for (const f of readdirSync(new URL("../.github/workflows/", import.meta.url))) {
    const wf = readFileSync(new URL(`../.github/workflows/${f}`, import.meta.url), "utf8");
    expect(wf, f).not.toMatch(/cancel-in-progress: true/);
  }
});
