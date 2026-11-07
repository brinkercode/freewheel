#!/usr/bin/env node
// update-e2e's release picker: given the published release tags (one per
// line on stdin, or as arguments) prints `from=<tag>` and `to=<tag>`: the
// newest release and the one before it (or FROM when set). Prints
// `skip=true` when there is no older release to update from yet.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { compareVersions } from "../src/whatsNew.js";

const isVersionTag = (tag) => /^v\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(tag);

/** { from, to } or null: the newest release and the newest one below it (or `from`). */
export function pickReleases(tags, from = "") {
  const sorted = tags.filter(isVersionTag).sort((a, b) => compareVersions(b, a));
  const to = sorted[0];
  if (!to) return null;
  const older = from ? sorted.find((t) => t === from) : sorted.find((t) => compareVersions(t, to) < 0);
  if (!older || compareVersions(older, to) >= 0) return null;
  return { from: older, to };
}

/** The version of a tag, as the installed exe's file version reports it ("v0.1.1" -> "0.1.1"). */
export const versionOf = (tag) => tag.replace(/^v/, "").split("-")[0];

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const tags = (args.length ? args : readFileSync(0, "utf8").split(/\s+/)).filter(Boolean);
  const picked = pickReleases(tags, process.env.FROM?.trim() ?? "");
  if (!picked) console.log("skip=true");
  else {
    console.log(`from=${picked.from}`);
    console.log(`to=${picked.to}`);
    console.log(`from_version=${versionOf(picked.from)}`);
    console.log(`to_version=${versionOf(picked.to)}`);
  }
}
