import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import {
  EXIT,
  configFile,
  parseArgs,
  updateTemp,
} from "./e2e-update-linux.mjs";

const read = (p) => readFileSync(new URL(p, import.meta.url), "utf8");

test("the Linux update e2e takes A's AppImage, B's manifest and package", () => {
  const opts = parseArgs([
    "--appimage",
    "/a/freewheel.AppImage",
    "--manifest",
    "m.json",
    "--package",
    "b.AppImage",
  ]);
  expect(opts).toEqual({
    appimage: "/a/freewheel.AppImage",
    manifest: "m.json",
    package: "b.AppImage",
    "server-port": "8719",
    out: "e2e-update-shots",
  });
  expect(() => parseArgs(["--appimage", "x"])).toThrow(/--manifest/);
  expect(() => parseArgs(["--nope", "1"])).toThrow(/unknown/);
});

test("it checks the places freewheel uses on Linux and the headless run's exit codes", () => {
  expect(configFile({}, "/home/a")).toBe(
    "/home/a/.config/freewheel/config.json",
  );
  expect(configFile({ XDG_CONFIG_HOME: "/x" }, "/home/a")).toBe(
    "/x/freewheel/config.json",
  );
  expect(updateTemp({})).toBe("/tmp/freewheel-update");
  expect(updateTemp({ TMPDIR: "/t" })).toBe("/t/freewheel-update");
  const rust = read("../src-tauri/src/selfupdate.rs");
  expect(rust).toContain(
    "Exit code: 0 installed, 2 up to date, 3 not configured, 1 failed.",
  );
  expect(EXIT).toEqual({
    installed: 0,
    failed: 1,
    current: 2,
    notConfigured: 3,
  });
  expect(rust).toContain('pub const TEMP_DIR_NAME: &str = "freewheel-update";');
});

test("the update job builds A and B as AppImages, signs B with a throwaway key and updates in place", () => {
  const wf = read("../.github/workflows/e2e-linux.yml");
  expect(wf).toContain(
    "name: update e2e (ubuntu-24.04, AppImage, local release server)",
  );
  expect(wf).toContain(
    'node "$TAURI_CLI" build --ci --features e2e --bundles appimage',
  );
  expect(wf).toContain(
    'signer sign -f "$E2E_UPDATER_KEY" -p "" --app-version "$TO_VERSION" "$image"',
  );
  expect(wf).toContain("--only linux");
  expect(wf).toContain("node scripts/e2e-update-linux.mjs --appimage");
  expect(wf).not.toMatch(/secrets\./);
  // Required checks run on every PR.
  expect(wf).not.toMatch(/^\s+paths-ignore:/m);
});
