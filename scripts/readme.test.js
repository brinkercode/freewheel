import { existsSync, readFileSync } from "node:fs";
import { expect, test } from "vitest";

const readme = readFileSync(new URL("../README.md", import.meta.url), "utf8");
const images = [...readme.matchAll(/(?:src|srcset)="(docs\/images\/[^"]+)"/g)].map((m) => m[1]);

test("every screenshot the README shows is committed", () => {
  expect(images.length).toBeGreaterThan(0);
  for (const img of images) expect(existsSync(new URL(`../${img}`, import.meta.url)), img).toBe(true);
});

test("the hero and the simulator have a light and a dark screenshot", () => {
  for (const screen of ["keys", "simulator"]) {
    expect(readme).toContain(`<source media="(prefers-color-scheme: dark)" srcset="docs/images/dark-${screen}.png">`);
    expect(readme).toContain(`src="docs/images/light-${screen}.png"`);
  }
});

test("the README names the sections a user looks for", () => {
  for (const h of ["What it does", "Download and run", "Updates", "Try it without a Speed Editor", "Screens", "Back up and restore",
    "Troubleshooting", "Building from source", "Releasing", "Privacy", "License and credits"])
    expect(readme).toMatch(new RegExp(`^## ${h}$`, "m"));
});

test("the README's privacy section names the update manifest URL the app uses", () => {
  const src = readFileSync(new URL("../src-tauri/src/selfupdate.rs", import.meta.url), "utf8");
  const url = src.match(/MANIFEST_URL: &str =\s*"([^"]+)"/)[1];
  expect(readme.split(/^## /m).find((s) => s.startsWith("Privacy"))).toContain(url);
});

test("the README's relative links point at files that exist", () => {
  for (const [, link] of readme.matchAll(/\]\(((?!https?:|#)[^)]+)\)/g))
    expect(existsSync(new URL(`../${link}`, import.meta.url)), link).toBe(true);
});
