import { readFileSync } from "node:fs";
import { renderToString } from "react-dom/server";
import { afterEach, expect, test } from "vitest";
import Backup, { backUp, restore } from "./Backup.jsx";
import { exportMappings, importErrorText, importMappings } from "./settingsApi.js";

afterEach(() => delete globalThis.window);

test("the tab shows Back up, Restore and the app opt-in, off by default", () => {
  const html = renderToString(<Backup />);
  for (const text of ["Back up…", "Restore from a backup…", "Let restored", "keys open apps"]) {
    expect(html).toContain(text);
  }
  expect(html).not.toContain("checked");
  expect(renderToString(<Backup initialAllowApps />)).toContain('checked=""');
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
