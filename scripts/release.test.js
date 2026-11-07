import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { expect, test } from "vitest";
import { checkLatestJson } from "./check-latest-json.mjs";
import { checkReleaseWorkflow } from "./check-workflow.mjs";
import { DMG_CREATE, PACKAGES, PLATFORMS as SERVED, makePackage, manifest, parseArgs as packageArgs } from "./portable-release.mjs";
import { binaryProblems } from "./check-release-binary.mjs";
import { appleEnv, releaseArgs } from "./tauri-release.mjs";
import { pickReleases, versionOf } from "./update-e2e.mjs";
import { boundVersion, latestProblems, signatureProblem, verifyLatest } from "./verify-update-sig.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const release = read("../.github/workflows/release.yml");
const conf = JSON.parse(read("../src-tauri/tauri.conf.json"));

test("release.yml passes the release lint", () => {
  expect(checkReleaseWorkflow(release)).toEqual([]);
});

test.each([
  ["an unpinned action", release.replace(/checkout@[0-9a-f]{40}/, "checkout@v4"), "not pinned to a 40-hex SHA"],
  ["a short SHA", release.replace(/checkout@[0-9a-f]{40}/, "checkout@11bd719"), "not pinned to a 40-hex SHA"],
  ["no release environment on the build", release.replace("  build:\n    needs: [test, notes]\n    environment: release\n", "  build:\n    needs: [test, notes]\n"), "'Build freewheel.app (macOS)' does not declare 'environment: release'"],
  ["no release environment on the signing", release.replace("  publish:\n    needs: [build, notes]\n    environment: release\n", "  publish:\n    needs: [build, notes]\n"), "'Sign the packages' does not declare 'environment: release'"],
  ["an APPLE_ secret in the job env", release.replace("    runs-on: ${{ matrix.os }}\n", "    runs-on: ${{ matrix.os }}\n    env:\n      APPLE_ID: ${{ secrets.APPLE_ID }}\n"), "APPLE_* outside the 'Build freewheel.app (macOS)' step env"],
  ["a TAURI_SIGNING_ secret in another step", release.replace("      - run: npm ci\n", "      - run: npm ci\n        env:\n          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}\n"), "TAURI_SIGNING_* outside the 'Sign the packages' step env"],
  ["the signing key in the build", release.replace("          TAURI_UPDATER_PUBKEY: ${{ vars.TAURI_UPDATER_PUBKEY }}\n        run: npm run tauri -- build --no-bundle", "          TAURI_UPDATER_PUBKEY: ${{ vars.TAURI_UPDATER_PUBKEY }}\n          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}\n        run: npm run tauri -- build --no-bundle"), "TAURI_SIGNING_* outside"],
  ["secrets: inherit", release.replace("    uses: ./.github/workflows/ci.yml\n", "    uses: ./.github/workflows/ci.yml\n    secrets: inherit\n"), "secrets: inherit"],
])("the release lint fails on %s", (_name, text, msg) => {
  expect(checkReleaseWorkflow(text).join("\n")).toContain(msg);
});

test("release.yml runs on v* tags (pushed, or dispatched on the tag by release-please) after the ci.yml matrix", () => {
  expect(release).toMatch(/^on:\n {2}push:\n {4}tags: \["v\*"\]\n {2}workflow_dispatch:\n\n/m);
  expect(release).not.toMatch(/pull_request|branches:/);
  expect(release).toMatch(/^ {2}test:\n {4}if: startsWith\(github\.ref, 'refs\/tags\/v'\)\n {4}uses: \.\/\.github\/workflows\/ci\.yml$/m);
  expect(release).toMatch(/^ {2}notes:\n {4}if: startsWith\(github\.ref, 'refs\/tags\/v'\)$/m);
  expect(release).toMatch(/^ {2}build:\n {4}needs: \[test, notes\]$/m);
  expect(release).toMatch(/^ {2}publish:\n {4}needs: \[build, notes\]$/m);
  expect(release.match(/- os: \S+/g)).toEqual(["- os: windows-2025", "- os: macos-15", "- os: ubuntu-22.04"]);
  expect(release).not.toMatch(/\bstable\b|dtolnay\/rust-toolchain|self-hosted/);
  expect(release).toContain("rustup toolchain install");
  expect(read("../rust-toolchain.toml")).toMatch(/channel = "1\.99\.1"[\s\S]*"aarch64-apple-darwin", "x86_64-apple-darwin"/);
  // the tag must be the version release-please wrote into the version files
  expect(release).toContain('test "v$version" = "$GITHUB_REF_NAME"');
});

