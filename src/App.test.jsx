import { readFileSync } from "node:fs";
import { renderToString } from "react-dom/server";
import { expect, test } from "vitest";
import viteConfig from "../vite.config.js";
import App from "./App.jsx";

test("App renders the settings heading", () => {
  expect(renderToString(<App />)).toContain("freewheel settings");
});

test("the UI uses the system font and loads no network fonts or icons", () => {
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  const ui = [html, readFileSync(new URL("./App.jsx", import.meta.url), "utf8")].join("\n");
  expect(html).toMatch(/font-family:[^;]*system-ui/);
  expect(ui).not.toMatch(/fonts\.googleapis|fonts\.gstatic|Material\sSymbols|https?:\/\//);
});

test("vite outDir is the frontendDist of the Tauri config", () => {
  const tauri = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url)));
  expect(tauri.build.frontendDist).toBe(`../${viteConfig.build.outDir}`);
});
