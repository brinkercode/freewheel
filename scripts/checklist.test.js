import { readFileSync } from "node:fs";
import { expect, test } from "vitest";

const readme = readFileSync(new URL("../README.md", import.meta.url), "utf8");
const checklist = readme.slice(readme.indexOf("## Manual checklist"));
const section = (title) => {
  const start = checklist.indexOf(`### ${title}\n`);
  expect(start, `no '### ${title}' section`).toBeGreaterThanOrEqual(0);
  const end = checklist.indexOf("\n### ", start + 1);
  return checklist.slice(start, end < 0 ? undefined : end);
};
const windows = () => section("Windows 11");
const mac = () => section("macOS 15 (Apple Silicon and Intel)");

test("[A12] the checklist has Windows 11 and macOS 15 sections and gates the draft on every item", () => {
  windows();
  mac();
  expect(checklist).toMatch(/draft release is published only when every item below passes/);
  expect(mac()).toMatch(/Apple Silicon[\s\S]*Intel/);
});

test.each([
  ["USB connect and auth", /USB.*authenticat/i],
  ["Bluetooth connect and auth", /Bluetooth.*authenticat/i],
  ["re-auth after the timeout", /re-auth/i],
  ["reconnect", /reconnect/i],
  ["On/Off release", /Off.*releas/i],
  ["Resolve hand-off with replug", /Resolve[\s\S]*replug/i],
  ["LEDs", /LED/],
  ["key injection debug command", /Shift\+A[\s\S]*scroll/i],
  ["Jog mode", /\bJog mode/],
  ["Scroll mode", /\bScroll mode/],
  ["Shuttle mode", /\bShuttle mode/],
  ["system volume override", /System volume/],
  ["per-app volume override", /app-volume override/i],
  ["each display's brightness", /Brightness override[\s\S]*each display|own error/],
  ["battery", /battery/i],
  ["start at login", /Start at login/],
  ["import/export", /Import[\s\S]*Export|export[\s\S]*import/i],
  ["update", /Update:/],
  ["[T3] tampered .sig rejected", /\[T3\].*tampered.*\.sig.*rejected/],
])("both OS sections cover %s", (_name, re) => {
  expect(windows()).toMatch(re);
  expect(mac()).toMatch(re);
});

test("the macOS section covers permissions", () => {
  expect(mac()).toMatch(/Input Monitoring/);
  expect(mac()).toMatch(/Accessibility/);
});

test("Windows covers SmartScreen on install and on update", () => {
  expect(windows()).toMatch(/SmartScreen.*install/i);
  expect(windows()).toMatch(/SmartScreen.*update/i);
});

test("the open spike items are listed, each tied to its real-device check", () => {
  const open = section("Open items the hardware spike left unconfirmed");
  expect(open).toMatch(/C9a.*Bluetooth on Windows 11/);
  expect(open).toMatch(/freewheel-m5-2.*Resolve re-acquire.*Windows/);
  expect(open).toMatch(/freewheel-m5-5.*macOS Input Monitoring/);
  expect(open).toMatch(/SmartScreen/);
  expect(windows()).toMatch(/\[C9a\]/);
  expect(mac()).toMatch(/\[C9a\]/);
  expect(windows()).toMatch(/\[freewheel-m5-2\]/);
  expect(mac()).toMatch(/\[freewheel-m5-5\]/);
});