test("release.yml uploads the portable packages, their signatures and latest.json with release-please's notes to the published release", () => {
  expect(release).not.toMatch(/tauri-action|releaseDraft|uploadUpdaterJson|nsis|NSIS|--bundles dmg|msi|setup\.exe/);
  expect(release).toContain("gh release view \"$GITHUB_REF_NAME\" --repo \"$GITHUB_REPOSITORY\" --json body --jq .body");
  expect(release).toContain("NOTES: ${{ needs.notes.outputs.body }}");
  expect(release).toContain("run: npm run tauri -- build --no-bundle -- --locked");
  expect(release).toContain("run: npm run tauri -- build --target universal-apple-darwin --bundles app -- --locked");
  expect(release).toContain("run: npm run tauri -- build --bundles appimage -- --locked");
  expect(release).toContain("for f in packages/*.zip packages/*.AppImage; do");
  expect(release).toContain("node scripts/portable-release.mjs package --os ${{ matrix.package }} --version \"$VERSION\" --out packages");
  expect(release).toContain('node node_modules/@tauri-apps/cli/tauri.js signer sign --app-version "$VERSION" "$f"');
  expect(release).toContain("node scripts/portable-release.mjs manifest --version \"$VERSION\" --notes-file \"$RUNNER_TEMP/notes.md\" --dir packages");
  expect(release).toContain('node scripts/verify-update-sig.mjs packages/latest.json --version "$GITHUB_REF_NAME" --dir packages');
  expect(release).toContain('gh release upload "$GITHUB_REF_NAME" --repo "$GITHUB_REPOSITORY" --clobber packages/*');
  expect(release).toContain("node scripts/check-latest-json.mjs latest.json");
  expect(release).toContain('node scripts/verify-update-sig.mjs latest.json --version "$GITHUB_REF_NAME"');
  expect(release).toContain("--json isDraft --jq .isDraft)\" = false");
  expect(release).toMatch(/^ {2}update-e2e:\n {4}needs: latest-json\n {4}uses: \.\/\.github\/workflows\/update-e2e\.yml$/m);
});

