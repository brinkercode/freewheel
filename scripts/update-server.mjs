// The Windows update e2e's local release server (scripts/e2e-update-windows.mjs):
// serves a release's latest.json (scripts/portable-release.mjs) and its
// release file (the exe) on 127.0.0.1, as GitHub's release download URLs would.
// Never used by a shipped build: only e2e builds (the `e2e` cargo feature)
// read FREEWHEEL_E2E_UPDATE_URL and allow plain http to 127.0.0.1.
//
// GET /latest.json answers by mode:
//   "fail"      500, the check fails (the error state, Try again)
//   "missing"   404, the latest release has no latest.json: the check
//               fails too (Try again later), it is not up to date
//   "none"      200, the manifest for `current` (default 0.0.0): nothing
//               newer (up to date)
//   "offer"     200, the manifest, its package url pointing here
//   "tampered"  200, the same manifest, but the package served has one
//               byte flipped (its signature no longer matches)
// GET /<package name> streams the package in throttled chunks, so the
// download's progress is visible.
// GET /repos/brinkercode/freewheel/releases/tags/v<version> answers as
// GitHub's releases API does (What's new's notes, src-tauri
// release_notes.rs; e2e builds read FREEWHEEL_E2E_RELEASES_API): 200 with
// { tag_name, body } for a version in `releases`, else 404.
import { readFileSync } from "node:fs";
import { createServer } from "node:http";
import { basename } from "node:path";

export const MODES = ["fail", "missing", "none", "offer", "tampered"];

/** The releases API path of a tag (GitHub's, as release_notes.rs asks it). */
export const RELEASE_PATH = /^\/repos\/brinkercode\/freewheel\/releases\/tags\/v(\d+\.\d+\.\d+[^/?]*)$/;

/** `manifest` with every platform's url moved to `base` (same file names). */
export function localManifest(manifest, base) {
  const platforms = Object.fromEntries(
    Object.entries(manifest.platforms).map(([key, p]) => [key, { ...p, url: `${base}/${basename(new URL(p.url).pathname)}` }]),
  );
  return { ...manifest, platforms };
}

/** A copy of `data` with the byte in the middle flipped. */
export function tamper(data) {
  const copy = Buffer.from(data);
  copy[Math.floor(copy.length / 2)] ^= 0xff;
  return copy;
}

/** Splits `size` bytes into chunks of `chunk`: [start, end) pairs. */
export function chunks(size, chunk) {
  const out = [];
  for (let start = 0; start < size; start += chunk) out.push([start, Math.min(size, start + chunk)]);
  return out;
}

/**
 * Starts the server; resolves to { url, port, setMode, mode, close, hits }.
 * `manifest` is the parsed latest.json, `package` the path of the package it
 * names; `downloadMs` is about how long the package takes to stream;
 * `releases` maps a version to its GitHub release body; `current` is the
 * version mode "none" serves (the app's own: up to date).
 */
export function startUpdateServer({ manifest, package: pkg, port = 0, mode = "fail", downloadMs = 6000, releases = {}, current: currentVersion = "0.0.0" }) {
  let current = mode;
  const hits = [];
  const name = basename(pkg);
  const good = readFileSync(pkg);
  const bad = tamper(good);
  const parts = chunks(good.length, Math.max(64 * 1024, Math.ceil(good.length / 40)));
  const pause = downloadMs / parts.length;
  const server = createServer((req, res) => {
    hits.push(`${req.method} ${req.url} (${current})`);
    if (req.method !== "GET") return res.writeHead(405).end();
    if (req.url.split("?")[0] === "/latest.json") {
      if (current === "fail") return res.writeHead(500).end("simulated server error");
      if (current === "missing") return res.writeHead(404).end("Not Found");
      const local = localManifest(manifest, `http://127.0.0.1:${server.address().port}`);
      const json = current === "none" ? { ...local, version: currentVersion } : local;
      return res.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify(json));
    }
    const tag = req.url.split("?")[0].match(RELEASE_PATH);
    if (tag) {
      const body = releases[tag[1]];
      const json = (status, value) => res.writeHead(status, { "content-type": "application/json; charset=utf-8" }).end(JSON.stringify(value));
      if (typeof body !== "string") return json(404, { message: "Not Found", status: "404" });
      if (req.headers.accept !== "application/vnd.github+json") return json(415, { message: "the releases API wants Accept: application/vnd.github+json" });
      return json(200, { tag_name: `v${tag[1]}`, name: `v${tag[1]}`, draft: false, prerelease: false, body });
    }
    if (decodeURIComponent(req.url.slice(1)) === name) {
      const data = current === "tampered" ? bad : good;
      res.writeHead(200, { "content-type": "application/octet-stream", "content-length": data.length });
      let i = 0;
      const next = () => {
        if (i >= parts.length) return res.end();
        const [a, b] = parts[i++];
        res.write(data.subarray(a, b));
        setTimeout(next, pause);
      };
      return next();
    }
    res.writeHead(404).end();
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", () => {
      const p = server.address().port;
      resolve({
        port: p,
        url: `http://127.0.0.1:${p}`,
        hits,
        mode: () => current,
        setMode: (m) => {
          if (!MODES.includes(m)) throw new Error(`unknown mode ${m}`);
          current = m;
        },
        close: () => new Promise((r) => server.close(() => r())),
      });
    });
  });
}
