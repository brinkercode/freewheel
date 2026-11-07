#!/usr/bin/env node
// Screenshots of the settings window in a plain browser, for comparing with
// the design handoff's prototype. Needs `npm run dev` running (the dev-only
// Tauri mock, src/dev/mockTauri.js, fakes the backend) and playwright-core:
//
//   NODE_PATH=<dir with playwright-core> CHROME=<chromium binary> \
//     node scripts/shots.mjs [out-dir] [base-url]
//
// The viewport is the prototype window's content: 1000x640 minus its 1px
// border and 36px title bar (Tauri draws the native one).
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";

const require = createRequire(import.meta.url);
const { chromium } = require("playwright-core");

const out = process.argv[2] ?? "shots";
const base = process.argv[3] ?? "http://127.0.0.1:5173/";
const executablePath = process.env.CHROME ?? `chromium`;
fs.mkdirSync(out, { recursive: true });

const tab = (name) => async (p) => {
  await p.getByRole("button", { name, exact: true }).first().click();
};
const STATES = [
  ["01-keys", "", null],
  ["02-wheel", "", tab("Wheel")],
  ["03-davinci", "", tab("DaVinci Resolve")],
  ["04-general", "", tab("General")],
  ["05-help", "", tab("Help & backup")],
  ["06-keys-list", "", async (p) => p.getByRole("button", { name: "List", exact: true }).click()],
  ["07-keypanel", "", async (p) => p.locator('[data-binding="SmartInsrt"]').click()],
  ["08-keypanel-detail", "", async (p) => {
    await p.locator('[data-binding="Trans"]').click();
    await p.getByText("Turn a key on and off", { exact: true }).click();
  }],
  ["09-firstrun", "?firstrun=1", null],
  ["10-firstrun-found", "?firstrun=1", async (p) => p.getByRole("button", { name: "Get started" }).click()],
  ["11-off", "?off=1", null],
  ["12-toast", "", async (p) => p.getByRole("switch", { name: /Turn freewheel on or off/ }).click()],
  ["13-help-more", "", async (p) => {
    await tab("Help & backup")(p);
    await p.getByRole("button", { name: "Download logs" }).click();
    await p.getByRole("button", { name: /^More/ }).click();
  }],
  ["14-pages", "", async (p) => p.getByRole("tab", { name: "Music" }).click()],
  ["15-keypanel-knob", "", async (p) => {
    await p.locator('[data-binding="FullView"]').click();
    await p.getByText("Turn the wheel into…", { exact: true }).click();
  }],
  ["16-keypanel-pages", "", async (p) => {
    await p.locator('[data-binding="Esc"]').click();
    await p.getByText("Switch to another set of keys", { exact: true }).click();
  }],
  ["17-keypanel-app", "", async (p) => {
    await p.locator('[data-binding="Cam1"]').click();
    await p.getByText("Open an app or website", { exact: true }).click();
  }],
  ["18-wheel-keys", "", async (p) => {
    await tab("Wheel")(p);
    await p.getByRole("button", { name: /^SHTL/ }).click();
  }],
  ["19-resolve-more", "", async (p) => {
    await tab("DaVinci Resolve")(p);
    await p.getByRole("button", { name: /^More/ }).click();
  }],
  ["20-back-key", "", async (p) => {
    await p.getByRole("tab", { name: "Music" }).click();
    await p.locator('[data-binding="Esc"]').click();
  }],
  ["21-looking", "?conn=none", null],
  ["22-permission", "?perm=missing", async (p) => p.getByRole("button", { name: "Fix this" }).click()],
  ["23-whats-new", "?updated=0.1.1", null],
  ["24-whats-new-notes", "?updated=0.1.1", async (p) => p.getByRole("button", { name: /^What’s new in v/ }).click()],
];

const only = process.env.ONLY ? new RegExp(process.env.ONLY) : null;
const browser = await chromium.launch({ executablePath, headless: true });
for (const scheme of ["light", "dark"]) {
  for (const [name, query, act] of STATES) {
    if (only && !only.test(name)) continue;
    const page = await browser.newPage({ viewport: { width: 998, height: 602 }, colorScheme: scheme });
    page.on("pageerror", (e) => console.error(name, "pageerror:", e.message));
    await page.goto(base + query);
    await page.waitForTimeout(600);
    if (act) await act(page).catch((e) => console.error(name, "step failed:", e.message.split("\n")[0]));
    await page.waitForTimeout(500);
    const file = path.join(out, `${scheme}-${name}.png`);
    await page.screenshot({ path: file });
    await page.close();
  }
}
await browser.close();
console.log(fs.readdirSync(out).filter((f) => f.endsWith(".png")).join(" "));