test("what ships is checked: no e2e-only code, the pubkey baked in, the Windows zero-install gate, the macOS signature", () => {
  expect(release).toContain('node scripts/check-release-binary.mjs "$bin" --pubkey "$TAURI_UPDATER_PUBKEY"');
  expect(release).toContain('node scripts/check-windows-exe.mjs "$bin"');
  expect(release).toContain('codesign --verify --deep --strict --verbose=2 "$app"');
  const e2e = Buffer.from("x FREEWHEEL_E2E_UPDATE_URL x");
  expect(binaryProblems(e2e)).toEqual(["contains FREEWHEEL_E2E_UPDATE_URL: built with the e2e feature, never ship it"]);
  expect(binaryProblems(Buffer.from("x PUBKEY x"), { pubkey: "PUBKEY" })).toEqual([]);
  expect(binaryProblems(Buffer.from("x x"), { pubkey: "PUBKEY" })).toEqual(["the updater pubkey is not baked in (FREEWHEEL_UPDATER_PUBKEY at build)"]);
  // The Rust side names the override exactly so, and only under the e2e feature.
  const rust = read("../src-tauri/src/selfupdate.rs");
  expect(rust).toMatch(/#\[cfg\(feature = "e2e"\)\]\npub const E2E_MANIFEST_URL_ENV: &str = "FREEWHEEL_E2E_UPDATE_URL";/);
  expect(rust.match(/"FREEWHEEL_E2E_UPDATE_URL"/g)).toHaveLength(2); // the const, and the test that a shipped build ignores it
});

test("no installer: the bundle is the macOS .app only, macOS 15.0", () => {
  expect(conf.bundle.targets).toEqual(["app"]);
  expect(conf.bundle.createUpdaterArtifacts).toBeUndefined();
  expect(conf.bundle.windows).toBeUndefined();
  expect(conf.bundle.macOS.minimumSystemVersion).toBe("15.0");
  expect(release).not.toMatch(/WINDOWS_CERTIFICATE|certificateThumbprint|signCommand|AZURE_/);
});

test("[T3][T7] the release build bakes in the pubkey variable, and no key is committed", () => {
  expect(release.match(/run: npm run tauri -- build/g)).toHaveLength(3);
  const pkg = JSON.parse(read("../package.json"));
  expect(pkg.scripts.tauri).toBe("node scripts/tauri-release.mjs");
  expect(release).not.toMatch(/secrets\.TAURI_UPDATER_PUBKEY/);
  expect(conf.plugins).toBeUndefined();
});

test("tauri build gets the pubkey as FREEWHEEL_UPDATER_PUBKEY, the args unchanged", () => {
  const { problems, args, env } = releaseArgs(["build", "--target", "universal-apple-darwin"], { TAURI_UPDATER_PUBKEY: " public-key-from-vars\n" });
  expect(problems).toEqual([]);
  expect(args).toEqual(["build", "--target", "universal-apple-darwin"]);
  expect(env).toEqual({ FREEWHEEL_UPDATER_PUBKEY: "public-key-from-vars" });
});

test("the release build fails naming a missing TAURI_UPDATER_PUBKEY", () => {
  const { problems, args } = releaseArgs(["build"], { TAURI_UPDATER_PUBKEY: " " });
  expect(args).toBeNull();
  expect(problems).toHaveLength(1);
  expect(problems[0]).toMatch(/^release build needs TAURI_UPDATER_PUBKEY:/);
});

test("other Tauri commands pass through without the release env", () => {
  expect(releaseArgs(["info"], {})).toEqual({ problems: [], args: ["info"], env: {} });
});

const at = (file) => ({ url: `https://github.com/brinkercode/freewheel/releases/download/v0.1.0/${file}`, signature: "sig" });
const latest = {
  version: "0.1.0",
  platforms: {
    "windows-x86_64": at("freewheel-0.1.0-windows-x64.exe"),
    "darwin-aarch64": at("freewheel-0.1.0-macos-universal.dmg"),
    "darwin-x86_64": at("freewheel-0.1.0-macos-universal.dmg"),
    "linux-x86_64": at("freewheel-0.1.0-linux-x86_64.AppImage"),
  },
};

test("latest.json must point each platform at its release file", () => {
  const platforms = { ...latest.platforms, "darwin-aarch64": at("freewheel-0.1.0-windows-x64.exe") };
  expect(checkLatestJson({ ...latest, platforms })).toEqual([
    "'darwin-aarch64' is not freewheel-0.1.0-macos-universal.dmg: https://github.com/brinkercode/freewheel/releases/download/v0.1.0/freewheel-0.1.0-windows-x64.exe",
  ]);
});

test("the packages and the manifest the release writes are what the updater reads", () => {
  expect(PACKAGES.windows("0.1.1")).toBe("freewheel-0.1.1-windows-x64.exe");
  expect(PACKAGES.macos("0.1.1")).toBe("freewheel-0.1.1-macos-universal.dmg");
  expect(PACKAGES.linux("0.1.1")).toBe("freewheel-0.1.1-linux-x86_64.AppImage");
  expect(SERVED).toEqual({ "windows-x86_64": "windows", "darwin-aarch64": "macos", "darwin-x86_64": "macos", "linux-x86_64": "linux" });
  const fixture = JSON.parse(read("./fixtures/portable-latest.json"));
  const signatures = { "freewheel-0.1.1-windows-x64.exe": "c2ln\n", "freewheel-0.1.1-macos-universal.dmg": "c2ln", "freewheel-0.1.1-linux-x86_64.AppImage": "c2ln" };
  const made = manifest({
    version: "0.1.1",
    notes: fixture.notes,
    baseUrl: "https://github.com/brinkercode/freewheel/releases/download/v0.1.1/",
    signatures,
    pubDate: fixture.pub_date,
  });
  // The fixture is what src-tauri/src/selfupdate.rs parses in its test.
  expect(made).toEqual(fixture);
  expect(checkLatestJson(made)).toEqual([]);
  expect(manifest({ version: "0.1.1", notes: "", baseUrl: "https://x", signatures, only: ["windows"] }).platforms).toEqual({
    "windows-x86_64": { url: "https://x/freewheel-0.1.1-windows-x64.exe", signature: "c2ln" },
  });
  expect(() => manifest({ version: "0.1.1", baseUrl: "https://x", signatures: {} })).toThrow(/no signature for freewheel-0.1.1-windows-x64.exe/);
  expect(() => manifest({ version: "nope", baseUrl: "https://x", signatures })).toThrow(/not a version/);
  expect(packageArgs(["package", "--os", "macos", "--version", "0.1.1"])).toEqual({ command: "package", os: "macos", version: "0.1.1" });
});

test("latest.json with every platform passes", () => {
  expect(checkLatestJson(latest)).toEqual([]);
});

test.each([
  ["windows-x86_64"],
  ["darwin-aarch64"],
  ["darwin-x86_64"],
  ["linux-x86_64"],
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

// The fixture the Rust updater tests too, signed by a throwaway key (only its
// pubkey is committed) with `tauri signer sign --app-version 0.2.0`.
const fixture = (name) => readFileSync(new URL(`../src-tauri/fixtures/update/${name}`, import.meta.url));
const data = fixture("package.bin");
const signature = fixture("package.bin.sig").toString("utf8");
const pubkey = fixture("test.pub").toString("utf8");

test("[T3] an update file verifies against the updater pubkey, bound to its version", () => {
  expect(signatureProblem(data, signature, pubkey)).toBeNull();
  expect(signatureProblem(data, signature, pubkey, "0.2.0")).toBeNull();
  expect(boundVersion(signature)).toBe("0.2.0");
  expect(signatureProblem(data, signature, pubkey, "0.3.0")).toBe("signed for version 0.2.0, not 0.3.0");
  expect(signatureProblem(data, signature, fixture("other.pub").toString("utf8"))).toBe("signed by another key");
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
  const url = "https://github.com/brinkercode/freewheel/releases/download/v0.2.0/freewheel-0.2.0-macos-universal.dmg";
  const json = {
    version: "0.2.0",
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
  expect(releasePlease).toContain("for wf in ci.yml e2e-windows.yml e2e-linux.yml; do");
  expect(releasePlease).toContain('gh workflow run "$wf" --repo "$GITHUB_REPOSITORY" --ref "$branch"');
  expect(releasePlease).toContain('gh workflow run release.yml --repo "$GITHUB_REPOSITORY" --ref "$TAG"');
  expect(releasePlease).toContain("if: steps.release.outputs.release_created == 'true'");
});

const e2e = read("../.github/workflows/update-e2e.yml");

test("update-e2e downloads the previous release's exe into a folder and updates it with that app's own updater", () => {
  expect(e2e).toMatch(/^on:\n {2}workflow_dispatch:/m);
  expect(e2e).toMatch(/^ {2}workflow_call:$/m);
  expect(e2e).toMatch(/^permissions:\n {2}contents: read$/m);
  expect(e2e).not.toMatch(/secrets\.|: write\b|setup\.exe|\/S'/);
  expect(e2e).toContain("runs-on: windows-2025");
  expect(e2e).toContain('--pattern "freewheel-$env:FROM_VERSION-windows-x64.exe"');
  expect(e2e).not.toMatch(/Expand-Archive|\.zip/);
  expect(e2e).toContain('$exe = "$env:RUNNER_TEMP\\freewheel\\freewheel-$env:FROM_VERSION-windows-x64.exe"');
  expect(e2e.match(/-ArgumentList '--update-check-and-install'/g)).toHaveLength(2);
  expect(e2e).toContain("if ($now -ne $env:TO_VERSION)");
  expect(e2e).toContain('if (Test-Path "$exe.old")');
  expect(e2e).toContain("node scripts/check-windows-exe.mjs $exe");
  expect(e2e).toContain("if ($run.ExitCode -ne 2)");
  expect(read("../src-tauri/src/selfupdate.rs")).toContain('pub const HEADLESS_FLAG: &str = "--update-check-and-install";');
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

test("the Windows release file is the exe itself, byte for byte; the simulator .cmd is for test builds", () => {
  const dir = mkdtempSync(join(tmpdir(), "fw-pkg-"));
  const exe = join(dir, "freewheel.exe");
  writeFileSync(exe, Buffer.from("MZ the exe"));
  const file = makePackage({ os: "windows", version: "0.1.1", out: join(dir, "out"), exe });
  expect(basename(file)).toBe("freewheel-0.1.1-windows-x64.exe");
  expect(readFileSync(file).equals(readFileSync(exe))).toBe(true);
  expect(readdirSync(join(dir, "out"))).toEqual(["freewheel-0.1.1-windows-x64.exe"]);
  expect(() => makePackage({ os: "windows", version: "0.1.1", out: dir, exe: join(dir, "gone.exe") })).toThrow(/--exe/);
  expect(() => makePackage({ os: "linux-zip", version: "0.1.1", out: dir })).toThrow(/unknown --os/);
  const script = read("./portable-release.mjs");
  expect(script).not.toContain("freewheel-simulate.cmd");
  expect(read("../.github/workflows/release.yml")).not.toContain("freewheel-simulate.cmd");
});

test("no zip anywhere in a release: the exe, the .dmg, their signatures, latest.json", () => {
  const script = read("./portable-release.mjs");
  expect(script).not.toMatch(/tar\.exe|"-k"|\.zip/);
  expect(release).not.toMatch(/\.zip\b/);
  expect(release).toContain("for f in packages/*.exe packages/*.dmg; do");
  expect(release).toContain("hdiutil verify packages/*.dmg");
  // The .dmg: freewheel.app and an Applications link, made with hdiutil.
  expect(DMG_CREATE).toEqual(["create", "-volname", "freewheel", "-fs", "HFS+", "-format", "UDZO", "-ov", "-srcfolder"]);
  expect(script).toContain('symlinkSync("/Applications", join(stage, "Applications"))');
  expect(script).toContain('execFileSync("/usr/bin/ditto", [app, join(stage, "freewheel.app")]');
  // The updater no longer unpacks anything.
  const rust = read("../src-tauri/src/selfupdate.rs");
  expect(rust).not.toMatch(/zip::|ZipArchive/);
  expect(read("../src-tauri/Cargo.toml")).not.toMatch(/^zip = /m);
  expect(rust).toContain('"-readonly"');
});
