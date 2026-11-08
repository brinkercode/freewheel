// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, expect, test } from "vitest";
import Simulator, { NOTES_CHOICES, QUICK_KEYS, deviceOption, notesPreview, trayState } from "./Simulator.jsx";
import { SIM_NEXT, SIM_SAMPLE, createSimFake } from "./dev/simFake.js";
import { LABELS } from "./keys.js";

// jsdom gives import.meta.url an http scheme; vitest runs from the repo root.
const SRC = join(process.cwd(), "src");

let fake;
beforeEach(() => {
  fake = createSimFake();
  window.__TAURI__ = { core: { invoke: fake.invoke } };
});
afterEach(() => {
  cleanup();
  delete window.__TAURI__;
});

const mount = async () => {
  render(<Simulator />);
  await waitFor(() => expect(screen.getByText(/^Page base/)).toBeTruthy());
};
const group = (name) => within(screen.getByRole("group", { name }));
const pressed = (name) =>
  group(name)
    .getAllByRole("button")
    .filter((b) => b.getAttribute("aria-pressed") === "true")
    .map((b) => b.textContent);
const click = (el) => act(async () => fireEvent.click(el));
const log = () => screen.getByRole("list", { name: "What the Speed Editor did" }).textContent;

test("it has every group of the prototype's controls column", async () => {
  await mount();
  for (const text of [
    "Simulator controls",
    "Pretend to be the computer and the Speed Editor.",
    "Press a key on the Speed Editor",
    "↺ Turn wheel left",
    "Turn right ↻",
    "What the Speed Editor did shows up here.",
    "Jump to",
    "Tray icon states",
    "Do actions for real",
  ]) {
    expect(screen.getByText(text)).toBeTruthy();
  }
  for (const key of QUICK_KEYS) expect(screen.getAllByText(LABELS[key]).length).toBeGreaterThan(0);
  expect(pressed("Speed Editor")).toEqual(["Not found"]);
  expect(pressed("DaVinci Resolve")).toEqual(["Closed"]);
  expect(pressed("freewheel")).toEqual(["On"]);
  expect(pressed("Keys")).toEqual(["Empty setup"]);
  expect(pressed("macOS permission")).toEqual(["Allowed"]);
  expect(screen.getByRole("listitem", { current: true }).textContent).toBe("Not connected");
});

test("the Speed Editor row connects over USB or Bluetooth with its battery", async () => {
  await mount();
  await click(group("Speed Editor").getByText("Bluetooth 12%"));
  expect(pressed("Speed Editor")).toEqual(["Bluetooth 12%"]);
  expect(screen.getByRole("listitem", { current: true }).textContent).toBe("Needs attention");
  await click(group("Speed Editor").getByText("USB"));
  expect(pressed("Speed Editor")).toEqual(["USB"]);
  expect(screen.getByRole("listitem", { current: true }).textContent).toBe("On");
  await click(group("Speed Editor").getByText("Not found"));
  expect(pressed("Speed Editor")).toEqual(["Not found"]);
});

test("Do actions for real is on at first and says what it does on this PC", async () => {
  await mount();
  const real = screen.getByRole("switch", { name: "Do actions for real" });
  expect(real.checked).toBe(true);
  expect(screen.getByText(/sends keystrokes, opens apps and websites, and changes the volume/)).toBeTruthy();
  await click(group("Speed Editor").getByText("USB"));
  await click(group("Keys").getByText("Several set (2 pages)"));
  await click(screen.getByRole("button", { name: "Press" }));
  await act(async () => fireEvent.pointerDown(screen.getByRole("button", { name: "CUT" })));
  expect(log()).toContain("sent Ctrl+C");
  await click(real);
  expect(real.checked).toBe(false);
  expect(screen.getByText(/nothing happens on this PC/)).toBeTruthy();
  expect(log()).toContain("Do actions for real is off");
});

test("keys and the wheel go to the device and the log says what it did", async () => {
  await mount();
  await click(screen.getByRole("switch", { name: "Do actions for real" }));
  await click(group("Speed Editor").getByText("USB"));
  await click(group("Keys").getByText("Several set (2 pages)"));
  expect(pressed("Keys")).toEqual(["Several set (2 pages)"]);
  const cut = screen.getByRole("button", { name: "CUT" });
  await act(async () => fireEvent.pointerDown(cut));
  expect(cut.getAttribute("aria-pressed")).toBe("true");
  await act(async () => fireEvent.pointerUp(cut));
  expect(cut.getAttribute("aria-pressed")).toBe("false");
  expect(log()).toContain("You pressed CUT");
  expect(log()).toContain("would press Ctrl+C");

  fireEvent.change(screen.getByRole("combobox", { name: "Key to press" }), { target: { value: "Esc" } });
  await click(screen.getByRole("button", { name: "Press" }));
  await waitFor(() => expect(log()).toContain("would press Ctrl+Z"));

  const right = screen.getByRole("button", { name: "Turn right ↻" });
  await act(async () => fireEvent.pointerDown(right));
  await act(async () => fireEvent.pointerUp(right));
  expect(log()).toContain("would scroll 1");
});

