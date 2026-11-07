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
      .permissions.filter((p) => p.startsWith("allow-"))
      .map((p) => p.slice("allow-".length).replaceAll("-", "_")),
  );

const sorted = (set) => [...set].sort();

test("the settings window may call exactly the commands its UI invokes", () => {
  const settings = invoked(uiSources.filter((f) => f !== "Simulator.jsx"));
  expect(sorted(settings)).toEqual([
    "app_version",
    "choose_app",
    "exit_wheel_override",
    "export_logs",
    "export_mappings",
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
    "save_config",
    "set_detailed_logs",
    "set_enabled",
    "set_start_at_login",
    "show_logs_file",
  ]);
  const granted = new Set([...allowed("default.json"), ...allowed("settings.json")]);
  expect(sorted(granted)).toEqual(sorted(settings));
});

test("the simulator window may call exactly the commands its UI invokes", () => {
  expect(sorted(allowed("simulator.json"))).toEqual(sorted(invoked(["Simulator.jsx"])));
});

test("build.rs declares exactly the commands the UI invokes", () => {
  const declared = [...read("../src-tauri/build.rs").matchAll(/^\s+"([a-z_]+)",$/gm)].map((m) => m[1]);
  expect(declared.sort()).toEqual(sorted(invoked(uiSources)));
});

test("every capability file is checked", () => {
  expect(readdirSync(new URL("../src-tauri/capabilities", import.meta.url)).sort()).toEqual([
    "default.json",
    "settings.json",
    "simulator.json",
  ]);
});
