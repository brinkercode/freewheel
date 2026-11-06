import { readFileSync } from "node:fs";
import { expect, test } from "vitest";

const read = (name) => readFileSync(new URL(`../.github/ISSUE_TEMPLATE/${name}`, import.meta.url), "utf8");
const bug = read("bug_report.yml");
const feature = read("feature_request.yml");
const config = read("config.yml");

// One body entry per `  - type:` item, keyed by its label.
const fields = (form) =>
  Object.fromEntries(
    form
      .split(/^  - type: /m)
      .slice(1)
      .map((f) => [f.match(/^      label: (.*)$/m)?.[1], f]),
  );
const required = (field) => /^      required: true$/m.test(field);

test.each([
  ["bug_report.yml", bug],
  ["feature_request.yml", feature],
])("%s has the issue-form top-level keys", (_name, form) => {
  for (const key of ["name", "description", "body"]) expect(form).toMatch(new RegExp(`^${key}:`, "m"));
});

test("the bug report asks for system, version, connection, steps, expected, actual and logs", () => {
  const f = fields(bug);
  expect(Object.keys(f)).toEqual([
    "undefined", // the intro text
    "Your computer's system and version",
    "freewheel version",
    "How is your Speed Editor connected?",
    "Steps to reproduce",
    "What did you expect to happen?",
    "What happened instead?",
    "Log files",
  ]);
  expect(f["How is your Speed Editor connected?"]).toMatch(/^dropdown[\s\S]*- USB\n\s+- Bluetooth\n/);
  expect(f["Log files"]).toContain("(https://github.com/brinkercode/freewheel/blob/main/SUPPORT.md)");
});

test("SUPPORT.md explains where the log files are and how to attach them", () => {
  const support = readFileSync(new URL("../SUPPORT.md", import.meta.url), "utf8");
  expect(support).toContain("## Finding your log files");
  expect(support).toContain("## Attaching them");
});

test("the feature request requires the two questions; the rest is optional", () => {
  const f = fields(feature);
  expect(required(f["What are you trying to do?"])).toBe(true);
  expect(required(f["What would you like freewheel to do?"])).toBe(true);
  expect(required(f["Anything else?"])).toBe(false);
  const os = f["Which computer do you use?"];
  expect(os).toMatch(/^dropdown[\s\S]*- Windows\n\s+- macOS\n/);
  expect(required(os)).toBe(false);
});

test("config.yml turns blank issues off and sends security reports to private reporting", () => {
  expect(config).toMatch(/^blank_issues_enabled: false$/m);
  expect(config).toMatch(/^ {4}url: https:\/\/github\.com\/brinkercode\/freewheel\/security\/advisories\/new$/m);
  expect(config).toContain("do not open a public issue for security problems");
});

test("no template mentions internal tooling", () => {
  for (const text of [bug, feature, config]) expect(text).not.toMatch(/factory|opal|tracker|planning/i);
});
