#!/usr/bin/env node
// The release workflow's tauriScript. For `build` it checks the updater
// pubkey variable and the signing secrets are set, then injects the pubkey
// with `tauri build --config`: no key of any kind is committed to the repo.
// Empty APPLE_* variables (the secrets are not set yet) are removed first:
// the bundler treats a set-but-empty APPLE_CERTIFICATE as a certificate and
// fails, so until they exist the macOS app is ad-hoc signed and not notarized.
// Every other command passes through to the Tauri CLI unchanged.
import { fileURLToPath } from "node:url";

/** The GitHub Actions variable (public) and the secrets the release build needs. */
export const RELEASE_ENV = ["TAURI_UPDATER_PUBKEY", "TAURI_SIGNING_PRIVATE_KEY", "TAURI_SIGNING_PRIVATE_KEY_PASSWORD"];

/** Returns { problems, args }: the Tauri CLI args with the pubkey override, or what is missing. */
export function releaseArgs(args, env) {
  if (args[0] !== "build") return { problems: [], args };
  const missing = RELEASE_ENV.filter((name) => !env[name]?.trim());
  if (missing.length) {
    return {
      problems: [
        `release build needs ${missing.join(", ")}: set the variable TAURI_UPDATER_PUBKEY and the secrets ` +
          "TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD in the 'release' environment",
      ],
      args: null,
    };
  }
  const config = JSON.stringify({ plugins: { updater: { pubkey: env.TAURI_UPDATER_PUBKEY.trim() } } });
  return { problems: [], args: ["build", "--config", config, ...args.slice(1)] };
}

/** The macOS signing and notarization variables the bundler reads. */
export const APPLE_ENV = [
  "APPLE_CERTIFICATE",
  "APPLE_CERTIFICATE_PASSWORD",
  "APPLE_SIGNING_IDENTITY",
  "APPLE_ID",
  "APPLE_PASSWORD",
  "APPLE_TEAM_ID",
];

/**
 * Removes empty APPLE_* variables from `env` (in place) and, with no
 * certificate, asks for an ad-hoc signature ("-"). Returns the notes to log.
 */
export function appleEnv(env) {
  const notes = [];
  for (const name of APPLE_ENV) if (name in env && !env[name]?.trim()) delete env[name];
  if (!env.APPLE_CERTIFICATE && !env.APPLE_SIGNING_IDENTITY) {
    env.APPLE_SIGNING_IDENTITY = "-";
    notes.push("no APPLE_CERTIFICATE: the macOS app is ad-hoc signed and not notarized");
  }
  return notes;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { problems, args } = releaseArgs(process.argv.slice(2), process.env);
  if (args?.[0] === "build" && process.platform === "darwin") for (const note of appleEnv(process.env)) console.warn(note);
  for (const problem of problems) console.error(problem);
  if (problems.length) process.exit(1);
  const cli = (await import("@tauri-apps/cli")).default;
  cli.run(args, "tauri").catch((err) => {
    cli.logError(err.message);
    process.exit(1);
  });
}
