#!/usr/bin/env node
// Windows zero-install gate (ci.yml and release.yml): lists freewheel.exe's
// imports with dumpbin /dependents and fails on any DLL that is not part of
// Windows (vcruntime*.dll and msvcp*.dll above all; api-ms-win-* is the OS
// UCRT), then runs the exe with --simulate --smoke and requires exit 0.
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

/** The DLL names in `dumpbin /dependents` output. */
export function dependents(dumpbinOutput) {
  return [...dumpbinOutput.matchAll(/^\s+(\S+\.dll)\s*$/gim)].map((m) => m[1]);
}

/** Problems with the imported DLLs (empty when all are Windows system DLLs). */
export function checkImports(dlls, isSystemDll) {
  if (dlls.length === 0) return ["found no imports"];
  return dlls
    .filter((dll) => /^(vcruntime|msvcp)/i.test(dll) || !(/^api-ms-win-/i.test(dll) || isSystemDll(dll)))
    .map((dll) => `imports ${dll}, which is not a Windows system DLL`);
}

function dumpbin() {
  const vswhere = join(process.env["ProgramFiles(x86)"], "Microsoft Visual Studio", "Installer", "vswhere.exe");
  const found = execFileSync(vswhere, ["-latest", "-products", "*", "-find", "VC\\Tools\\MSVC\\**\\bin\\Hostx64\\x64\\dumpbin.exe"], { encoding: "utf8" });
  return found.split(/\r?\n/).find(Boolean);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const exe = process.argv[2];
  if (!exe) {
    console.error("usage: check-windows-exe.mjs <freewheel.exe>");
    process.exit(1);
  }
  const system32 = join(process.env.SystemRoot, "System32");
  const output = execFileSync(dumpbin(), ["/dependents", exe], { encoding: "utf8" });
  const dlls = dependents(output);
  console.log(`${exe} imports: ${dlls.join(", ")}`);
  const problems = checkImports(dlls, (dll) => existsSync(join(system32, dll)));
  for (const problem of problems) console.error(`${exe}: ${problem}`);
  if (problems.length > 0) process.exit(1);
  const smoke = spawnSync(exe, ["--simulate", "--smoke"], { stdio: "inherit", timeout: 180_000 });
  if (smoke.status !== 0) {
    console.error(`${exe} --simulate --smoke: exit ${smoke.status}${smoke.error ? ` (${smoke.error.message})` : ""}`);
    process.exit(1);
  }
  console.log(`${exe} --simulate --smoke: exit 0`);
}
