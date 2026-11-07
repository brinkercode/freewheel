import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { renderToString } from "react-dom/server";
import { expect, test, vi } from "vitest";
import App from "./App.jsx";
import UpdateBanner from "./UpdateBanner.jsx";
import { download, restart, startupCheck } from "./updater.js";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const conf = JSON.parse(read("../src-tauri/tauri.conf.json"));

function fakeUpdate() {
  return {
    version: "0.2.0",
    download: vi.fn(async (onEvent) => {
      onEvent({ event: "Started", data: { contentLength: 200 } });
      onEvent({ event: "Progress", data: { chunkLength: 100 } });
      onEvent({ event: "Progress", data: { chunkLength: 100 } });
      onEvent({ event: "Finished" });
    }),
    install: vi.fn(async () => {}),
  };
}

test("[A11][T3] the updater commits no pubkey (release builds inject it), https endpoints only and no insecure transport", () => {
  const updater = conf.plugins.updater;
  expect(updater.pubkey ?? "").toBe("");
  expect(updater.endpoints.length).toBeGreaterThan(0);
  for (const url of updater.endpoints) expect(url).toMatch(/^https:\/\/github\.com\/[^/]+\/freewheel\/releases\//);
  expect(read("../src-tauri/tauri.conf.json")).not.toContain("dangerousInsecureTransportProtocol");
});

test("[T7] tauri.conf.json holds no key of any kind", () => {
  expect(read("../src-tauri/tauri.conf.json")).not.toMatch(/private|secret|password/i);
});

test("Windows installs updates with the passive NSIS mode", () => {
  expect(conf.plugins.updater.windows.installMode).toBe("passive");
});

test("the webview may only check, download, install and restart", () => {
  const cap = JSON.parse(read("../src-tauri/capabilities/default.json"));
  expect(cap.permissions).toEqual(["updater:default", "process:allow-restart"]);
});

test("'Update automatically' is on by default: a newer version starts at downloading, then ready", async () => {
  const update = fakeUpdate();
  const states = [];
  await startupCheck({ check: async () => update, onState: (s) => states.push(s) });
  expect(states[0]).toEqual({ status: "downloading", version: "0.2.0", progress: 0 });
  expect(states.map((s) => s.progress).filter((p) => p !== undefined)).toEqual([0, 0, 50, 100, 100]);
  expect(states.at(-1)).toEqual({ status: "ready", version: "0.2.0" });
});

test("with auto-update off a newer version is offered as available", async () => {
  const update = fakeUpdate();
  const onState = vi.fn();
  expect(await startupCheck({ check: async () => update, autoUpdate: false, onState })).toBe(update);
  expect(onState.mock.calls).toEqual([[{ status: "available", version: "0.2.0" }]]);
  expect(update.download).not.toHaveBeenCalled();
});

test("no newer version shows no banner", async () => {
  const onState = vi.fn();
  expect(await startupCheck({ check: async () => null, onState })).toBeNull();
  expect(onState).not.toHaveBeenCalled();
});

test("a failed check is logged and does not throw", async () => {
  const log = vi.fn();
  const onState = vi.fn();
  const err = new Error("offline");
  await expect(startupCheck({ check: async () => Promise.reject(err), onState, log })).resolves.toBeNull();
  expect(log).toHaveBeenCalledWith("update check failed", err);
  expect(onState).not.toHaveBeenCalled();
});

test("a failed download is logged and goes back to available", async () => {
  const log = vi.fn();
  const states = [];
  const update = { version: "0.2.0", download: async () => Promise.reject(new Error("reset")) };
  await download(update, (s) => states.push(s), log);
  expect(log).toHaveBeenCalledWith("update download failed", expect.any(Error));
  expect(states.at(-1)).toEqual({ status: "available", version: "0.2.0" });
});

test("Restart to update shows restarting, installs, then relaunches", async () => {
  const update = fakeUpdate();
  const relaunch = vi.fn(async () => {});
  const onState = vi.fn();
  await restart(update, relaunch, onState);
  expect(onState).toHaveBeenCalledWith({ status: "restarting", version: "0.2.0" });
  expect(update.install).toHaveBeenCalled();
  expect(relaunch).toHaveBeenCalled();
});

const banner = (update) => renderToString(<UpdateBanner update={update} />);

test("the banner shows each state", () => {
  expect(banner(null)).toBe("");
  const available = banner({ status: "available", version: "0.2.0" });
  expect(available).toContain("Download and install");
  expect(available).toContain("Later");
  const downloading = banner({ status: "downloading", version: "0.2.0", progress: 40 });
  expect(downloading).toMatch(/<progress[^>]*value="40"/);
  const ready = banner({ status: "ready", version: "0.2.0" });
  expect(ready).toContain("Restart to update");
  expect(ready).toContain("Later");
  expect(banner({ status: "restarting", version: "0.2.0" })).toContain("Restarting");
});

test("the app renders without an update banner before the check", () => {
  expect(renderToString(<App />)).not.toContain("update-banner");
});

// Assembled from fragments so this file does not itself trip the repo-wide forbidden-string scan.
const networkAssets = new RegExp(
  [
    ["fonts\\.google", "apis"].join(""),
    ["fonts\\.g", "static"].join(""),
    ["Material", " Symbols"].join(""),
    ["material", "-symbols"].join(""),
    "<link[^>]+https?:",
  ].join("|"),
  "i",
);

test("no network fonts or icons; the UI uses the system font", () => {
  // src/ has subfolders (styles/, assets/): scan every file in them too.
  const files = ["../index.html", ...readdirSync(new URL(".", import.meta.url), { recursive: true, withFileTypes: true })
    .filter((d) => d.isFile() && !d.name.includes(".test."))
    .map((d) => `./${relative(fileURLToPath(new URL(".", import.meta.url)), join(d.parentPath, d.name)).replaceAll("\\", "/")}`)];
  for (const f of files) {
    expect(read(f)).not.toMatch(networkAssets);
  }
  expect(read("../index.html")).toMatch(/font-family: system-ui/);
});
