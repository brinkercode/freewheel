import { afterEach, expect, test } from "vitest";
import {
  getConfig,
  getStatus,
  onKeyPressed,
  onStatus,
  saveConfig,
  saveErrorText,
} from "./settingsApi.js";

function fakeTauri() {
  const calls = [];
  const listeners = {};
  globalThis.window = {
    __TAURI__: {
      core: { invoke: async (cmd, args) => (calls.push([cmd, args]), cmd) },
      event: {
        listen: async (name, handler) => {
          listeners[name] = handler;
          return () => delete listeners[name];
        },
      },
    },
  };
  return { calls, listeners };
}

afterEach(() => delete globalThis.window);

test("the config and status commands reach src-tauri by name", async () => {
  const { calls } = fakeTauri();
  const config = { version: 1 };
  await getConfig();
  await saveConfig(config);
  await getStatus();
  expect(calls).toEqual([
    ["get_config", undefined],
    ["save_config", { config }],
    ["get_status", undefined],
  ]);
});

test("status and key-pressed events hand their payloads over", async () => {
  const { listeners } = fakeTauri();
  const statuses = [];
  const presses = [];
  const stop = await onStatus((s) => statuses.push(s));
  await onKeyPressed((p) => presses.push(p));
  listeners.status({ payload: { connection: "Connected", layer: "base" } });
  listeners["key-pressed"]({ payload: { key: "Cut", label: "CUT" } });
  expect(statuses).toEqual([{ connection: "Connected", layer: "base" }]);
  expect(presses).toEqual([{ key: "Cut", label: "CUT" }]);
  stop();
  expect(listeners.status).toBeUndefined();
});

test("every typed save error has its own message", () => {
  expect(saveErrorText({ kind: "VersionMismatch", found: 2, expected: 1 })).toBe(
    "Config version 2 is not 1.",
  );
  expect(saveErrorText({ kind: "UnknownField", path: "settings.colour" })).toBe(
    "Unknown setting settings.colour.",
  );
  expect(saveErrorText({ kind: "UnknownKey", layer: "base", key: "Turbo" })).toContain(
    "Turbo, which is not a Speed Editor key",
  );
  expect(
    saveErrorText({ kind: "BadLayerReference", location: "base_layer", target: "gone" }),
  ).toBe("A key goes to page gone, which does not exist.");
  expect(saveErrorText({ kind: "UnknownKey", layer: "base", key: "Turbo" })).toBe(
    "Page base binds Turbo, which is not a Speed Editor key.",
  );
  expect(saveErrorText({ kind: "Malformed", message: "bad" })).toContain("bad");
  expect(saveErrorText({ kind: "SaveFailed", message: "disk full" })).toContain("disk full");
});
