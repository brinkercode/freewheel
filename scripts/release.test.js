import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { checkLatestJson } from "./check-latest-json.mjs";
import { checkReleaseWorkflow } from "./check-workflow.mjs";
import { appleEnv, releaseArgs } from "./tauri-release.mjs";
import { pickReleases, versionOf } from "./update-e2e.mjs";
import { latestProblems, signatureProblem, verifyLatest } from "./verify-update-sig.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const release = read("../.github/workflows/release.yml");
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

test("release.yml runs on v* tags (pushed, or dispatched on the tag by release-please) after the ci.yml matrix", () => {
  expect(release).toMatch(/^on:\n {2}push:\n {4}tags: \["v\*"\]\n {2}workflow_dispatch:\n\n/m);
  expect(release).not.toMatch(/pull_request|branches:/);
  expect(release).toMatch(/^ {2}test:\n {4}if: startsWith\(github\.ref, 'refs\/tags\/v'\)\n {4}uses: \.\/\.github\/workflows\/ci\.yml$/m);
  expect(release).toMatch(/^ {2}notes:\n {4}if: startsWith\(github\.ref, 'refs\/tags\/v'\)$/m);
  expect(release).toMatch(/^ {2}release:\n {4}needs: \[test, notes\]$/m);
  expect(release.match(/- os: \S+/g)).toEqual(["- os: windows-2025", "- os: macos-15"]);
  expect(release).not.toMatch(/\bstable\b|dtolnay\/rust-toolchain|self-hosted/);
  expect(release).toContain("rustup toolchain install");
  expect(read("../rust-toolchain.toml")).toMatch(/channel = "1\.99\.1"[\s\S]*"aarch64-apple-darwin", "x86_64-apple-darwin"/);
  // the tag must be the version release-please wrote into the version files
  expect(release).toContain('test "v$version" = "$GITHUB_REF_NAME"');
});

test("release.yml publishes the release with release-please's notes, in latest.json too, and verifies it", () => {
  expect(release).toContain("releaseDraft: false");
  expect(release).not.toContain("releaseDraft: true");
  expect(release).toContain("releaseBody: ${{ needs.notes.outputs.body }}");
  expect(release).toContain("gh release view \"$GITHUB_REF_NAME\" --repo \"$GITHUB_REPOSITORY\" --json body --jq .body");
  expect(release).toContain("uploadUpdaterJson: true");
  expect(release).toMatch(/- os: macos-15\n\s+args: "--target universal-apple-darwin -- --locked"/);
  expect(release).toContain("run: npm ci");
  expect(release).toContain("node scripts/check-latest-json.mjs latest.json");
  expect(release).toContain('node scripts/verify-update-sig.mjs latest.json --version "$GITHUB_REF_NAME"');
  expect(release).toContain("--json isDraft --jq .isDraft)\" = false");
  expect(release).toContain("TAURI_UPDATER_PUBKEY: ${{ vars.TAURI_UPDATER_PUBKEY }}");
  expect(release).toMatch(/^ {2}update-e2e:\n {4}needs: latest-json\n {4}uses: \.\/\.github\/workflows\/update-e2e\.yml$/m);
});

test("bundles: per-user unsigned NSIS on Windows, macOS 15.0 dmg, no MSI", () => {
  expect(conf.bundle.targets).toEqual(["nsis", "app", "dmg"]);
  expect(conf.bundle.createUpdaterArtifacts).toBe(true);
  expect(conf.bundle.macOS.minimumSystemVersion).toBe("15.0");
  expect(conf.bundle.windows).toEqual({
    nsis: { installMode: "currentUser" },
    webviewInstallMode: { type: "downloadBootstrapper" },
  });
  expect(release).not.toMatch(/WINDOWS_CERTIFICATE|certificateThumbprint|signCommand|AZURE_/);
});

