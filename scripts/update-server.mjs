// The Windows update e2e's local release server (scripts/e2e-update-windows.mjs):
// serves a release's latest.json (scripts/portable-release.mjs) and its
// portable package on 127.0.0.1, as GitHub's release download URLs would.
// Never used by a shipped build: only e2e builds (the `e2e` cargo feature)
// read FREEWHEEL_E2E_UPDATE_URL and allow plain http to 127.0.0.1.
//
// GET /latest.json answers by mode:
//   "fail"      500, the check fails (the error state, Try again)
//   "none"      204, no update (up to date)
//   "offer"     200, the manifest, its package url pointing here
//   "tampered"  200, the same manifest, but the package served has one
//               byte flipped (its signature no longer matches)
// GET /<package name> streams the package in throttled chunks, so the
// download's progress is visible.
import { readFileSync } from "node:fs";
import { createServer } from "node:http";
import { basename } from "node:path";

export const MODES = ["fail", "none", "offer", "tampered"];

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
 * names; `downloadMs` is about how long the package takes to stream.
 */
export function startUpdateServer({ manifest, package: pkg, port = 0, mode = "fail", downloadMs = 6000 }) {
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
      if (current === "none") return res.writeHead(204).end();
      const json = localManifest(manifest, `http://127.0.0.1:${server.address().port}`);
      return res.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify(json));
    }
    if (decodeURIComponent(req.url.slice(1)) === name) {
      const data = current === "tampered" ? bad : good;
      res.writeHead(200, { "content-type": "application/zip", "content-length": data.length });
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
