// The browser harness (src/dev/mockTauri.js) answers every command the
// settings window invokes, and the Simulator's fake (src/dev/simFake.js)
// every command the Simulator invokes, so `npm run dev` logs no
// "mock: no command …" errors.
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test } from "vitest";
import { installMockTauri } from "./mockTauri.js";
import { createSimFake } from "./simFake.js";

const SRC = fileURLToPath(new URL("..", import.meta.url));

/** The src/ files the app ships (no tests, no dev harness). */
function appFiles(dir = SRC) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const full = join(dir, e.name);
    if (e.isDirectory()) return e.name === "dev" ? [] : appFiles(full);
    return /\.(js|jsx)$/.test(e.name) && !/\.test\.(js|jsx)$/.test(e.name) ? [full] : [];
  });
}

/** Every `invoke("name"` command in `files`. */
const commandsIn = (files) =>
  [...new Set(files.flatMap((f) => [...readFileSync(f, "utf8").matchAll(/\binvoke\(\s*["'`]([^"'`]+)["'`]/g)].map((m) => m[1])))].sort();

function memoryStorage() {
  const map = new Map();
  return { getItem: (k) => map.get(k) ?? null, setItem: (k, v) => map.set(k, String(v)), removeItem: (k) => map.delete(k) };
}

const isSimulator = (f) => /[\\/]Simulator\.jsx$/.test(f);

test("the source invokes its commands by name (so this check sees them all)", () => {
  const settings = commandsIn(appFiles().filter((f) => !isSimulator(f)));
  for (const cmd of ["get_config", "save_config", "get_status", "settings_ready", "update_check", "plugin:opener|open_url"]) expect(settings).toContain(cmd);
  expect(commandsIn(appFiles().filter(isSimulator))).toEqual(["sim_jump", "sim_notes", "sim_tray", "simulate"]);
});

test("the dev harness answers every command the settings window invokes", () => {
  const { commands } = installMockTauri({ localStorage: memoryStorage() });
  const missing = commandsIn(appFiles().filter((f) => !isSimulator(f))).filter((c) => !commands.includes(c));
  expect(missing).toEqual([]);
});

test("settings_ready is a no-op in the harness", async () => {
  const win = { localStorage: memoryStorage() };
  const { invoke } = installMockTauri(win);
  await expect(invoke("settings_ready")).resolves.toBeNull();
  await expect(win.__TAURI__.core.invoke("settings_ready")).resolves.toBeNull();
  await expect(invoke("no_such_command")).rejects.toThrow("mock: no command no_such_command");
});

test("the Simulator's fake answers every command the Simulator invokes", async () => {
  const fake = createSimFake();
  const args = { simulate: { control: { type: "SetEnabled", on: true } }, sim_jump: { to: "keys" }, sim_notes: { choice: "State", text: null }, sim_tray: { item: "State" } };
  // An unknown command throws; each of these answers.
  await expect(fake.invoke("no_such_command", {})).rejects.toThrow("no command no_such_command");
  for (const cmd of commandsIn(appFiles().filter(isSimulator))) await fake.invoke(cmd, args[cmd]);
});
