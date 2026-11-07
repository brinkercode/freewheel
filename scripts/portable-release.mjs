#!/usr/bin/env node
// The release files and their manifest (release.yml, and the Windows update
// e2e for its version B). Each is what the user downloads and runs: no zip.
//
//   node scripts/portable-release.mjs package --os windows --version 0.1.1 --out <dir> [--exe target/release/freewheel.exe]
//   node scripts/portable-release.mjs package --os macos --version 0.1.1 --out <dir> --app <path/to/freewheel.app>
//     -> <dir>/freewheel-0.1.1-windows-x64.exe (the portable exe itself)
//        <dir>/freewheel-0.1.1-macos-universal.dmg (freewheel.app and an
//        Applications link to drag it onto; a .app is a folder, so it needs
//        a container: open the .dmg and run it, or drag it to Applications)
//   node scripts/portable-release.mjs manifest --version 0.1.1 --notes-file notes.md --dir <dir> --base-url <url> [--only windows]
//     -> latest.json on stdout: each platform's file url and its signature
//        (<file>.sig, made by `tauri signer sign --app-version <version>`);
//        --only limits it to some files (the Windows update e2e)
//
// The in-app updater (src-tauri/src/selfupdate.rs) reads latest.json from
// releases/latest/download/latest.json, picks platforms[<os>-<arch>],
// downloads that file, verifies its signature and swaps the app in place
// (Windows: the exe itself; macOS: freewheel.app copied out of the .dmg).
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** The release file each OS ships. */
export const PACKAGES = {
  windows: (version) => `freewheel-${version}-windows-x64.exe`,
  macos: (version) => `freewheel-${version}-macos-universal.dmg`,
};

/** The manifest's platform keys (selfupdate.rs platform_key) and the release file serving each. */
export const PLATFORMS = { "windows-x86_64": "windows", "darwin-aarch64": "macos", "darwin-x86_64": "macos" };

/** `hdiutil create` for the .dmg (selfupdate.rs tests make theirs the same way). */
export const DMG_CREATE = ["create", "-volname", "freewheel", "-fs", "HFS+", "-format", "UDZO", "-ov", "-srcfolder"];

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

/** Makes the OS's release file in `out`; returns its path. */
export function makePackage({ os, version, out, exe = "target/release/freewheel.exe", app }) {
  if (!PACKAGES[os]) throw new Error(`unknown --os ${os}`);
  if (!VERSION.test(version ?? "")) throw new Error(`not a version: ${version}`);
  mkdirSync(out, { recursive: true });
  const file = resolve(out, PACKAGES[os](version));
  rmSync(file, { force: true });
  if (os === "windows") {
    // The exe is the release file: download it and run it.
    if (!existsSync(exe)) throw new Error(`--exe must be an existing freewheel.exe: ${exe}`);
    copyFileSync(exe, file);
  } else {
    if (!app || basename(app) !== "freewheel.app" || !existsSync(app)) throw new Error(`--app must be an existing freewheel.app: ${app}`);
    const stage = mkdtempSync(join(tmpdir(), "freewheel-dmg-"));
    try {
      // ditto keeps the bundle's modes, symlinks and signature.
      execFileSync("/usr/bin/ditto", [app, join(stage, "freewheel.app")], { stdio: "inherit" });
      symlinkSync("/Applications", join(stage, "Applications"));
      // hdiutil on hosted runners now and then fails with "Resource busy": retry.
      for (let attempt = 1; ; attempt++) {
        try {
          execFileSync("/usr/bin/hdiutil", [...DMG_CREATE, stage, file], { stdio: "inherit" });
          break;
        } catch (e) {
          if (attempt >= 4) throw e;
          execFileSync("/bin/sleep", [String(attempt * 3)]);
        }
      }
    } finally {
      rmSync(stage, { recursive: true, force: true });
    }
  }
  return file;
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
