#!/usr/bin/env node
// The Windows update e2e's build settings (e2e builds only, never a shipped
// build): the `tauri build --config` override with the version to build,
// and the env that bakes the throwaway test pubkey (generated in the job)
// into it. The updater's manifest URL is set at run time instead
// (FREEWHEEL_E2E_UPDATE_URL, read only by `e2e` feature builds).
//
//   node scripts/e2e-update-config.mjs <version>       prints the --config JSON
//   node scripts/e2e-update-config.mjs --next <version> prints the next minor version
import { fileURLToPath } from "node:url";

/** The next minor version: "0.1.0" -> "0.2.0". */
export function nextVersion(version) {
  const [major = 0, minor = 0] = String(version ?? "0.0.0")
    .replace(/^v/, "")
    .split("-")[0]
    .split(".")
    .map((n) => Number.parseInt(n, 10) || 0);
  return `${major}.${minor + 1}.0`;
}

/** The --config override for one e2e build. */
export function e2eConfig({ version }) {
  if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) throw new Error(`not a version: ${version}`);
  return { version };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [a, b] = process.argv.slice(2);
  if (a === "--next") console.log(nextVersion(b));
  else console.log(JSON.stringify(e2eConfig({ version: a })));
}
