#!/usr/bin/env node
// Lints GitHub Actions workflows: fails on any use of secrets, on
// pull_request_target, on any write permission, and when the workflow does
// not declare 'permissions: contents: read'.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const RULES = [
  { re: /secrets\./, msg: "uses 'secrets.'" },
  { re: /pull_request_target/, msg: "uses 'pull_request_target'" },
  { re: /:\s*write\b/, msg: "grants a write permission" },
  { re: /write-all/, msg: "grants write-all" },
];

/** Returns the list of problems found in a workflow's text (empty when clean). */
export function checkWorkflow(text) {
  const problems = [];
  text.split(/\r?\n/).forEach((line, i) => {
    const code = line.replace(/(^|\s)#.*$/, "");
    for (const { re, msg } of RULES) {
      if (re.test(code)) problems.push(`line ${i + 1}: ${msg}`);
    }
  });
  if (!/^permissions:\s*\r?\n\s+contents:\s*read\s*$/m.test(text)) {
    problems.push("missing top-level 'permissions: contents: read'");
  }
  return problems;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const files = process.argv.slice(2);
  let failed = files.length === 0;
  if (failed) console.error("usage: check-workflow.mjs <workflow.yml>...");
  for (const file of files) {
    for (const problem of checkWorkflow(readFileSync(file, "utf8"))) {
      console.error(`${file}: ${problem}`);
      failed = true;
    }
  }
  process.exit(failed ? 1 : 0);
}
