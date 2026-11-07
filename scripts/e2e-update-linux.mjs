#!/usr/bin/env node
// The AppImage self-update, end to end, on a hosted Linux runner, without a
// published release (.github/workflows/e2e-linux.yml, job update): version
// A's AppImage (kept in a folder, as a user keeps it) and version B's are
// e2e builds with a throwaway test pubkey baked in. This script serves B's
// AppImage, signature and latest.json (scripts/update-server.mjs) and runs
// A's headless update (`--update-check-and-install`, the updater pointed at
// that server through FREEWHEEL_E2E_UPDATE_URL, e2e builds only):
//   the server fails -> exit 1, nothing changed
//   nothing newer -> exit 2, nothing changed
//   a tampered B (one byte flipped) -> exit 1, A's file, the folder and the
//     temp dir untouched
//   B -> exit 0: the same path now holds B, byte for byte, mode 755
//   B's own run -> exit 2 (up to date), and it removed <name>.old and the
//     temp download
// and the user's config.json (keys and a second page) is byte-identical
// throughout. Then B starts from the same path (`--simulate --smoke`).
//
//   node scripts/e2e-update-linux.mjs --appimage <folder>/freewheel.AppImage --manifest <B latest.json> --package <B AppImage>
//        --server-port 8719 [--out e2e-update-shots]
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { startUpdateServer } from "./update-server.mjs";

/** The e2e builds' manifest override (src-tauri/src/selfupdate.rs, `e2e` feature only). */
export const UPDATE_URL_ENV = "FREEWHEEL_E2E_UPDATE_URL";

/** `--update-check-and-install`'s exit codes (selfupdate.rs run_headless). */
export const EXIT = { installed: 0, failed: 1, current: 2, notConfigured: 3 };

export function parseArgs(argv) {
  const opts = {
    appimage: null,
    manifest: null,
    package: null,
    "server-port": "8719",
    out: "e2e-update-shots",
  };
  for (let i = 0; i < argv.length; i++) {
    const name = argv[i].replace(/^--/, "");
    if (!(name in opts)) throw new Error(`unknown option ${argv[i]}`);
    opts[name] = argv[++i];
  }
  for (const need of ["appimage", "manifest", "package"])
    if (!opts[need]) throw new Error(`--${need} is required`);
  return opts;
}

/** Where freewheel keeps its config on Linux (directories::ProjectDirs). */
export const configFile = (env = process.env, home = os.homedir()) =>
  path.posix.join(
    env.XDG_CONFIG_HOME || path.posix.join(home, ".config"),
    "freewheel",
    "config.json",
  );

/** The updater's temp dir (selfupdate.rs temp_dir: $TMPDIR, else /tmp). */
export const updateTemp = (env = process.env) =>
  path.posix.join(env.TMPDIR || "/tmp", "freewheel-update");

const sha = (file) =>
  createHash("sha256").update(readFileSync(file)).digest("hex");

function main() {
  const opts = parseArgs(process.argv.slice(2));
  const out = path.resolve(opts.out);
  mkdirSync(out, { recursive: true });
  const log = [];
  const t0 = Date.now();
  const say = (line) => (
    console.log(`[${((Date.now() - t0) / 1000).toFixed(1)}s] ${line}`),
    log.push(line)
  );
  const app = path.resolve(opts.appimage);
  const folder = path.dirname(app);
  const files = () => readdirSync(folder).sort().join(", ");
  return { opts, out, log, say, app, folder, files };
}

