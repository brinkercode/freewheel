import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "vitest";
import { checkWorkflow } from "./check-workflow.mjs";
import { e2eConfig } from "./e2e-update-config.mjs";
import { NOTES, parseArgs } from "./e2e-update-windows.mjs";
import { chunks, latestJson, startUpdateServer } from "./update-server.mjs";
import { noteSections } from "../src/whatsNew.js";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const wf = read("../.github/workflows/e2e-windows.yml");
const job = wf.slice(wf.indexOf("\n  update:"));

test("the e2e build override: the version, the test pubkey and a loopback-only http endpoint", () => {
  const c = e2eConfig({ version: "0.2.0", pubkey: " PUB\n", port: "8719" });
  expect(c.version).toBe("0.2.0");
  expect(c.bundle.createUpdaterArtifacts).toBe(true);
  expect(c.plugins.updater).toEqual({
    pubkey: "PUB",
    endpoints: ["http://127.0.0.1:8719/latest.json"],
    dangerousInsecureTransportProtocol: true,
  });
  expect(() => e2eConfig({ version: "x", pubkey: "P", port: 1 })).toThrow(/version/);
  expect(() => e2eConfig({ version: "0.2.0", pubkey: " ", port: 1 })).toThrow(/pubkey/);
  expect(() => e2eConfig({ version: "0.2.0", pubkey: "P", port: 0 })).toThrow(/port/);
});

test("latest.json has the shape the updater reads, for Windows", () => {
  expect(latestJson({ version: "0.2.0", notes: "n", signature: "SIG\n", url: "http://127.0.0.1:1/a.exe", pubDate: "d" })).toEqual({
    version: "0.2.0",
    notes: "n",
    pub_date: "d",
    platforms: { "windows-x86_64": { signature: "SIG", url: "http://127.0.0.1:1/a.exe" } },
  });
  expect(chunks(10, 4)).toEqual([
    [0, 4],
    [4, 8],
    [8, 10],
  ]);
});

test("the local release server fails, then has nothing, then offers the bundle, streamed in chunks", async () => {
  const dir = mkdtempSync(join(process.env.TMPDIR ?? "/tmp", "fw-upd-"));
  const bundle = join(dir, "freewheel_0.2.0_x64-setup.exe");
  writeFileSync(bundle, Buffer.alloc(300 * 1024, 7));
  const server = await startUpdateServer({ bundle, signature: "SIG", version: "0.2.0", notes: "n", downloadMs: 200 });
  try {
    expect((await fetch(`${server.url}/latest.json`)).status).toBe(500);
    await fetch(`${server.url}/__mode`, { method: "POST", body: "none" });
    expect((await fetch(`${server.url}/latest.json`)).status).toBe(204);
    server.setMode("offer");
    const latest = await (await fetch(`${server.url}/latest.json`)).json();
    expect(latest.version).toBe("0.2.0");
    expect(latest.platforms["windows-x86_64"].url).toBe(`${server.url}/freewheel_0.2.0_x64-setup.exe`);
    const res = await fetch(latest.platforms["windows-x86_64"].url);
    expect(res.headers.get("content-length")).toBe(String(300 * 1024));
    let reads = 0;
    let size = 0;
    for await (const part of res.body) {
      reads++;
      size += part.length;
    }
    expect(size).toBe(300 * 1024);
    expect(reads).toBeGreaterThan(1);
    expect((await fetch(`${server.url}/__mode`, { method: "POST", body: "nope" })).status).toBe(400);
  } finally {
    await server.close();
  }
});

test("the update e2e script needs the app, both versions and B's bundle and signature", () => {
  expect(() => parseArgs([])).toThrow(/--app/);
  const o = parseArgs(["--app", "a.exe", "--bundle", "b.exe", "--sig", "b.sig", "--from", "0.1.0", "--to", "0.2.0"]);
  expect(o["server-port"]).toBe("8719");
  expect(o.port).toBe("9223");
  // What's new for B shows the notes it was published with.
  expect(noteSections(NOTES).map((s) => s.title)).toEqual(["Features", "Bug Fixes"]);
});

test("the update job: no secrets, a throwaway key, two e2e builds, A installed, B asserted, screenshots kept", () => {
  expect(checkWorkflow(wf)).toEqual([]);
  expect(job).toContain("name: update e2e (windows-2025, local release server)");
  expect(job).toContain('signer generate --ci -p "" -w "$key" -f');
  expect(job).toContain("--features e2e --bundles nsis --config");
  expect(job).toContain("node scripts/e2e-update-config.mjs");
  expect(job).toContain("node scripts/e2e-update-windows.mjs");
  expect(job).toContain("-ArgumentList '/S' -Wait");
  expect(job).toMatch(/if \(\$installed -ne \$env:TO_VERSION\)/);
  expect(job).toMatch(/name: e2e-update-screenshots\n\s+path: e2e-update-shots/);
  // The port and server the builds point at are the ones the script serves.
  expect(job).toContain('SERVER_PORT: "8719"');
});

test("shipped builds never get the e2e feature, the e2e config or an http endpoint", () => {
  for (const file of ["release.yml", "ci.yml", "test-build.yml", "update-e2e.yml"]) {
    const text = read(`../.github/workflows/${file}`);
    expect(text, file).not.toMatch(/features[ =]+e2e|e2e-update-config|dangerousInsecure/);
  }
  expect(read("./tauri-release.mjs")).not.toMatch(/e2e|dangerousInsecure/);
  const cargo = read("../src-tauri/Cargo.toml");
  expect(cargo).toMatch(/^e2e = \[\]$/m);
  expect(cargo).not.toMatch(/^default = .*e2e/m);
});

test("the release PR gets every required check without approval: release-please dispatches ci.yml and e2e-windows.yml", () => {
  const rp = read("../.github/workflows/release-please.yml");
  expect(rp).toContain("for wf in ci.yml e2e-windows.yml; do");
  expect(rp).toContain('gh workflow run "$wf" --repo "$GITHUB_REPOSITORY" --ref "$branch"');
  expect(wf).toMatch(/^ {2}workflow_dispatch:/m);
  expect(read("../.github/workflows/ci.yml")).toMatch(/^ {2}workflow_dispatch:/m);
});
