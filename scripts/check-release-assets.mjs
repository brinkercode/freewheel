#!/usr/bin/env node
// Checks a GitHub release holds every file the release ships before it is
// published: each package (scripts/portable-release.mjs PACKAGES), its .sig
// and latest.json, all uploaded and not empty, nothing else; and that
// latest.json is for this tag's version with every platform's url pointing at
// a file of this tag's release (check-latest-json.mjs checks the rest).
//
//   gh release view vX.Y.Z --json tagName,isDraft,assets > release.json
//   gh release download vX.Y.Z --pattern latest.json
//   node scripts/check-release-assets.mjs release.json latest.json --repo owner/name [--draft]
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { checkLatestJson } from "./check-latest-json.mjs";
import { PACKAGES } from "./portable-release.mjs";

/** The file names a release of `version` must hold. */
export function expectedAssets(version) {
  const packages = Object.values(PACKAGES).map((name) => name(version));
  return [...packages, ...packages.map((p) => `${p}.sig`), "latest.json"].sort();
}

/**
 * Returns the problems with `release` (gh release view --json tagName,isDraft,assets)
 * and its parsed `latest` latest.json for `repo` (empty when ready to publish).
 * With `draft`, the release must still be a draft.
 */
export function releaseAssetProblems({ release, latest, repo, draft = false }) {
  const tag = release?.tagName ?? "";
  const version = tag.replace(/^v/, "");
  const problems = [];
  if (!/^v\d+\.\d+\.\d+/.test(tag)) return [`'${tag}' is not a v* version tag`];
  if (draft && release.isDraft !== true) problems.push(`${tag} is already published, not a draft`);
  const assets = new Map((release.assets ?? []).map((a) => [a.name, a]));
  const expected = expectedAssets(version);
  for (const name of expected) {
    const a = assets.get(name);
    if (!a) problems.push(`missing ${name}`);
    else if (a.state && a.state !== "uploaded") problems.push(`${name} is ${a.state}, not uploaded`);
    else if (!(a.size > 0)) problems.push(`${name} is empty`);
  }
  for (const name of assets.keys()) if (!expected.includes(name)) problems.push(`unexpected file ${name}`);
  if (latest?.version !== version) problems.push(`latest.json is version ${latest?.version}, not ${version}`);
  problems.push(...checkLatestJson(latest).map((p) => `latest.json: ${p}`));
  const base = `https://github.com/${repo}/releases/download/${tag}/`;
  for (const [key, p] of Object.entries(latest?.platforms ?? {})) {
    const url = p?.url ?? "";
    if (!url.startsWith(base)) problems.push(`latest.json '${key}' does not point at ${tag}: ${url}`);
    else if (!assets.has(url.slice(base.length))) problems.push(`latest.json '${key}' points at a file the release does not hold: ${url}`);
  }
  return problems;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const draft = args.includes("--draft");
  const repoAt = args.indexOf("--repo");
  const repo = repoAt >= 0 ? args[repoAt + 1] : undefined;
  const [releaseFile, latestFile] = args.filter((a, i) => !a.startsWith("--") && i !== repoAt + 1);
  if (!releaseFile || !latestFile || !repo) {
    console.error("usage: check-release-assets.mjs <release.json> <latest.json> --repo owner/name [--draft]");
    process.exit(1);
  }
  const release = JSON.parse(readFileSync(releaseFile, "utf8"));
  const problems = releaseAssetProblems({ release, latest: JSON.parse(readFileSync(latestFile, "utf8")), repo, draft });
  for (const problem of problems) console.error(`${release.tagName}: ${problem}`);
  if (!problems.length) console.log(`${release.tagName}: all ${expectedAssets(release.tagName.replace(/^v/, "")).length} files are there and latest.json points at them`);
  process.exit(problems.length ? 1 : 0);
}
