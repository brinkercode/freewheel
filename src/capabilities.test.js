// T8: each window's capability file allows exactly the app commands its UI
// invokes, and build.rs declares exactly those commands.
import { readdirSync, readFileSync } from "node:fs";
import { expect, test } from "vitest";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");

const uiSources = readdirSync(new URL(".", import.meta.url)).filter(
  (f) => /\.(jsx?|tsx?)$/.test(f) && !/\.test\./.test(f),
);

const invoked = (files) =>
  new Set(files.flatMap((f) => [...read(`./${f}`).matchAll(/invoke\(\s*"([a-z_]+)"/g)].map((m) => m[1])));

const allowed = (capability) =>
  new Set(
    JSON.parse(read(`../src-tauri/capabilities/${capability}`))
      .permissions.filter((p) => typeof p === "string" && p.startsWith("allow-"))
      .map((p) => p.slice("allow-".length).replaceAll("-", "_")),
  );

const sorted = (set) => [...set].sort();

test("the settings window may call exactly the commands its UI invokes", () => {
  const settings = invoked(uiSources.filter((f) => f !== "Simulator.jsx"));
  expect(sorted(settings)).toEqual([
    "app_icons",
    "app_version",
    "choose_app",
    "desktop_hints",
    "exit_wheel_override",
    "export_logs",
    "export_mappings",
    "front_app_support",
    "get_config",
    "get_device_diagnostics",
    "get_permissions",
    "get_start_at_login",
    "get_status",
    "import_mappings",
    "list_apps",
    "open_bluetooth_settings",
    "open_permission_pane",
    "open_release_page",
    "profile_app",
    "release_notes",
    "save_config",
    "set_detailed_logs",
    "set_enabled",
    "set_start_at_login",
    "settings_ready",
    "show_logs_file",
    "tray_update_state",
    "update_check",
    "update_download",
    "update_install",
  ]);
  const granted = allowed("settings.json");
  expect(sorted(granted)).toEqual(sorted(settings));
});

test("the simulator window may call exactly the commands its UI invokes", () => {
  expect(sorted(allowed("simulator.json"))).toEqual(sorted(invoked(["Simulator.jsx"])));
});

test("build.rs declares exactly the commands the UI invokes", () => {
  const declared = [...read("../src-tauri/build.rs").matchAll(/^\s+"([a-z_]+)",$/gm)].map((m) => m[1]);
  expect(declared.sort()).toEqual(sorted(invoked(uiSources)));
});

test("links open in the browser: What's new's notes to GitHub, the profile editor's 'Browse all icons' to the icon catalogue", () => {
  const settings = JSON.parse(read("../src-tauri/capabilities/settings.json")).permissions;
  const scoped = settings.filter((p) => typeof p !== "string");
  expect(scoped).toEqual([{ identifier: "opener:allow-open-url", allow: [{ url: "https://github.com/*" }, { url: "https://fonts.google.com/icons*" }] }]);
  expect(settings.filter((p) => typeof p === "string" && p.startsWith("opener:"))).toEqual([]);
  expect(JSON.parse(read("../src-tauri/capabilities/simulator.json")).permissions.some((p) => String(p.identifier ?? p).startsWith("opener:"))).toBe(false);
  expect(read("./notesMarkdown.js")).toContain('export const LINK_PREFIX = "https://github.com/";');
});

test("every capability file is checked", () => {
  expect(readdirSync(new URL("../src-tauri/capabilities", import.meta.url)).sort()).toEqual([
    "settings.json",
    "simulator.json",
  ]);
});