async function run() {
  const { opts, out, log, say, app, folder, files } = main();
  let server = null;
  try {
    const manifest = JSON.parse(
      readFileSync(path.resolve(opts.manifest), "utf8"),
    );
    const pkg = path.resolve(opts.package);
    server = await startUpdateServer({
      manifest,
      package: pkg,
      port: Number(opts["server-port"]),
      mode: "fail",
      downloadMs: 2000,
    });
    say(
      `update server on ${server.url} offering ${manifest.version} (${path.basename(pkg)})`,
    );
    const env = {
      ...process.env,
      [UPDATE_URL_ENV]: `${server.url}/latest.json`,
    };
    const update = (expect, what) => {
      say(`step: ${what} (server: ${server.mode()})`);
      const r = spawnSync(app, ["--update-check-and-install"], {
        env,
        cwd: folder,
        encoding: "utf8",
        timeout: 120000,
      });
      say(`  exit ${r.status}${r.signal ? ` (signal ${r.signal})` : ""}`);
      if (r.status !== expect)
        throw new Error(
          `${what}: exit ${r.status}, expected ${expect}\n${(r.stderr ?? "").slice(-2000)}`,
        );
    };

    // The user's setup: keys and a second page (a valid config, checked by
    // freewheel-core's tests), byte for byte through the whole update.
    const config = configFile();
    mkdirSync(path.dirname(config), { recursive: true });
    copyFileSync(
      path.join(
        path.dirname(fileURLToPath(import.meta.url)),
        "fixtures",
        "update-e2e-config.json",
      ),
      config,
    );
    const configHash = sha(config);
    const a = { hash: sha(app), files: files() };
    if (a.hash === sha(pkg)) throw new Error("A and B are the same file");
    say(
      `A ${app} sha256 ${a.hash}; folder: ${a.files}; config sha256 ${configHash}`,
    );
    const untouched = (what) => {
      if (sha(app) !== a.hash) throw new Error(`${what}: the AppImage changed`);
      if (files() !== a.files)
        throw new Error(`${what}: the folder changed: ${files()}`);
      if (existsSync(updateTemp()))
        throw new Error(`${what}: ${updateTemp()} was kept`);
      if (sha(config) !== configHash)
        throw new Error(`${what}: config.json changed`);
      say(
        `  A untouched: same file, same folder (${files()}), no download kept, same config`,
      );
    };

    update(EXIT.failed, "the server fails");
    untouched("after a failed check");
    server.setMode("none");
    update(EXIT.current, "nothing newer");
    untouched("when up to date");
    server.setMode("tampered");
    update(EXIT.failed, `a tampered ${manifest.version}`);
    untouched("after the tampered package");

    server.setMode("offer");
    update(EXIT.installed, `${manifest.version} published`);
    say(`  folder after the swap: ${files()}`);
    if (sha(app) !== sha(pkg)) throw new Error(`${app} is not B's AppImage`);
    if ((statSync(app).mode & 0o777) !== 0o755)
      throw new Error(
        `${app} has mode ${(statSync(app).mode & 0o777).toString(8)}, not 755`,
      );
    say(`  ${app} is B (sha256 ${sha(app)}), mode 755`);

    update(EXIT.current, `B, checking again (${manifest.version} is current)`);
    if (existsSync(`${app}.old`)) throw new Error(`${app}.old is still there`);
    if (existsSync(updateTemp()))
      throw new Error(`${updateTemp()} is still there`);
    if (files() !== a.files)
      throw new Error(`the folder is not as before: ${files()}`);
    if (sha(config) !== configHash) throw new Error("config.json changed");
    say(
      `  no .old, no temp download; folder: ${files()}; config identical (sha256 ${configHash})`,
    );

    say("step: B starts from the same path (--simulate --smoke)");
    const smoke = spawnSync(app, ["--simulate", "--smoke"], {
      env,
      cwd: folder,
      encoding: "utf8",
      timeout: 120000,
    });
    say(`  exit ${smoke.status}`);
    if (smoke.status !== 0)
      throw new Error(
        `B's smoke run failed (${smoke.status})\n${(smoke.stderr ?? "").slice(-2000)}`,
      );
    if (sha(config) !== configHash)
      throw new Error("config.json changed when B started");
    say(`  server saw: ${server.hits.join(" | ")}`);
    say("update e2e: PASS");
  } catch (e) {
    say(`update e2e: FAIL: ${e.message}`);
    if (server) say(`  server saw: ${server.hits.join(" | ")}`);
    say(`  folder: ${existsSync(folder) ? files() : "(gone)"}`);
    process.exitCode = 1;
  } finally {
    writeFileSync(path.join(out, "e2e-update-log.txt"), log.join("\n") + "\n");
    await server?.close();
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await run();