test("Resolve, the switch and the permission drive the app's state", async () => {
  await mount();
  await click(group("Speed Editor").getByText("USB"));
  await click(group("DaVinci Resolve").getByText("Open"));
  expect(pressed("DaVinci Resolve")).toEqual(["Open"]);
  expect(pressed("freewheel")).toEqual(["Off"]);
  expect(screen.getByRole("listitem", { current: true }).textContent).toBe("Off / paused");
  // While Off the device stays as picked.
  await click(group("Speed Editor").getByText("Bluetooth 64%"));
  expect(pressed("Speed Editor")).toEqual(["Bluetooth 64%"]);
  await click(group("DaVinci Resolve").getByText("Closed"));
  expect(pressed("freewheel")).toEqual(["On"]);
  await click(group("macOS permission").getByText("Missing"));
  expect(screen.getByRole("listitem", { current: true }).textContent).toBe("Needs attention");
});

test("Jump to opens the settings window's screens", async () => {
  await mount();
  for (const name of ["First run", "Settings window", "Record a key", "Reload settings", "Quit app"]) {
    await click(screen.getByRole("button", { name }));
  }
  await click(group("Computer appearance").getByText("Dark"));
  expect(fake.jumps).toEqual(["FirstRun", "Settings", "RecordKey", "Reload", "Quit", "Dark"]);
  expect(pressed("Computer appearance")).toEqual(["Dark"]);
  await click(screen.getByRole("button", { name: "New version" }));
  expect(screen.getByText(/New version published: freewheel finds it by itself/)).toBeTruthy();
});

test("New version's notes: the next release's real notes, the sample or pasted ones", async () => {
  await mount();
  const notes = () => fake.invoke("sim_notes", { choice: "State", text: null });
  // A build with the next release's notes starts on them.
  await waitFor(() => expect(pressed("New version’s notes")).toEqual(["Next release (real notes)"]));
  expect(group("New version’s notes").getAllByRole("button").map((b) => b.textContent)).toEqual(NOTES_CHOICES.map(([, t]) => t));
  expect(document.querySelector("[data-notes-preview]").textContent).toBe("1 change: The next release's change");
  expect((await notes()).notes).toBe(SIM_NEXT);
  await click(group("New version’s notes").getByText("Sample notes"));
  expect(pressed("New version’s notes")).toEqual(["Sample notes"]);
  expect((await notes()).notes).toBe(SIM_SAMPLE);
  expect(log()).toContain("New version's notes: Sample notes");
  await click(group("New version’s notes").getByText("Paste notes…"));
  const box = screen.getByRole("textbox", { name: "Pasted notes" });
  fireEvent.change(box, { target: { value: "### Fixed\n\n* Pasted fix\n* Another" } });
  await click(screen.getByRole("button", { name: "Use these notes" }));
  expect(await notes()).toEqual({ choice: "Paste", notes: "### Fixed\n\n* Pasted fix\n* Another", next_available: true });
  expect(document.querySelector("[data-notes-preview]").textContent).toBe("2 changes: Pasted fix");
});

test("a build without the next release's notes can't pick them", async () => {
  fake = createSimFake({ next: "" });
  window.__TAURI__ = { core: { invoke: fake.invoke } };
  await mount();
  await waitFor(() => expect(pressed("New version’s notes")).toEqual(["Sample notes"]));
  expect(group("New version’s notes").getByText("Next release (real notes)").disabled).toBe(true);
  expect(document.querySelector("[data-notes-preview]").textContent).toMatch(/^This build has no next-release notes\. 1 change: Smoother/);
});

test("the notes preview counts the bullets", () => {
  expect(notesPreview("")).toBe("No notes: What’s new links to GitHub only.");
  expect(notesPreview("Just a line")).toBe("Notes without bullets.");
  expect(notesPreview("* **a:** [b](https://github.com/x) `c`\n- d")).toBe("2 changes: a: b c");
});

test("Do actions for real is a switch that turns back on", async () => {
  await mount();
  const real = screen.getByRole("switch");
  expect(real.checked).toBe(true);
  await click(real);
  expect(real.checked).toBe(false);
  await click(real);
  expect(real.checked).toBe(true);
  expect(screen.getByText(/On: a press does what its key is set to on this PC/)).toBeTruthy();
});


