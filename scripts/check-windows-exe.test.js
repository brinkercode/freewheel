import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { checkImports, dependents } from "./check-windows-exe.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const ci = read("../.github/workflows/ci.yml");
const release = read("../.github/workflows/release.yml");
const CHECK = "node scripts/check-windows-exe.mjs";

const dumpbin = `Dump of file target\\release\\freewheel.exe

File Type: EXECUTABLE IMAGE

  Image has the following dependencies:

    KERNEL32.dll
    USER32.dll
    api-ms-win-crt-runtime-l1-1-0.dll
    VCRUNTIME140.dll

  Summary

        1000 .data
`;
const system = new Set(["kernel32.dll", "user32.dll", "vcruntime140.dll", "msvcp140.dll"]);
const isSystem = (dll) => system.has(dll.toLowerCase());

test("dumpbin /dependents output lists the imported DLLs", () => {
  expect(dependents(dumpbin)).toEqual([
    "KERNEL32.dll", "USER32.dll", "api-ms-win-crt-runtime-l1-1-0.dll", "VCRUNTIME140.dll",
  ]);
});

test("system DLLs and the OS UCRT api sets pass", () => {
  expect(checkImports(["KERNEL32.dll", "USER32.dll", "api-ms-win-crt-heap-l1-1-0.dll"], isSystem)).toEqual([]);
});

test.each(["VCRUNTIME140.dll", "vcruntime140_1.dll", "MSVCP140.dll", "WebView2Loader.dll"])(
  "the import check rejects %s, even when it sits in System32",
  (dll) => {
    expect(checkImports(["KERNEL32.dll", dll], isSystem)).toEqual([
      `imports ${dll}, which is not a Windows system DLL`,
    ]);
  },
);

test("an exe with no imports found fails", () => {
  expect(checkImports([], isSystem)).toEqual(["found no imports"]);
});

test("the MSVC target links the C runtime statically", () => {
  const config = read("../.cargo/config.toml");
  expect(config).toMatch(
    /^\[target\.x86_64-pc-windows-msvc\]\r?\nrustflags = \["-C", "target-feature=\+crt-static"\]$/m,
  );
});

test("ci.yml builds freewheel.exe on Windows, then runs the import check and smoke on it", () => {
  const build = ci.indexOf("run: cargo build --release --locked -p freewheel --features tauri/custom-protocol");
  const check = ci.indexOf(`run: ${CHECK} target/release/freewheel.exe`);
  expect(build).toBeGreaterThan(ci.indexOf("run: cargo test --workspace --locked"));
  expect(check).toBeGreaterThan(build);
  expect(ci.slice(build - 60, check)).toContain("if: runner.os == 'Windows'");
});

test("ci.yml skips the zero-install build on pull requests, and the required job names stay", () => {
  const gated = "if: runner.os == 'Windows' && github.event_name != 'pull_request'";
  expect(ci.split(gated)).toHaveLength(3);
  expect(ci).toContain(`${gated}\n        run: cargo build --release --locked -p freewheel`);
  expect(ci).toContain(`${gated}\n        run: ${CHECK} target/release/freewheel.exe`);
  expect(ci).toMatch(/^ {2}lint:\n {4}name: lint \(ubuntu-latest\)$/m);
  expect(ci).toMatch(/^ {2}test:\n {4}name: test \(\$\{\{ matrix\.os \}\}\)$/m);
});

test("release.yml runs the zero-install gate on the built exe before packing it (no installer)", () => {
  const build = release.indexOf("run: npm run tauri -- build --no-bundle -- --locked");
  const check = release.indexOf(`${CHECK} "$bin"`);
  const pack = release.indexOf("node scripts/portable-release.mjs package");
  expect(build).toBeGreaterThan(0);
  expect(check).toBeGreaterThan(build);
  expect(pack).toBeGreaterThan(check);
  expect(release).not.toMatch(/-ArgumentList '\/S'|setup\.exe/);
});

test("the check script runs the smoke flags and requires exit 0", () => {
  const script = read("./check-windows-exe.mjs");
  expect(script).toContain('spawnSync(exe, ["--simulate", "--smoke"]');
  expect(script).toContain("if (smoke.status !== 0)");
  expect(script).toContain('"/dependents"');
});
