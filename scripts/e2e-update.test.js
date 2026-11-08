import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test } from "vitest";
import { checkWorkflow } from "./check-workflow.mjs";
import { e2eConfig, nextVersion } from "./e2e-update-config.mjs";
import { UPDATE_URL_ENV, configFile, parseArgs, populate, runningPathsCommand, updateTemp } from "./e2e-update-windows.mjs";
import { chunks, localManifest, startUpdateServer, tamper } from "./update-server.mjs";
import { sampleConfig } from "../src/dev/mockTauri.js";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const wf = read("../.github/workflows/e2e-windows.yml");
const job = wf.slice(wf.indexOf("\n  update:"));

test("the e2e builds: only the version is overridden; the next version is the next minor", () => {
  expect(e2eConfig({ version: "0.2.0" })).toEqual({ version: "0.2.0" });
  expect(() => e2eConfig({ version: "x" })).toThrow(/version/);
  expect(nextVersion("0.1.0")).toBe("0.2.0");
  expect(nextVersion("v1.4.2")).toBe("1.5.0");
  expect(nextVersion("0.2.0-beta.1")).toBe("0.3.0");
});

test("the local server moves the manifest's urls to itself; a tampered copy differs in one byte", () => {
  const m = { version: "0.2.0", notes: "n", platforms: { "windows-x86_64": { url: "https://github.com/x/releases/download/v0.2.0/freewheel-0.2.0-windows-x64.exe", signature: "S" } } };
  expect(localManifest(m, "http://127.0.0.1:9").platforms["windows-x86_64"]).toEqual({ url: "http://127.0.0.1:9/freewheel-0.2.0-windows-x64.exe", signature: "S" });
  const data = Buffer.alloc(10, 1);
  const bad = tamper(data);
  expect([...bad].filter((b, i) => b !== data[i])).toHaveLength(1);
  expect(chunks(10, 4)).toEqual([
    [0, 4],
    [4, 8],
    [8, 10],
  ]);
});

test("the local release server fails, has nothing, offers the package streamed in chunks, or a tampered one", async () => {
  const dir = mkdtempSync(join(tmpdir(), "fw-upd-"));
  const pkg = join(dir, "freewheel-0.2.0-windows-x64.exe");
  writeFileSync(pkg, Buffer.alloc(300 * 1024, 7));
  const manifest = { version: "0.2.0", notes: "n", platforms: { "windows-x86_64": { url: "https://x/freewheel-0.2.0-windows-x64.exe", signature: "SIG" } } };
  const server = await startUpdateServer({ manifest, package: pkg, downloadMs: 200 });
  const body = async (url) => {
    const res = await fetch(url);
    let reads = 0;
    const parts = [];
    for await (const part of res.body) {
      reads++;
      parts.push(part);
    }
    return { res, reads, data: Buffer.concat(parts) };
  };
  try {
    expect((await fetch(`${server.url}/latest.json`)).status).toBe(500);
    server.setMode("none");
    expect((await fetch(`${server.url}/latest.json`)).status).toBe(204);
    server.setMode("offer");
    const latest = await (await fetch(`${server.url}/latest.json`)).json();
    expect(latest.version).toBe("0.2.0");
    expect(latest.platforms["windows-x86_64"]).toEqual({ url: `${server.url}/freewheel-0.2.0-windows-x64.exe`, signature: "SIG" });
    const good = await body(latest.platforms["windows-x86_64"].url);
    expect(good.res.headers.get("content-length")).toBe(String(300 * 1024));
    expect(good.reads).toBeGreaterThan(1);
    expect(good.data.equals(readFileSync(pkg))).toBe(true);
    server.setMode("tampered");
    const bad = await body(latest.platforms["windows-x86_64"].url);
    expect(bad.data.length).toBe(300 * 1024);
    expect(bad.data.equals(readFileSync(pkg))).toBe(false);
    expect(() => server.setMode("nope")).toThrow(/unknown mode/);
  } finally {
    await server.close();
  }
});

test("the update e2e script needs the app, both versions and B's manifest and package; it keeps a populated config", () => {
  expect(() => parseArgs([])).toThrow(/--app/);
  const o = parseArgs(["--app", "a\\freewheel.exe", "--manifest", "latest.json", "--package", "b\\freewheel-0.2.0-windows-x64.exe", "--from", "0.1.0", "--to", "0.2.0"]);
  expect(o["server-port"]).toBe("8719");
  expect(o.port).toBe("9223");
  const c = populate(sampleConfig());
  expect(c.layers.find((l) => l.id === "e2e-page").bindings.Out.Hotkey.combo.key).toBe("J");
  expect(c.layers.find((l) => l.id === c.base_layer).bindings.Cut).toEqual({ LayerPush: { target: "e2e-page" } });
  expect(populate(c)).toEqual(c);
  expect(configFile("C:\\Users\\u\\AppData\\Roaming")).toBe(join("C:\\Users\\u\\AppData\\Roaming", "freewheel", "config", "config.json"));
  expect(updateTemp("T")).toBe(join("T", "freewheel-update"));
  // B runs from A's path: found by the downloaded file's name, not "freewheel".
  expect(runningPathsCommand("C:\\d\\freewheel-0.1.0-windows-x64.exe")).toBe(
    `Get-CimInstance Win32_Process -Filter "Name = 'freewheel-0.1.0-windows-x64.exe'" | ForEach-Object { $_.ExecutablePath }`,
  );
  // The override the script sets is the one only e2e builds read.
  expect(read("../src-tauri/src/selfupdate.rs")).toContain(`pub const E2E_MANIFEST_URL_ENV: &str = "${UPDATE_URL_ENV}";`);
});

