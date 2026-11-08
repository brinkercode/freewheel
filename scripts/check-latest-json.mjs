#!/usr/bin/env node
// Checks a release's latest.json lists every platform freewheel ships, each
// with an https url to its release file and a signature (the universal
// macOS .dmg serves both macOS keys, the AppImage Linux; scripts/portable-release.mjs).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { PACKAGES, PLATFORMS as SERVED } from "./portable-release.mjs";

export const PLATFORMS = ["windows-x86_64", "darwin-aarch64", "darwin-x86_64", "linux-x86_64"];

/** Returns the list of problems found in latest.json's parsed content (empty when clean). */
export function checkLatestJson(json) {
  const platforms = json?.platforms ?? {};
  return PLATFORMS.flatMap((key) => {
    const p = platforms[key];
    if (!p) return [`missing platform '${key}'`];
    const problems = [];
    if (!/^https:\/\//.test(p.url ?? "")) problems.push(`'${key}' has no https url`);
    else if (json.version && !p.url.endsWith(`/${PACKAGES[SERVED[key]](json.version)}`))
      problems.push(`'${key}' is not ${PACKAGES[SERVED[key]](json.version)}: ${p.url}`);
    if (!p.signature) problems.push(`'${key}' has no signature`);
    return problems;
  });
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const file = process.argv[2];
  if (!file) {
    console.error("usage: check-latest-json.mjs <latest.json>");
    process.exit(1);
  }
  const problems = checkLatestJson(JSON.parse(readFileSync(file, "utf8")));
  for (const problem of problems) console.error(`${file}: ${problem}`);
  process.exit(problems.length ? 1 : 0);
}
