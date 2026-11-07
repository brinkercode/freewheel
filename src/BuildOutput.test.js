// The owner's run of the settings window showed browser-default HTML: no
// stylesheet, and the pre-design <main><h1>freewheel settings</h1> shell.
// These checks fail if the built window could ship that way again: the
// production build must link the stylesheet, the stylesheet must have rules
// for the window's chrome and for every class the components use, and the
// dev-only Tauri mock must never be bundled.
import { mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "vite";
import { afterAll, beforeAll, expect, test } from "vitest";

const here = new URL(".", import.meta.url);
const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
let out;
let html;
let css;
let js;

beforeAll(async () => {
  out = mkdtempSync(join(process.env.TMPDIR ?? tmpdir(), "freewheel-build-"));
  // as `npm run build` does: vitest runs with NODE_ENV=test and mode "test"
  const env = process.env.NODE_ENV;
  process.env.NODE_ENV = "production";
  try {
    await build({ root: fileURLToPath(new URL("..", import.meta.url)), mode: "production", logLevel: "silent", build: { outDir: out, emptyOutDir: true } });
  } finally {
    process.env.NODE_ENV = env;
  }
  html = readFileSync(join(out, "index.html"), "utf8");
  const assets = readdirSync(join(out, "assets"));
  css = assets.filter((f) => f.endsWith(".css")).map((f) => readFileSync(join(out, "assets", f), "utf8")).join("\n");
  js = assets.filter((f) => f.endsWith(".js")).map((f) => readFileSync(join(out, "assets", f), "utf8")).join("\n");
}, 60_000);

afterAll(() => out && rmSync(out, { recursive: true, force: true }));

test("the built index.html links the stylesheet, and it has the tokens and the base rules", () => {
  expect(html).toMatch(/<link rel="stylesheet"[^>]*href="\/assets\/[^"]+\.css"/);
  expect(css).toContain("--accent:");
  expect(css).toContain("--bg:#f7f7f5");
  expect(css.length).toBeGreaterThan(20_000);
});

test("the toolbar, tabs, switch, notice, toast and pages have rules in the built stylesheet", () => {
  for (const selector of [
    ".app{",
    ".app-body{",
    ".toolbar{",
    ".tabs{",
    ".tab{",
    ".tab[aria-pressed=true]",
    ".status{",
    ".switch{",
    ".switch-knob{",
    ".notice{",
    ".toast{",
    ".tab-page{",
    ".card{",
    ".keys-tab{",
    ".side-panel{",
    ".device svg{",
    ".first-run{",
  ])
    expect(css, selector).toContain(selector);
});

test("every class the settings window's components use has a rule", () => {
  const rules = new Set([...read("./styles/base.css").replace(/url\([^)]*\)/g, "").matchAll(/\.([a-zA-Z][\w-]*)/g)].map((m) => m[1]));
  // hooks for tests and assistive tech only, never styled
  const hooks = new Set(["list-group", "button-list", "key-editor"]);
  const sources = readdirSync(here).filter((f) => f.endsWith(".jsx") && !f.includes(".test.") && f !== "Simulator.jsx");
  const unstyled = [];
  for (const f of sources)
    for (const [, names] of read(`./${f}`).matchAll(/className="([^"]+)"/g))
      for (const name of names.split(/\s+/)) if (!rules.has(name) && !hooks.has(name)) unstyled.push(`${f}: ${name}`);
  expect(unstyled).toEqual([]);
});

test("the pre-design shell is gone and the dev-only Tauri mock is not in the bundle", () => {
  expect(read("./App.jsx")).not.toMatch(/freewheel settings|<main>|<Status/);
  // (booleans: a failure must not print the whole bundle)
  for (const text of ["freewheel settings", "installMockTauri", "__fwPress", "mockTauri"]) expect(js.includes(text), text).toBe(false);
  // the production React build, not the development one
  expect(js.includes("react-stack-top-frame")).toBe(false);
  expect(readdirSync(join(out, "assets")).some((f) => /mock/i.test(f))).toBe(false);
});
