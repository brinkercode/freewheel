import { existsSync, readFileSync } from "node:fs";
import { expect, test } from "vitest";

const readme = readFileSync(new URL("../README.md", import.meta.url), "utf8");
const updates = readFileSync(new URL("../docs/updates.md", import.meta.url), "utf8");
const images = [...readme.matchAll(/(?:src|srcset)="(docs\/images\/[^"]+)"/g)].map((m) => m[1]);

test("every screenshot the README shows is committed", () => {
  expect(images.length).toBeGreaterThan(0);
  for (const img of images) expect(existsSync(new URL(`../${img}`, import.meta.url)), img).toBe(true);
});

test("the hero has a light and a dark screenshot", () => {
  expect(readme).toContain(`<source media="(prefers-color-scheme: dark)" srcset="docs/images/dark-keys.png">`);
  expect(readme).toContain(`src="docs/images/light-keys.png"`);
});

test("the README links the mappings format and the updates, backups and privacy page", () => {
  expect(readme).toContain("[SCHEMA.md](SCHEMA.md)");
  expect(readme).toContain("[docs/updates.md](docs/updates.md)");
  expect(updates.split(/^## /m).find((s) => s.startsWith("Back up and restore"))).toContain("[SCHEMA.md](../SCHEMA.md)");
});

test("the README opens with the light and dark banner, and the tagline is its alt text", () => {
  expect(readme).toContain('srcset=".github/assets/freewheel-logo-dark.svg"');
  expect(readme).toContain('src=".github/assets/freewheel-logo-light.svg"');
  expect(readme).toContain("No telemetry, no accounts, no analytics, always portable");
  for (const f of ["light", "dark"]) {
    const svg = readFileSync(new URL(`../.github/assets/freewheel-logo-${f}.svg`, import.meta.url), "utf8");
    expect(svg).not.toMatch(/<text|font-family|<image/);
  }
});

test("the README is for users: the simulator is a development aid, in docs/development.md", () => {
  expect(readme).not.toMatch(/simulat/i);
  expect(readme).toContain("[docs/development.md](docs/development.md)");
  const dev = readFileSync(new URL("../docs/development.md", import.meta.url), "utf8");
  expect(dev).toContain("npx tauri dev -- -- --simulate");
  expect(dev).toContain("freewheel-simulate.cmd");
});

test("the README names the sections a user looks for", () => {
  const sections = [...readme.matchAll(/^## (.+)$/gm)].map((m) => m[1]);
  expect(sections).toEqual(["What it does", "Download and run", "Screens", "Troubleshooting"]);
});

test("the privacy section (docs/updates.md) names the exact URLs the app requests", () => {
  const src = readFileSync(new URL("../src-tauri/src/selfupdate.rs", import.meta.url), "utf8");
  const url = src.match(/MANIFEST_URL: &str =\s*"([^"]+)"/)[1];
  const privacy = updates.split(/^## /m).find((s) => s.startsWith("Privacy"));
  expect(privacy).toContain(url);
  // What's new's notes (release_notes.rs): the releases API's tag URL.
  const notes = readFileSync(new URL("../src-tauri/src/release_notes.rs", import.meta.url), "utf8");
  const api = notes.match(/RELEASES_API: &str = "([^"]+)"/)[1];
  expect(notes).toContain('format!("{base}/tags/v{v}")');
  expect(privacy).toContain(`${api}/tags/v<version>`);
});

test("every screenshot docs/development.md shows is committed", () => {
  const dev = readFileSync(new URL("../docs/development.md", import.meta.url), "utf8");
  const shots = [...dev.matchAll(/(?:src|srcset)="(images\/[^"]+)"/g)].map((m) => m[1]);
  expect(shots.length).toBeGreaterThan(0);
  for (const img of shots) expect(existsSync(new URL(`../docs/${img}`, import.meta.url)), img).toBe(true);
});

test("the README's relative links point at files that exist", () => {
  // SCHEMA.md (the mappings format) lands in its own change; until then it may be missing.
  for (const [, link] of readme.matchAll(/\]\(((?!https?:|#)[^)]+)\)/g))
    if (link !== "SCHEMA.md") expect(existsSync(new URL(`../${link}`, import.meta.url)), link).toBe(true);
});

test("the header's badges: the latest release's build, and the license, with no external badge service", () => {
  const header = readme.slice(0, readme.indexOf("## What it does"));
  expect(header).toContain('<a href="https://github.com/brinkercode/freewheel/releases/latest"><img src="https://github.com/brinkercode/freewheel/actions/workflows/release.yml/badge.svg" alt="Release build"></a>');
  expect(header).toContain('<a href="LICENSE"><img src=".github/assets/license-mit.svg" alt="License: MIT"></a>');
  expect(header).not.toMatch(/shields\.io|workflows\/ci\.yml/);
  expect(existsSync(new URL("../.github/assets/license-mit.svg", import.meta.url))).toBe(true);
  // The release workflow runs only for version tags, so its badge is the latest release's build.
  const release = readFileSync(new URL("../.github/workflows/release.yml", import.meta.url), "utf8");
  expect(release).toMatch(/^on:\r?\n  push:\r?\n    tags: \["v\*"\]\r?\n  workflow_dispatch:/m);
});

test("the README's hero shows the profile picker", () => {
  expect(readme).toMatch(/alt="freewheel's Keys tab: the profile picker[^"]*"/);
});