test("the script checks: tampered rejected with A intact, progress, same path runs B, What's new, config kept, no leftovers", () => {
  const script = read("./e2e-update-windows.mjs");
  for (const needle of [
    'server.setMode("tampered")',
    "the tampered package changed ${app}",
    "the rejected download was kept",
    "the download showed too little progress",
    "B does not run from",
    "freewheel is updated to",
    "the config changed",
    "config.json changed on disk",
    "${app}.old is still there",
  ])
    expect(script).toContain(needle);
});

test("the update job: no secrets, a throwaway key, two e2e builds made into release files as the release does, A's exe run as downloaded, B asserted", () => {
  expect(checkWorkflow(wf)).toEqual([]);
  expect(job).toContain("name: update e2e (windows-2025, local release server)");
  expect(job).toContain('signer generate --ci -p "" -w "$key" -f');
  expect(job).toContain('export FREEWHEEL_UPDATER_PUBKEY="$(cat "$E2E_UPDATER_KEY.pub")"');
  expect(job).toContain('build --ci --no-bundle --features e2e --config "$(node scripts/e2e-update-config.mjs "$v")" -- --locked');
  expect(job).toContain("node scripts/portable-release.mjs package --os windows");
  expect(job).toContain('exe="$RUNNER_TEMP/release-$TO_VERSION/freewheel-$TO_VERSION-windows-x64.exe"');
  expect(job).toContain('signer sign -f "$E2E_UPDATER_KEY" -p "" --app-version "$TO_VERSION" "$exe"');
  expect(job).toContain("node scripts/portable-release.mjs manifest --version \"$TO_VERSION\"");
  // No zip anywhere in the update: A is the downloaded exe, B's file is an exe.
  expect(job).not.toMatch(/Expand-Archive "\$env:RUNNER_TEMP\\release|windows-x64\.zip|\$zip"/);
  expect(job).toContain('"FREEWHEEL_EXE=$env:RUNNER_TEMP\\Downloads\\freewheel-$env:FROM_VERSION-windows-x64.exe"');
  expect(job).toContain('node scripts/e2e-update-windows.mjs --app $env:FREEWHEEL_EXE --manifest "$b\\latest.json" --package "$b\\freewheel-$env:TO_VERSION-windows-x64.exe"');
  expect(job).toContain('if ((Get-ChildItem "$env:RUNNER_TEMP\\Downloads").Count -ne 1)');
  expect(job).not.toMatch(/nsis|setup\.exe|\/S'/);
  expect(job).toMatch(/if \(\$now -ne \$env:TO_VERSION\)/);
  expect(job).toMatch(/name: e2e-update-screenshots\n\s+path: e2e-update-shots/);
  expect(job).toContain('SERVER_PORT: "8719"');
});

test("shipped builds never get the e2e feature or the e2e override", () => {
  for (const file of ["release.yml", "ci.yml", "test-build.yml", "update-e2e.yml"]) {
    const text = read(`../.github/workflows/${file}`);
    expect(text, file).not.toMatch(/features[ =]+e2e|features e2e|e2e-update-config|FREEWHEEL_E2E_UPDATE_URL/);
  }
  expect(read("./tauri-release.mjs")).not.toMatch(/e2e/);
  const cargo = read("../src-tauri/Cargo.toml");
  expect(cargo).toMatch(/^e2e = \[\]$/m);
  expect(cargo).not.toMatch(/^default = .*e2e/m);
  // release.yml and test-build.yml check the binaries they ship.
  expect(read("../.github/workflows/release.yml")).toContain("node scripts/check-release-binary.mjs");
  expect(read("../.github/workflows/test-build.yml")).toContain("node scripts/check-release-binary.mjs target/release/freewheel.exe");
});

test("the release PR gets every required check without approval: release-please dispatches ci.yml, e2e-windows.yml and e2e-linux.yml", () => {
  const rp = read("../.github/workflows/release-please.yml");
  expect(rp).toContain("for wf in ci.yml e2e-windows.yml e2e-linux.yml; do");
  expect(rp).toContain('gh workflow run "$wf" --repo "$GITHUB_REPOSITORY" --ref "$branch"');
  expect(wf).toMatch(/^ {2}workflow_dispatch:/m);
  expect(read("../.github/workflows/ci.yml")).toMatch(/^ {2}workflow_dispatch:/m);
});
