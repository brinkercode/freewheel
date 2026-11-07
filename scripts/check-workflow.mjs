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

/** Where release secrets may appear: only in the env of these steps. */
export const SECRET_STEPS = [
  { re: /APPLE_/, step: "Build freewheel.app (macOS)", what: "APPLE_*" },
  { re: /TAURI_SIGNING_/, step: "Sign the packages", what: "TAURI_SIGNING_*" },
];

/** The [start, end) lines of the env block of the step named `name`, and its job's lines; null when absent. */
function stepEnv(lines, name) {
  const step = lines.findIndex((l) => new RegExp(`^\\s*-\\s+name:\\s*${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*$`).test(l));
  if (step < 0) return null;
  const stepEnd = blockEnd(lines, step);
  const env = lines.findIndex((l, i) => i > step && i < stepEnd && /^\s+env:\s*$/.test(l));
  let job = step;
  while (job >= 0 && indent(lines[job]) !== 2) job--;
  return { env, envEnd: env < 0 ? env : blockEnd(lines, env), job: job < 0 ? [] : lines.slice(job, blockEnd(lines, job)) };
}

/**
 * Lints the release workflow: every 'uses:' is pinned to a 40-hex commit SHA
 * (or is a local ./ workflow), APPLE_* appear only in the macOS build step's
 * env and TAURI_SIGNING_* only in the sign step's env, both of those jobs
 * declare 'environment: release', and no secrets are inherited by called
 * workflows.
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
  for (const { re, step, what } of SECRET_STEPS) {
    const found = stepEnv(lines, step);
    if (!found) {
      problems.push(`no '${step}' step`);
      continue;
    }
    lines.forEach((line, i) => {
      if (re.test(line) && !(i > found.env && i < found.envEnd)) problems.push(`line ${i + 1}: ${what} outside the '${step}' step env`);
    });
    if (!found.job.some((l) => /^ {4}environment:\s*release\s*$/.test(l))) {
      problems.push(`the job of '${step}' does not declare 'environment: release'`);
    }
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
