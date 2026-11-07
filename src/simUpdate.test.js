import { expect, test, vi } from "vitest";
import { SIM_CHECK_MS, SIM_DOWNLOAD_MS, SIM_RESTART_MS, createSimUpdates, nextVersion, simRelaunch } from "./simUpdate.js";
import { checkNow, download, restart, startupCheck } from "./updater.js";
import { noteSections } from "./whatsNew.js";

test("the next version is the next minor", () => {
  expect(nextVersion("0.1.0")).toBe("0.2.0");
  expect(nextVersion("v1.4.2")).toBe("1.5.0");
  expect(nextVersion("0.2.0-beta.1")).toBe("0.3.0");
});

test("nothing is published until New version; then only a newer version is found", async () => {
  const sim = createSimUpdates({ checkMs: 0 });
  expect(await sim.check("0.1.0")).toBeNull();
  expect(sim.publish("0.1.0")).toBe("0.2.0");
  expect((await sim.check("0.1.0")).version).toBe("0.2.0");
  expect(await sim.check("0.2.0")).toBeNull();
  // Each press publishes a newer one, past the running version.
  expect(sim.publish("0.1.0")).toBe("0.3.0");
  expect(sim.publish("0.5.0")).toBe("0.6.0");
});

test("the simulated update runs the real flow: checking, downloading with progress, ready, restarting, the new version", async () => {
  vi.useFakeTimers();
  try {
    const sim = createSimUpdates();
    sim.publish("0.1.0");
    const states = [];
    const onState = (s) => states.push(s);
    const checked = checkNow({ check: () => sim.check("0.1.0"), autoUpdate: false, onState });
    await vi.advanceTimersByTimeAsync(1000);
    expect(states.map((s) => s.status)).toEqual(["checking"]);
    await vi.advanceTimersByTimeAsync(600);
    const update = await checked;
    expect(states.map((s) => s.status)).toEqual(["checking", "available"]);
    expect(noteSections(update.body).map((s) => s.title)).toEqual(["Features", "Bug Fixes"]);
    const downloaded = download(update, onState);
    await vi.advanceTimersByTimeAsync(SIM_DOWNLOAD_MS / 2);
    const mid = states.at(-1);
    expect(mid.status).toBe("downloading");
    expect(mid.progress).toBeGreaterThan(20);
    expect(mid.progress).toBeLessThan(80);
    await vi.advanceTimersByTimeAsync(SIM_DOWNLOAD_MS);
    expect(await downloaded).toBe(true);
    expect(states.at(-1)).toEqual({ status: "ready", version: "0.2.0" });
    const progress = states.filter((s) => s.status === "downloading").map((s) => s.progress);
    expect(progress).toEqual([...progress].sort((a, b) => a - b));
    expect(progress.at(-1)).toBe(100);
    const restarted = vi.fn();
    const kept = vi.fn();
    const done = restart(update, () => simRelaunch(restarted), onState, kept);
    expect(states.at(-1)).toEqual({ status: "restarting", version: "0.2.0" });
    expect(kept).toHaveBeenCalledWith(update);
    await vi.advanceTimersByTimeAsync(SIM_RESTART_MS);
    await done;
    expect(restarted).toHaveBeenCalled();
  } finally {
    vi.useRealTimers();
  }
});

test("with Update automatically on, New version starts at downloading", async () => {
  vi.useFakeTimers();
  try {
    const sim = createSimUpdates();
    sim.publish("0.1.0");
    const states = [];
    const found = startupCheck({ check: () => sim.check("0.1.0"), onState: (s) => states.push(s.status) });
    await vi.advanceTimersByTimeAsync(SIM_CHECK_MS + SIM_DOWNLOAD_MS + 100);
    await found;
    expect(states[0]).toBe("downloading");
    expect(states.at(-1)).toBe("ready");
  } finally {
    vi.useRealTimers();
  }
});
