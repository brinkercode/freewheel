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

const indent = (line) => line.search(/\S/);

/** Index one past the last line of the block that starts at `start`. */
function blockEnd(lines, start) {
  const base = indent(lines[start]);
  let i = start + 1;
  while (i < lines.length && (lines[i].trim() === "" || indent(lines[i]) > base)) i++;
  return i;
}

/**
 * Lints the release workflow: every 'uses:' is pinned to a 40-hex commit SHA
 * (or is a local ./ workflow), the tauri-action job declares
 * 'environment: release', APPLE_* and TAURI_SIGNING_* appear only in the
 * tauri-action step env, and no secrets are inherited by called workflows.
 */
export function checkReleaseWorkflow(text) {
  const lines = text.split(/\r?\n/).map((l) => l.replace(/(^|\s)#.*$/, ""));
  const problems = [];
  lines.forEach((line, i) => {
    const uses = line.match(/uses:\s*(\S+)/);
    if (uses && !/^\.\//.test(uses[1]) && !/@[0-9a-f]{40}$/.test(uses[1])) {
      problems.push(`line ${i + 1}: '${uses[1]}' is not pinned to a 40-hex SHA`);
    }
    if (/secrets:\s*inherit/.test(line)) problems.push(`line ${i + 1}: uses 'secrets: inherit'`);
  });
  const step = lines.findIndex((l) => /^\s*-\s+uses:\s*tauri-apps\/tauri-action@/.test(l));
  if (step < 0) return [...problems, "no tauri-action step"];
  const stepEnd = blockEnd(lines, step);
  const env = lines.findIndex((l, i) => i > step && i < stepEnd && /^\s+env:\s*$/.test(l));
  const envEnd = env < 0 ? env : blockEnd(lines, env);
  lines.forEach((line, i) => {
    if (/APPLE_|TAURI_SIGNING_/.test(line) && !(i > env && i < envEnd)) {
      problems.push(`line ${i + 1}: APPLE_*/TAURI_SIGNING_* outside the tauri-action step env`);
    }
  });
  let job = step;
  while (job >= 0 && indent(lines[job]) !== 2) job--;
  const jobLines = job < 0 ? [] : lines.slice(job, blockEnd(lines, job));
  if (!jobLines.some((l) => /^ {4}environment:\s*release\s*$/.test(l))) {
    problems.push("the tauri-action job does not declare 'environment: release'");
  }
  return problems;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const release = process.argv[2] === "--release";
  const files = process.argv.slice(release ? 3 : 2);
  let failed = files.length === 0;
  if (failed) console.error("usage: check-workflow.mjs [--release] <workflow.yml>...");
  const check = release ? checkReleaseWorkflow : checkWorkflow;
  for (const file of files) {
    for (const problem of check(readFileSync(file, "utf8"))) {
      console.error(`${file}: ${problem}`);
      failed = true;
    }
  }
  process.exit(failed ? 1 : 0);
}
