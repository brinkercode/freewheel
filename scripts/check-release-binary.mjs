#!/usr/bin/env node
// Checks a shipped freewheel binary (release.yml, test-build.yml): it must
// not contain the e2e-only update override (the `e2e` cargo feature, which
// lets the update e2e point the updater at a local server), and with
// --pubkey it must contain the updater pubkey baked in at build (else the
// release could never update itself).
//
//   node scripts/check-release-binary.mjs <freewheel.exe | freewheel.app/Contents/MacOS/freewheel> [--pubkey <base64>]
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** Names only e2e builds contain (src-tauri/src/selfupdate.rs, release_notes.rs). */
export const E2E_ONLY = ["FREEWHEEL_E2E_UPDATE_URL", "FREEWHEEL_E2E_RELEASES_API"];

/** Problems with the binary's bytes (empty when it is fit to ship). */
export function binaryProblems(bytes, { pubkey } = {}) {
  const problems = E2E_ONLY.filter((name) => bytes.includes(Buffer.from(name))).map(
    (name) => `contains ${name}: built with the e2e feature, never ship it`,
  );
  if (pubkey !== undefined) {
    if (!pubkey.trim()) problems.push("no pubkey given to look for");
    else if (!bytes.includes(Buffer.from(pubkey.trim()))) problems.push("the updater pubkey is not baked in (FREEWHEEL_UPDATER_PUBKEY at build)");
  }
  return problems;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [file, flag, pubkey] = process.argv.slice(2);
  if (!file || (flag && flag !== "--pubkey")) {
    console.error("usage: check-release-binary.mjs <binary> [--pubkey <base64>]");
    process.exit(1);
  }
  const problems = binaryProblems(readFileSync(file), flag ? { pubkey: pubkey ?? "" } : {});
  for (const p of problems) console.error(`${file}: ${p}`);
  if (!problems.length) console.log(`${file}: fit to ship${flag ? " (updater pubkey baked in)" : ""}`);
  process.exit(problems.length ? 1 : 0);
}
