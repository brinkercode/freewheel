// The Windows update e2e's local release server (scripts/e2e-update-windows.mjs):
// serves latest.json and the newer version's signed NSIS installer on
// 127.0.0.1, as GitHub's release download URLs would. Never used by a shipped
// build: only the e2e builds point their updater endpoint here.
//
// GET /latest.json answers by mode:
//   "fail"  500, the check fails (the error state, Try again)
//   "none"  204, no update (up to date)
//   "offer" 200, latest.json offering the bundle
// GET /<bundle name> streams the installer in throttled chunks, so the
// download's progress is visible. POST /__mode with the mode as the body
// switches the mode.
import { readFileSync, statSync } from "node:fs";
import { createServer } from "node:http";
import { basename } from "node:path";

export const MODES = ["fail", "none", "offer"];

/** latest.json for one Windows NSIS bundle, as tauri-action writes it. */
export function latestJson({ version, notes, signature, url, pubDate = new Date().toISOString() }) {
  return {
    version,
    notes,
    pub_date: pubDate,
    platforms: { "windows-x86_64": { signature: signature.trim(), url } },
  };
}

/** Splits `size` bytes into chunks of `chunk`: [start, end) pairs. */
export function chunks(size, chunk) {
  const out = [];
  for (let start = 0; start < size; start += chunk) out.push([start, Math.min(size, start + chunk)]);
  return out;
}

/**
 * Starts the server; resolves to { url, port, setMode, close, hits }.
 * `downloadMs` is about how long the bundle takes to stream.
 */
export function startUpdateServer({ bundle, signature, version, notes, port = 0, mode = "fail", downloadMs = 6000 }) {
  let current = mode;
  const hits = [];
  const name = basename(bundle);
  const size = statSync(bundle).size;
  const parts = chunks(size, Math.max(64 * 1024, Math.ceil(size / 40)));
  const pause = downloadMs / parts.length;
  const server = createServer((req, res) => {
    hits.push(`${req.method} ${req.url} (${current})`);
    if (req.method === "POST" && req.url === "/__mode") {
      let body = "";
      req.on("data", (d) => (body += d));
      req.on("end", () => {
        if (!MODES.includes(body.trim())) return res.writeHead(400).end("unknown mode");
        current = body.trim();
        res.writeHead(200).end(current);
      });
      return;
    }
    if (req.method !== "GET") return res.writeHead(405).end();
    if (req.url.split("?")[0] === "/latest.json") {
      if (current === "fail") return res.writeHead(500).end("simulated server error");
      if (current === "none") return res.writeHead(204).end();
      const base = `http://127.0.0.1:${server.address().port}`;
      const json = latestJson({ version, notes, signature, url: `${base}/${encodeURIComponent(name)}` });
      return res.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify(json));
    }
    if (decodeURIComponent(req.url.slice(1)) === name) {
      const data = readFileSync(bundle);
      res.writeHead(200, { "content-type": "application/octet-stream", "content-length": size });
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
