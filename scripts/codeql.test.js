import { readFileSync } from "node:fs";
import { expect, test } from "vitest";

const wf = readFileSync(new URL("../.github/workflows/codeql.yml", import.meta.url), "utf8");
const code = wf.split(/\r?\n/).map((l) => l.replace(/(^|\s)#.*$/, "")).join("\n");

test("codeql runs only on push to main and a weekly schedule, never on pull requests", () => {
  const on = code.match(/^on:\r?\n((?:[ \t]+.*\r?\n|\r?\n)*)/m)[1];
  expect(on.trimEnd()).toBe(
    ["  push:", "    branches: [main]", "  schedule:", '    - cron: "17 4 * * 1"'].join("\n"),
  );
  expect(code).not.toMatch(/pull_request|workflow_call|workflow_dispatch/);
});

test("codeql analyzes rust and javascript-typescript", () => {
  expect(code).toMatch(/^\s+language: \[rust, javascript-typescript\]$/m);
  expect(code).toMatch(/languages: \$\{\{ matrix\.language \}\}/);
});

test("every uses: is a GitHub-owned action pinned to a 40-hex SHA", () => {
  const uses = code.match(/uses: \S+/g);
  expect(uses.length).toBeGreaterThan(0);
  for (const u of uses) expect(u).toMatch(/^uses: (github\/codeql-action|actions)\/[\w/-]+@[0-9a-f]{40}$/);
});

test("codeql reads contents and writes only security events, with no secrets", () => {
  expect(code).toMatch(/^permissions:\r?\n  contents: read$/m);
  expect(code.match(/: write\b/g)).toEqual([": write"]);
  expect(code).toMatch(/^      security-events: write$/m);
  expect(code).not.toMatch(/secrets\./);
});
