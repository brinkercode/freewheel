#!/usr/bin/env node
// The Windows update e2e's `tauri build --config` override (e2e builds only,
// never a shipped build): the version to build, the throwaway test pubkey
// generated in the job, and the updater endpoint on the runner's local
// release server (scripts/update-server.mjs), plain http on 127.0.0.1.
//
//   node scripts/e2e-update-config.mjs <version> <test key .pub file> <server port> > config.json
//   node scripts/e2e-update-config.mjs --next <version>     prints the next minor version
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { nextVersion } from "../src/simUpdate.js";

/** The override for one e2e build. */
export function e2eConfig({ version, pubkey, port }) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`not a version: ${version}`);
  if (!pubkey?.trim()) throw new Error("no test pubkey");
  const p = Number(port);
  if (!Number.isInteger(p) || p < 1 || p > 65535) throw new Error(`not a port: ${port}`);
  return {
    version,
    bundle: { createUpdaterArtifacts: true },
    plugins: {
      updater: {
        pubkey: pubkey.trim(),
        endpoints: [`http://127.0.0.1:${p}/latest.json`],
        // Local, loopback-only http for the e2e's server; shipped builds keep https (updater.test.jsx).
        dangerousInsecureTransportProtocol: true,
      },
    },
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [a, b, c] = process.argv.slice(2);
  if (a === "--next") console.log(nextVersion(b));
  else console.log(JSON.stringify(e2eConfig({ version: a, pubkey: readFileSync(b, "utf8"), port: c })));
}
