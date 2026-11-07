import { expect, test } from "vitest";
import { SIM_RECORD_KEY_EVENT, SIM_UPDATE_EVENT, listenSimulate, simulating } from "./simulateEvents.js";

const fakeWindow = (flag) => {
  const listeners = {};
  return {
    __FREEWHEEL_SIMULATE__: flag,
    listeners,
    __TAURI__: { event: { listen: async (e, h) => ((listeners[e] = h), () => delete listeners[e]) } },
  };
};

test("the simulate events act only in --simulate", async () => {
  for (const flag of [true, undefined]) {
    const win = fakeWindow(flag);
    const seen = [];
    const stop = listenSimulate({ onRecordKey: (k) => seen.push(["key", k]), onUpdate: (u) => seen.push(["update", u]) }, win);
    await Promise.resolve();
    win.listeners[SIM_RECORD_KEY_EVENT]({ payload: "Cut" });
    win.listeners[SIM_UPDATE_EVENT]({ payload: { status: "ready", version: "99.0.0" } });
    expect(seen).toEqual(flag ? [["key", "Cut"], ["update", { status: "ready", version: "99.0.0" }]] : []);
    stop();
    await new Promise((r) => setTimeout(r));
    expect(Object.keys(win.listeners)).toEqual([]);
  }
  expect(simulating({})).toBe(false);
  expect(listenSimulate({}, {})).toBeTypeOf("function");
});