test("option and tray state helpers", () => {
  expect(deviceOption(null)).toBe("none");
  expect(deviceOption({ connection: "Off" })).toBe(null);
  expect(deviceOption({ connection: "Connected", bus: "Bluetooth", battery: 64 })).toBe("bt");
  expect(trayState({ enabled: false, connection: "Off" })).toBe("off");
  expect(trayState({ enabled: true, connection: "Connected", low_battery: false, permissions: "Granted" })).toBe("on");
});

test("the simulator's styles live in their own stylesheet, scoped to its window", () => {
  const css = readFileSync(join(SRC, "styles/simulator.css"), "utf8");
  const selectors = css
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/@supports[^{]*\{/g, "")
    .match(/[^{}]+(?=\{)/g)
    .flatMap((s) => s.split(","))
    .map((s) => s.trim())
    .filter(Boolean);
  for (const s of selectors) expect(s).toMatch(/fw-sim|fw-tray-icon/);
  expect(readFileSync(join(SRC, "Simulator.jsx"), "utf8")).toContain('import "./styles/simulator.css"');
});

test("the quick keys light as the device does: the wheel button, then the page key", async () => {
  await mount();
  await click(group("Speed Editor").getByText("USB"));
  const lit = () => [...document.querySelectorAll(".fw-sim-quick button.is-lit")].map((b) => b.textContent);
  expect(lit()).toEqual(["SCRL"]);
  expect(document.querySelector(".fw-sim-quick button.is-lit .fw-sim-led")).toBeTruthy();
  const shtl = screen.getByRole("button", { name: "SHTL" });
  await act(async () => fireEvent.pointerDown(shtl));
  await act(async () => fireEvent.pointerUp(shtl));
  expect(lit()).toEqual(["SHTL"]);
  await click(group("freewheel").getByText("Off"));
  expect(lit()).toEqual([]);
});

test("a tray reply read before a newer tray event does not undo it (Checking for updates… stays)", async () => {
  const handlers = {};
  window.__TAURI__.event = { listen: (name, cb) => ((handlers[name] = cb), Promise.resolve(() => {})) };
  await mount();
  const stale = await fake.invoke("sim_tray", { item: "State" });
  const checking = { ...stale, update: "Checking for updates…", update_enabled: false };
  let reply;
  let now = stale;
  const invoke = fake.invoke;
  // The click's reply is held back; later State reads see the app as it is now.
  window.__TAURI__.core.invoke = (cmd, args) =>
    cmd !== "sim_tray" ? invoke(cmd, args) : args?.item === "update" ? new Promise((ok) => (reply = ok)) : Promise.resolve(now);
  const item = () => document.querySelector('[data-tray="update"]');
  await click(item());
  now = checking;
  await act(async () => handlers["sim-tray"]({ payload: checking }));
  expect(item().textContent).toContain("Checking for updates…");
  // The follow-up State read is held too, so this sees the stale reply's effect alone.
  let state;
  window.__TAURI__.core.invoke = (cmd, args) => (cmd === "sim_tray" ? new Promise((ok) => (state = ok)) : invoke(cmd, args));
  await act(async () => reply({ ...stale, update: "Check for updates", update_enabled: true }));
  expect(item().textContent).toContain("Checking for updates…");
  await act(async () => state?.(checking));
});

test("a key pressed twice quickly reaches the device as two presses, in order (one control in flight at a time)", async () => {
  await mount();
  const invoke = fake.invoke;
  const waiting = [];
  const applied = [];
  // The app answers concurrent `simulate` calls in any order (an async
  // command on worker threads): here the newest call in flight goes first.
  window.__TAURI__.core.invoke = (cmd, args) =>
    cmd !== "simulate" || args.control.type !== "Key"
      ? invoke(cmd, args)
      : new Promise((ok) => waiting.push(() => (applied.push(`${args.control.key} ${args.control.down ? "down" : "up"}`), ok(invoke(cmd, args)))));
  const trans = screen.getAllByRole("button").find((b) => b.textContent === LABELS.Trans);
  await act(async () => {
    for (let i = 0; i < 2; i++) {
      fireEvent.pointerDown(trans);
      fireEvent.pointerUp(trans);
    }
  });
  let inFlight = 0;
  while (waiting.length) {
    inFlight = Math.max(inFlight, waiting.length);
    await act(async () => waiting.pop()());
  }
  expect(applied).toEqual(["Trans down", "Trans up", "Trans down", "Trans up"]);
  expect(inFlight).toBe(1);
});
