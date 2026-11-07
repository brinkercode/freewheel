#!/usr/bin/env node
// The portable release packages and their manifest (release.yml, and the
// Windows update e2e for its version B):
//
//   node scripts/portable-release.mjs package --os windows --version 0.1.1 --out <dir> [--exe target/release/freewheel.exe]
//   node scripts/portable-release.mjs package --os macos --version 0.1.1 --out <dir> --app <path/to/freewheel.app>
//     -> <dir>/freewheel-0.1.1-windows-x64.zip (freewheel.exe + freewheel-simulate.cmd)
//        <dir>/freewheel-0.1.1-macos-universal.zip (freewheel.app, as ditto keeps it)
//   node scripts/portable-release.mjs manifest --version 0.1.1 --notes-file notes.md --dir <dir> --base-url <url> [--only windows]
//     -> latest.json on stdout: each platform's package url and its signature
//        (<package>.sig, made by `tauri signer sign --app-version <version>`);
//        --only limits it to some packages (the Windows update e2e)
//
// The in-app updater (src-tauri/src/selfupdate.rs) reads latest.json from
// releases/latest/download/latest.json, picks platforms[<os>-<arch>],
// downloads the zip, verifies the signature and swaps the app in place.
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** The package each OS ships. */
export const PACKAGES = {
  windows: (version) => `freewheel-${version}-windows-x64.zip`,
  macos: (version) => `freewheel-${version}-macos-universal.zip`,
};

/** The manifest's platform keys (selfupdate.rs platform_key) and the package serving each. */
export const PLATFORMS = { "windows-x86_64": "windows", "darwin-aarch64": "macos", "darwin-x86_64": "macos" };

/** What the Windows package holds, next to each other at its root. */
export const WINDOWS_FILES = ["freewheel.exe", "freewheel-simulate.cmd"];

const VERSION = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;

/** latest.json for `version`: `signatures` maps a package file name to its .sig content. */
export function manifest({ version, notes, baseUrl, signatures, only = Object.keys(PACKAGES), pubDate = new Date().toISOString() }) {
  if (!VERSION.test(version ?? "")) throw new Error(`not a version: ${version}`);
  const platforms = {};
  for (const [key, os] of Object.entries(PLATFORMS)) {
    if (!only.includes(os)) continue;
    const file = PACKAGES[os](version);
    const signature = signatures[file]?.trim();
    if (!signature) throw new Error(`no signature for ${file}`);
    platforms[key] = { url: `${baseUrl.replace(/\/$/, "")}/${file}`, signature };
  }
  return { version, notes: notes ?? "", pub_date: pubDate, platforms };
}

/** Parses `--name value` pairs. */
export function parseArgs(argv) {
  const [command, ...rest] = argv;
  const opts = { command };
  for (let i = 0; i < rest.length; i += 2) {
    if (!rest[i].startsWith("--")) throw new Error(`unexpected ${rest[i]}`);
    opts[rest[i].slice(2)] = rest[i + 1];
  }
  return opts;
}

/** Makes the OS's package in `out`; returns its path. */
export function makePackage({ os, version, out, exe = "target/release/freewheel.exe", app }) {
  if (!PACKAGES[os]) throw new Error(`unknown --os ${os}`);
  if (!VERSION.test(version ?? "")) throw new Error(`not a version: ${version}`);
  mkdirSync(out, { recursive: true });
  const zip = resolve(out, PACKAGES[os](version));
  rmSync(zip, { force: true });
  if (os === "windows") {
    const stage = mkdtempSync(join(tmpdir(), "freewheel-package-"));
    try {
      copyFileSync(exe, join(stage, "freewheel.exe"));
      copyFileSync(fileURLToPath(new URL("../src-tauri/windows/freewheel-simulate.cmd", import.meta.url)), join(stage, "freewheel-simulate.cmd"));
      // Windows' own bsdtar (not Git's GNU tar) writes zip with -a.
      const tar = join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe");
      execFileSync(tar, ["-a", "-c", "-f", zip, "-C", stage, ...WINDOWS_FILES], { stdio: "inherit" });
    } finally {
      rmSync(stage, { recursive: true, force: true });
    }
  } else {
    if (!app || basename(app) !== "freewheel.app" || !existsSync(app)) throw new Error(`--app must be an existing freewheel.app: ${app}`);
    // ditto keeps the bundle's modes, symlinks and signature; no resource forks.
    execFileSync("/usr/bin/ditto", ["-c", "-k", "--norsrc", "--keepParent", app, zip], { stdio: "inherit" });
  }
  return zip;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.command === "package") {
    console.log(makePackage(opts));
  } else if (opts.command === "manifest") {
    const signatures = {};
    for (const os of Object.keys(PACKAGES)) {
      const file = PACKAGES[os](opts.version);
      const sig = join(opts.dir, `${file}.sig`);
      if (existsSync(sig)) signatures[file] = readFileSync(sig, "utf8");
    }
    const notes = opts["notes-file"] ? readFileSync(opts["notes-file"], "utf8") : "";
    const only = opts.only ? opts.only.split(",") : undefined;
    console.log(JSON.stringify(manifest({ version: opts.version, notes, baseUrl: opts["base-url"], signatures, only }), null, 2));
  } else {
    console.error("usage: portable-release.mjs package|manifest --version X.Y.Z ...");
    process.exit(1);
  }
}