test("[T3][T7] the release build's tauriScript goes through the pubkey override, and no pubkey is committed", () => {
  expect(release).toContain("tauriScript: npm run tauri");
  expect(release.match(/vars\.TAURI_UPDATER_PUBKEY/g)).toHaveLength(2);
  const pkg = JSON.parse(read("../package.json"));
  expect(pkg.scripts.tauri).toBe("node scripts/tauri-release.mjs");
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

test("without the Apple secrets the macOS build drops the empty APPLE_* variables and is ad-hoc signed", () => {
  const env = { APPLE_CERTIFICATE: "", APPLE_CERTIFICATE_PASSWORD: " ", APPLE_SIGNING_IDENTITY: "", APPLE_ID: "", APPLE_PASSWORD: "", APPLE_TEAM_ID: "", PATH: "/bin" };
  expect(appleEnv(env)).toEqual(["no APPLE_CERTIFICATE: the macOS app is ad-hoc signed and not notarized"]);
  expect(env).toEqual({ APPLE_SIGNING_IDENTITY: "-", PATH: "/bin" });
  const set = { APPLE_CERTIFICATE: "cert", APPLE_CERTIFICATE_PASSWORD: "pw", APPLE_SIGNING_IDENTITY: "Developer ID", APPLE_ID: "a@b" };
  expect(appleEnv(set)).toEqual([]);
  expect(set).toEqual({ APPLE_CERTIFICATE: "cert", APPLE_CERTIFICATE_PASSWORD: "pw", APPLE_SIGNING_IDENTITY: "Developer ID", APPLE_ID: "a@b" });
});

// A fixture signed by a throwaway key (only its pubkey is committed), as `tauri signer sign` signs updates.
const fixture = (name) => readFileSync(new URL(`./fixtures/${name}`, import.meta.url));
const data = fixture("update-fixture.bin");
const signature = fixture("update-fixture.bin.sig").toString("utf8");
const pubkey = fixture("update-fixture.pub").toString("utf8");

test("[T3] an update file verifies against the updater pubkey", () => {
  expect(signatureProblem(data, signature, pubkey)).toBeNull();
});

test("[T3] a tampered file, a tampered .sig or another key is rejected", () => {
  const tampered = Buffer.from(data);
  tampered[0] ^= 1;
  expect(signatureProblem(tampered, signature, pubkey)).toBe("bad signature");
  const text = Buffer.from(signature, "base64").toString("utf8").split("\n");
  // one character of the signature line changed
  text[1] = (text[1][20] === "A" ? text[1].slice(0, 20) + "B" : text[1].slice(0, 20) + "A") + text[1].slice(21);
  expect(signatureProblem(data, Buffer.from(text.join("\n")).toString("base64"), pubkey)).toMatch(/bad signature|another key/);
  const other = Buffer.from("untrusted comment: minisign public key: 0\nRWQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n").toString("base64");
  expect(signatureProblem(data, signature, other)).toBe("signed by another key");
  expect(signatureProblem(data, "bm90IGEgc2lnbmF0dXJl", pubkey)).toBe("not a minisign signature");
});

test("latest.json must carry the tag's version and the release notes", () => {
  expect(latestProblems({ ...latest, notes: "### Bug Fixes" }, { version: "v0.1.0" })).toEqual([]);
  expect(latestProblems({ ...latest, notes: "" }, { version: "v0.1.1" })).toEqual([
    "version is '0.1.0', not '0.1.1'",
    "has no release notes",
  ]);
});

test("each platform's download is fetched once and checked against its signature", async () => {
  const url = "https://github.com/brinkercode/freewheel/releases/download/v0.1.0/freewheel.app.tar.gz";
  const json = {
    platforms: {
      "darwin-aarch64": { url, signature },
      "darwin-x86_64": { url, signature },
      "windows-x86_64": { url: `${url}.exe`, signature },
    },
  };
  const fetched = [];
  const fetchBytes = async (u) => {
    fetched.push(u);
    return u.endsWith(".exe") ? Buffer.from("something else") : data;
  };
  expect(await verifyLatest(json, pubkey, fetchBytes)).toEqual(["'windows-x86_64': bad signature"]);
  expect(fetched).toEqual([url, `${url}.exe`]);
});

const releasePlease = read("../.github/workflows/release-please.yml");
const rpConfig = JSON.parse(read("../release-please-config.json"));
const rpManifest = JSON.parse(read("../.release-please-manifest.json"));

test("release-please bumps every version file and starts from the version they hold", () => {
  const pkg = rpConfig.packages["."];
  expect(rpConfig["release-type"]).toBe("node");
  expect(rpConfig["include-v-in-tag"]).toBe(true);
  expect(rpConfig["include-component-in-tag"]).toBe(false);
  expect(pkg["extra-files"].map((f) => f.path)).toEqual([
    "src-tauri/tauri.conf.json",
    "src-tauri/Cargo.toml",
    "crates/freewheel-core/Cargo.toml",
    "Cargo.lock",
  ]);
  const version = rpManifest["."];
  expect(conf.version).toBe(version);
  expect(JSON.parse(read("../package.json")).version).toBe(version);
  expect(read("../src-tauri/Cargo.toml")).toContain(`\nversion = "${version}"\n`);
  expect(read("../crates/freewheel-core/Cargo.toml")).toContain(`\nversion = "${version}"\n`);
  for (const name of ["freewheel", "freewheel-core"]) {
    expect(read("../Cargo.lock")).toContain(`[[package]]\nname = "${name}"\nversion = "${version}"\n`);
  }
});

test("release-please runs on main, pinned, and dispatches the release PR's CI and the tag's release build", () => {
  expect(releasePlease).toMatch(/^on:\n {2}push:\n {4}branches: \[main\]\n {2}workflow_dispatch:\n/m);
  expect(releasePlease).toMatch(/^permissions:\n {2}contents: read$/m);
  expect(releasePlease).toContain("uses: googleapis/release-please-action@45996ed1f6d02564a971a2fa1b5860e934307cf7 # v5.0.0");
  expect(releasePlease).not.toMatch(/secrets\.|pull_request_target|secrets: inherit/);
  expect(releasePlease).toContain("for wf in ci.yml e2e-windows.yml; do");
  expect(releasePlease).toContain('gh workflow run "$wf" --repo "$GITHUB_REPOSITORY" --ref "$branch"');
  expect(releasePlease).toContain('gh workflow run release.yml --repo "$GITHUB_REPOSITORY" --ref "$TAG"');
  expect(releasePlease).toContain("if: steps.release.outputs.release_created == 'true'");
});

const e2e = read("../.github/workflows/update-e2e.yml");

test("update-e2e installs the previous release on Windows and updates it with the installed app's updater", () => {
  expect(e2e).toMatch(/^on:\n {2}workflow_dispatch:/m);
  expect(e2e).toMatch(/^ {2}workflow_call:$/m);
  expect(e2e).toMatch(/^permissions:\n {2}contents: read$/m);
  expect(e2e).not.toMatch(/secrets\.|: write\b/);
  expect(e2e).toContain("runs-on: windows-2025");
  expect(e2e).toContain("--pattern '*-setup.exe'");
  expect(e2e).toContain("-ArgumentList '/S' -Wait -PassThru");
  expect(e2e.match(/-ArgumentList '--update-check-and-install'/g)).toHaveLength(2);
  expect(e2e).toContain('if ($installed -ne $env:TO_VERSION)');
  expect(e2e).toContain("node scripts/check-windows-exe.mjs $exe");
  expect(e2e).toContain("if ($run.ExitCode -ne 2)");
});

test("update-e2e updates from the release before the newest one (or the one asked for)", () => {
  const tags = ["v0.1.0", "v0.1.1", "v0.2.0", "v0.10.0", "nightly"];
  expect(pickReleases(tags)).toEqual({ from: "v0.2.0", to: "v0.10.0" });
  expect(pickReleases(tags, "v0.1.0")).toEqual({ from: "v0.1.0", to: "v0.10.0" });
  expect(pickReleases(["v0.1.0"])).toBeNull();
  expect(pickReleases([])).toBeNull();
  expect(pickReleases(tags, "v0.10.0")).toBeNull();
  expect(versionOf("v0.1.1")).toBe("0.1.1");
});
