#!/usr/bin/env node
// Verifies a release's latest.json the way the in-app updater does
// (src-tauri/src/selfupdate.rs): each platform's package must carry a
// minisign signature by the updater key (the pubkey release builds embed),
// bound to the manifest's version. Also checks the version and the notes.
//
//   TAURI_UPDATER_PUBKEY=<base64 pubkey> node scripts/verify-update-sig.mjs latest.json [--version 0.1.1] [--dir <packages>]
//
// Each platform's url is downloaded once (the universal macOS zip serves
// both macOS keys) and checked against its signature; with --dir the
// packages are read from that folder by file name instead (before upload).
import { createHash, createPublicKey, verify } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { checkLatestJson } from "./check-latest-json.mjs";

const b64 = (text) => Buffer.from(String(text).trim(), "base64");
const lines = (text) => text.split(/\r?\n/).filter((l) => l.trim() !== "");
// An Ed25519 SubjectPublicKeyInfo is this prefix and the 32 raw key bytes.
const SPKI_ED25519 = Buffer.from("302a300506032b6570032100", "hex");

/** Parses a Tauri pubkey (base64 of minisign's .pub text): { keyId, key }. */
export function parsePubkey(pubkey) {
  const raw = b64(lines(b64(pubkey).toString("utf8")).at(-1));
  if (raw.length !== 42 || raw.subarray(0, 2).toString() !== "Ed") throw new Error("not a minisign public key");
  return {
    keyId: raw.subarray(2, 10),
    key: createPublicKey({ key: Buffer.concat([SPKI_ED25519, raw.subarray(10)]), format: "der", type: "spki" }),
  };
}

/** Parses a Tauri signature (base64 of minisign's .sig text). */
export function parseSignature(signature) {
  const [, sigLine, trusted, globalLine] = lines(b64(signature).toString("utf8"));
  const sig = b64(sigLine ?? "");
  if (sig.length !== 74 || !/^trusted comment: /.test(trusted ?? "")) throw new Error("not a minisign signature");
  return {
    algorithm: sig.subarray(0, 2).toString(),
    keyId: sig.subarray(2, 10),
    sig: sig.subarray(10),
    trusted: trusted.slice("trusted comment: ".length),
    global: b64(globalLine ?? ""),
  };
}

/** The version a signature is bound to (`tauri signer sign --app-version`), or null. */
export function boundVersion(signature) {
  try {
    const field = parseSignature(signature).trusted.split("\t").find((f) => f.startsWith("version:"));
    return field ? field.slice("version:".length) : null;
  } catch {
    return null;
  }
}

/** Returns the problem with `signature` over `data` under `pubkey` (bound to `version`, if given), or null when it verifies. */
export function signatureProblem(data, signature, pubkey, version) {
  let key;
  let sig;
  try {
    key = parsePubkey(pubkey);
    sig = parseSignature(signature);
  } catch (e) {
    return e.message;
  }
  if (!sig.keyId.equals(key.keyId)) return "signed by another key";
  if (sig.algorithm !== "ED" && sig.algorithm !== "Ed") return `unknown algorithm '${sig.algorithm}'`;
  // "ED" signs the BLAKE2b-512 of the file (what Tauri's signer makes); "Ed" the file itself.
  const message = sig.algorithm === "ED" ? createHash("blake2b512").update(data).digest() : data;
  if (!verify(null, message, key.key, sig.sig)) return "bad signature";
  const global = Buffer.concat([sig.sig, Buffer.from(sig.trusted, "utf8")]);
  if (!verify(null, global, key.key, sig.global)) return "bad trusted comment signature";
  if (version && boundVersion(signature) !== version) return `signed for version ${boundVersion(signature)}, not ${version}`;
  return null;
}

/** latest.json's own problems: platforms (check-latest-json), the version and the notes. */
export function latestProblems(json, { version } = {}) {
  const problems = checkLatestJson(json);
  if (version && json?.version !== version.replace(/^v/, "")) {
    problems.push(`version is '${json?.version}', not '${version.replace(/^v/, "")}'`);
  }
  if (!String(json?.notes ?? "").trim()) problems.push("has no release notes");
  return problems;
}

/** Downloads each platform's file and checks its signature; resolves to the problems. */
export async function verifyLatest(json, pubkey, fetchBytes) {
  const problems = [];
  const files = new Map();
  for (const [platform, entry] of Object.entries(json?.platforms ?? {})) {
    if (!entry?.url || !entry?.signature) continue;
    if (!files.has(entry.url)) files.set(entry.url, fetchBytes(entry.url));
    let data;
    try {
      data = await files.get(entry.url);
    } catch (e) {
      problems.push(`'${platform}': download failed: ${e.message}`);
      continue;
    }
    const problem = signatureProblem(data, entry.signature, pubkey, json.version);
    if (problem) problems.push(`'${platform}': ${problem}`);
  }
  return problems;
}

async function download(url) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return Buffer.from(await res.arrayBuffer());
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const file = args.find((a, i) => !a.startsWith("--") && !["--version", "--dir"].includes(args[i - 1]));
  const opt = (name) => (args.indexOf(name) >= 0 ? args[args.indexOf(name) + 1] : undefined);
  const version = opt("--version");
  const dir = opt("--dir");
  const pubkey = process.env.TAURI_UPDATER_PUBKEY?.trim();
  if (!file || !pubkey) {
    console.error("usage: TAURI_UPDATER_PUBKEY=<pubkey> verify-update-sig.mjs <latest.json> [--version X.Y.Z]");
    process.exit(1);
  }
  const json = JSON.parse(readFileSync(file, "utf8"));
  const local = async (url) => readFileSync(join(dir, decodeURIComponent(url.split("/").pop())));
  const problems = [...latestProblems(json, { version }), ...(await verifyLatest(json, pubkey, dir ? local : download))];
  for (const problem of problems) console.error(`${file}: ${problem}`);
  if (!problems.length) console.log(`${file}: ${json.version} verified for ${Object.keys(json.platforms).join(", ")}`);
  process.exit(problems.length ? 1 : 0);
}
