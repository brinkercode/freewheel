import { readFileSync } from "node:fs";
import { renderToString } from "react-dom/server";
import { afterEach, expect, test } from "vitest";
import Backup, { backUp, downloadLogs, restore } from "./Backup.jsx";
import { exportMappings, importErrorText, importMappings } from "./settingsApi.js";

afterEach(() => delete globalThis.window);

test("the tab shows Download logs, Back up, Restore and, under More, detailed logs and the app opt-in, off by default", () => {
  const html = renderToString(<Backup />);
  for (const text of ["Download logs for support", "Logs never include what you type, your shortcuts, or the apps you open.", "Back up…", "Restore from a backup…"]) {
    expect(html).toContain(text);
  }
  expect(html).not.toContain("Let restored");
  const more = renderToString(<Backup initialMore detailedLogs={false} />);
  for (const text of ["Turn on detailed logs for troubleshooting", "Only if support asks. Turns itself off after a day.", "Let restored keys open apps"]) {
    expect(more).toContain(text);
  }
  expect(more).not.toContain('aria-checked="true"');
  expect(renderToString(<Backup initialMore initialAllowApps />)).toContain('aria-checked="true"');
});

test("Download logs names the saved file", async () => {
  expect(await downloadLogs(async () => "C:\\Users\\me\\Downloads\\freewheel-logs-2026-11-06.zip")).toBe(
    "Downloaded “freewheel-logs-2026-11-06.zip” to your Downloads folder.",
  );
  expect(await downloadLogs(async () => "/Users/me/Downloads/freewheel-logs.zip")).toBe("Downloaded “freewheel-logs.zip” to your Downloads folder.");
});

test("restore passes the opt-in only when it is ticked", async () => {
  const sent = [];
  const fake = async (allow) => (sent.push(allow), true);
  expect(await restore(false, fake)).toBe("Your keys are restored.");
  await restore(undefined, fake);
  await restore("yes", fake);
  await restore(true, fake);
  expect(sent).toEqual([false, false, false, true]);
  expect(await restore(false, async () => false)).toBeNull();
});

test("back up toasts only when a file was written", async () => {
  expect(await backUp(async () => true)).toBe("Your keys are backed up.");
  expect(await backUp(async () => false)).toBeNull();
});

test("export and import reach src-tauri by name", async () => {
  const calls = [];
  globalThis.window = { __TAURI__: { core: { invoke: async (cmd, args) => calls.push([cmd, args]) } } };
  await exportMappings();
  await importMappings(false);
  expect(calls).toEqual([
    ["export_mappings", undefined],
    ["import_mappings", { allowLaunch: false }],
  ]);
});

test("a rejected import says why in plain words", () => {
  expect(importErrorText({ kind: "VersionMismatch", found: 2, expected: 1 })).toBe(
    "This file is from a newer version of freewheel. Update freewheel, then try again.",
  );
  expect(importErrorText({ kind: "Malformed", message: "expected value at line 1" })).toBe(
    "This is not a freewheel mappings file: expected value at line 1",
  );
  expect(importErrorText({ kind: "UnknownKey", layer: "base", key: "Turbo" })).toContain("not a Speed Editor key");
});

test("the backup section loads no network fonts or icons", () => {
  const src = readFileSync(new URL("./Backup.jsx", import.meta.url), "utf8");
  expect(src).not.toMatch(/fonts\.googleapis|fonts\.gstatic|Material\sSymbols|https?:\/\//);
});
