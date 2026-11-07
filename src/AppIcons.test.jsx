// @vitest-environment jsdom
// The app picker's real app icons (src-tauri app_icons.rs): PNG data URLs in
// <img>, letter avatars until (or unless) an app has one.
import { cleanup, render, screen } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { expect, test, vi } from "vitest";
import { ActionForm, blank, iconSource } from "./ActionEditor.jsx";

const config = () => ({
  version: 1,
  settings: { enabled: true, start_at_login: false, resolve_process_name: "Resolve", low_battery_threshold: 15 },
  layers: [{ id: "base", name: "Main", bindings: {} }],
  base_layer: "base",
  jog: { default_mode: "Scroll", per_mode: {} },
  overrides: { system_volume_step: 0.02, app_volume_step: 0.02, brightness_step: 5 },
});

test("the picker shows each app's real icon (a PNG data URL) and a letter avatar for apps without one", async () => {
  const apps = [
    { name: "Notepad", path: "explorer.exe", args: ["C:/Start Menu/Notepad.lnk"] },
    { name: "Spotify", path: "explorer.exe", args: ["C:/Start Menu/Spotify.lnk"] },
    { name: "Tool", path: "C:/Tools/Tool.exe", args: [] },
  ];
  const png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==";
  const loadIcons = vi.fn(async () => ({ "C:/Start Menu/Notepad.lnk": png, "C:/Tools/Tool.exe": png }));
  render(<ActionForm action={blank("LaunchApp", config(), "base")} initialApps={apps} loadIcons={loadIcons} />);
  // Letters first, so the list never waits for the icons.
  expect(document.querySelectorAll(".app-row .app-initial")).toHaveLength(3);
  await vi.waitFor(() => expect(document.querySelectorAll(".app-row img.app-icon")).toHaveLength(2));
  expect(loadIcons).toHaveBeenCalledWith(["C:/Start Menu/Notepad.lnk", "C:/Start Menu/Spotify.lnk", "C:/Tools/Tool.exe"]);
  const row = (name) => screen.getByText(name).closest(".app-row");
  expect(row("Notepad").querySelector("img").getAttribute("src")).toBe(png);
  expect(row("Spotify").querySelector("img")).toBeNull();
  expect(row("Spotify").querySelector(".app-initial").textContent).toBe("S");
  expect(iconSource(apps[0])).toBe("C:/Start Menu/Notepad.lnk");
  expect(iconSource(apps[2])).toBe("C:/Tools/Tool.exe");
  cleanup();
});

test("a failed icon lookup keeps the letter avatars", async () => {
  const err = vi.spyOn(console, "error").mockImplementation(() => {});
  const apps = [{ name: "Notepad", path: "explorer.exe", args: ["C:/Start Menu/Notepad.lnk"] }];
  render(<ActionForm action={blank("LaunchApp", config(), "base")} initialApps={apps} loadIcons={() => Promise.reject("no shell")} />);
  await vi.waitFor(() => expect(err).toHaveBeenCalled());
  expect(document.querySelector(".app-row .app-initial").textContent).toBe("N");
  err.mockRestore();
  cleanup();
});

test("the icons come from app_icons, keyed by the file each app opens", () => {
  const api = readFileSync("src/settingsApi.js", "utf8");
  expect(api).toContain('export const appIcons = (paths) => invoke("app_icons", { paths });');
  const rust = readFileSync("src-tauri/src/app_icons.rs", "utf8");
  expect(rust).toContain("pub async fn app_icons(paths: Vec<String>) -> HashMap<String, String>");
  const css = readFileSync("src/styles/base.css", "utf8");
  expect(css).toMatch(/\.app-icon \{[^}]*width: 28px; height: 28px;/);
});
