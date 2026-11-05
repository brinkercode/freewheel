import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { checkLatestJson } from "./check-latest-json.mjs";
import { checkReleaseWorkflow } from "./check-workflow.mjs";
import { releaseArgs } from "./tauri-release.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const release = read("./fixtures/proposed-release.yml");
const conf = JSON.parse(read("../src-tauri/tauri.conf.json"));

test("release.yml passes the release lint", () => {
  expect(checkReleaseWorkflow(release)).toEqual([]);
});

test.each([
  ["an unpinned action", release.replace(/tauri-action@[0-9a-f]{40}/, "tauri-action@v0"), "not pinned to a 40-hex SHA"],
  ["a short SHA", release.replace(/checkout@[0-9a-f]{40}/, "checkout@11bd719"), "not pinned to a 40-hex SHA"],
  ["no release environment", release.replace("    environment: release\n", ""), "'environment: release'"],
  ["an APPLE_ secret in the job env", release.replace("    runs-on: ${{ matrix.os }}\n", "    runs-on: ${{ matrix.os }}\n    env:\n      APPLE_ID: ${{ secrets.APPLE_ID }}\n"), "outside the tauri-action step env"],
  ["a TAURI_SIGNING_ secret in another step", release.replace("      - run: npm ci\n", "      - run: npm ci\n        env:\n          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}\n"), "outside the tauri-action step env"],
  ["secrets: inherit", release.replace("    uses: ./.github/workflows/ci.yml\n", "    uses: ./.github/workflows/ci.yml\n    secrets: inherit\n"), "secrets: inherit"],
])("the release lint fails on %s", (_name, text, msg) => {
  expect(checkReleaseWorkflow(text).join("\n")).toContain(msg);
});

test("release.yml is tag-only and builds after the ci.yml matrix", () => {
  expect(release).toMatch(/^on:\n {2}push:\n {4}tags: \["v\*"\]\n\n/m);
  expect(release).not.toMatch(/pull_request|branches:|workflow_dispatch/);
  expect(release).toMatch(/^ {2}test:\n {4}uses: \.\/\.github\/workflows\/ci\.yml$/m);
  expect(release).toMatch(/^ {2}release:\n {4}needs: test$/m);
  expect(release.match(/- os: \S+/g)).toEqual(["- os: windows-2025", "- os: macos-15"]);
  expect(release).not.toMatch(/\bstable\b|dtolnay\/rust-toolchain|ubuntu|self-hosted/);
  expect(release).toContain("rustup toolchain install");
  expect(read("../rust-toolchain.toml")).toMatch(/channel = "1\.99\.1"[\s\S]*"aarch64-apple-darwin", "x86_64-apple-darwin"/);
});

test("release.yml makes draft releases with latest.json and a universal macOS build", () => {
  expect(release).toContain("releaseDraft: true");
  expect(release).toContain("uploadUpdaterJson: true");
  expect(release).toMatch(/- os: macos-15\n\s+args: "--target universal-apple-darwin -- --locked"/);
  expect(release).toContain("run: npm ci");
  expect(release).toContain("node scripts/check-latest-json.mjs latest.json");
  expect(release).toContain("--json isDraft --jq .isDraft)\" = true");
});

test("bundles: per-user unsigned NSIS on Windows, macOS 15.0 dmg, no MSI", () => {
  expect(conf.bundle.targets).toEqual(["nsis", "app", "dmg"]);
  expect(conf.bundle.createUpdaterArtifacts).toBe(true);
  expect(conf.bundle.macOS.minimumSystemVersion).toBe("15.0");
  expect(conf.bundle.windows).toEqual({ nsis: { installMode: "currentUser" } });
  expect(release).not.toMatch(/WINDOWS_CERTIFICATE|certificateThumbprint|signCommand|AZURE_/);
});

test("[T3][T7] the release build injects the updater pubkey from vars through the tauri-action step", () => {
  expect(release).toContain("tauriScript: node scripts/tauri-release.mjs");
  expect(release).toMatch(/^ {10}TAURI_UPDATER_PUBKEY: \$\{\{ vars\.TAURI_UPDATER_PUBKEY \}\}$/m);
  expect(release).not.toMatch(/secrets\.TAURI_UPDATER_PUBKEY/);
  expect(conf.plugins.updater.pubkey ?? "").toBe("");
});

const releaseEnv = {
  TAURI_UPDATER_PUBKEY: "public-key-from-vars",
  TAURI_SIGNING_PRIVATE_KEY: "x",
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "y",
};

test("tauri build gets a --config override of plugins.updater.pubkey", () => {
  const { problems, args } = releaseArgs(["build", "--target", "universal-apple-darwin"], releaseEnv);
  expect(problems).toEqual([]);
  expect(args.slice(0, 2)).toEqual(["build", "--config"]);
  expect(JSON.parse(args[2])).toEqual({ plugins: { updater: { pubkey: "public-key-from-vars" } } });
  expect(args.slice(3)).toEqual(["--target", "universal-apple-darwin"]);
});

test.each(Object.keys(releaseEnv))("the release build fails naming a missing %s", (name) => {
  const { problems, args } = releaseArgs(["build"], { ...releaseEnv, [name]: "" });
  expect(args).toBeNull();
  expect(problems).toHaveLength(1);
  expect(problems[0]).toMatch(new RegExp(`^release build needs ${name}:`));
});

test("other Tauri commands pass through without the release env", () => {
  expect(releaseArgs(["info"], {})).toEqual({ problems: [], args: ["info"] });
});

const platform = { url: "https://github.com/x/freewheel/releases/download/v1/a", signature: "sig" };
const latest = {
  version: "0.1.0",
  platforms: { "windows-x86_64": platform, "darwin-aarch64": platform, "darwin-x86_64": platform },
};

test("latest.json with every platform passes", () => {
  expect(checkLatestJson(latest)).toEqual([]);
});

test.each([
  ["windows-x86_64"],
  ["darwin-aarch64"],
  ["darwin-x86_64"],
])("latest.json without %s fails", (key) => {
  const platforms = { ...latest.platforms };
  delete platforms[key];
  expect(checkLatestJson({ ...latest, platforms })).toEqual([`missing platform '${key}'`]);
});

test("latest.json needs an https url and a signature", () => {
  const platforms = { ...latest.platforms, "darwin-x86_64": { url: "http://x/a", signature: "" } };
  expect(checkLatestJson({ ...latest, platforms })).toEqual([
    "'darwin-x86_64' has no https url",
    "'darwin-x86_64' has no signature",
  ]);
});

test("the repo holds no signing certificate or private key", () => {
  const root = new URL("..", import.meta.url);
  const files = execFileSync("git", ["ls-files", "--cached", "--others", "--exclude-standard"], { cwd: root, encoding: "utf8" })
    .split("\n")
    .filter(Boolean);
  expect(files.filter((f) => /\.(p12|pfx|pem|key|cer|p8)$/i.test(f))).toEqual([]);
  const leaks = files.filter((f) => {
    let text;
    try {
      text = readFileSync(new URL(f, root), "utf8");
    } catch {
      return false;
    }
    return /-----BEGIN [A-Z ]*PRIVATE KEY-----|rsign encrypted secret[ ]key|secret[ ]key: /.test(text);
  });
  expect(leaks).toEqual([]);
});
