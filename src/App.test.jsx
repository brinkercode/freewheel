import { existsSync, readdirSync, readFileSync } from "node:fs";
import { renderToString } from "react-dom/server";
import { expect, test } from "vitest";
import viteConfig from "../vite.config.js";
import App from "./App.jsx";

test("App renders the settings heading", () => {
  const done = { getItem: () => "1" };
  expect(renderToString(<App storage={done} />)).toContain("freewheel settings");
});

test("the UI uses the system font and loads no network fonts or icons", () => {
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  const sources = readdirSync(new URL(".", import.meta.url)).filter(
    (f) => /\.(jsx?|tsx?|css)$/.test(f) && !/\.test\./.test(f),
  );
  expect(sources).toContain("Status.jsx");
  expect(sources).toEqual(expect.arrayContaining(["FirstRun.jsx", "Permissions.jsx", "KeysTab.jsx"]));
  const ui = [html, ...sources.map((f) => readFileSync(new URL(`./${f}`, import.meta.url), "utf8"))].join("\n");
  expect(html).toMatch(/font-family:[^;]*system-ui/);
  expect(ui).not.toMatch(/fonts\.googleapis|fonts\.gstatic|Material\sSymbols|https?:\/\//);
});

test("vite outDir is the frontendDist of the Tauri config", () => {
  const tauri = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url)));
  expect(tauri.build.frontendDist).toBe(`../${viteConfig.build.outDir}`);
});

test("bundle icons are the tauri icon set macOS and Windows can bundle", () => {
  const tauri = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url)));
  const icons = tauri.bundle.icon;
  for (const name of ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"]) {
    expect(icons).toContain(name);
  }
  for (const icon of icons) {
    expect(existsSync(new URL(`../src-tauri/${icon}`, import.meta.url))).toBe(true);
  }
});
