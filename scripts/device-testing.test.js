import { readFileSync } from "node:fs";
import { expect, test } from "vitest";

const doc = readFileSync(new URL("../docs/device-testing.md", import.meta.url), "utf8");

test("the live device checklist covers USB then Bluetooth, and says what to send back", () => {
  const usb = doc.indexOf("## 1. USB");
  const bt = doc.indexOf("## 2. Bluetooth");
  expect(usb).toBeGreaterThan(0);
  expect(bt).toBeGreaterThan(usb);
  for (const step of ["auth", "Shuttle", "Unplug the cable", "Sleep the PC", "Speed Editor sleeps", "Battery", "DaVinci Resolve"]) {
    expect(doc, step).toContain(step);
  }
  expect(doc).toContain("Help & backup → More → Device test");
  expect(doc).toContain("Copy diagnostics");
  expect(doc).toContain("Download logs");
  expect(doc).toContain("## 3. Send back");
});

test("the checklist's UI words match the Device test view", () => {
  const view = readFileSync(new URL("../src/DeviceTest.jsx", import.meta.url), "utf8");
  const backup = readFileSync(new URL("../src/Backup.jsx", import.meta.url), "utf8");
  for (const words of ["Copy diagnostics", "No battery report yet", "Next check in", "Held now", "Connected by"]) {
    expect(view, words).toContain(words);
    expect(doc, words).toContain(words);
  }
  expect(backup).toContain("Open device test");
  expect(doc).toContain("Open device test");
});
