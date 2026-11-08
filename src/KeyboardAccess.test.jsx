// @vitest-environment jsdom
// Keys picked from the keyboard (#205) and the Simulator's quick keys and
// wheel buttons on Enter or Space (#207).
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import KeysTab from "./KeysTab.jsx";
import Simulator from "./Simulator.jsx";
import { createSimFake } from "./dev/simFake.js";

afterEach(() => {
  cleanup();
  delete window.__TAURI__;
});

const hotkey = (key) => ({ Hotkey: { combo: { modifiers: ["Ctrl"], key } } });
const config = () => ({
  version: 1,
  base_layer: "main",
  layers: [{ id: "main", name: "Main", bindings: { Cut: hotkey("S") } }],
  jog: { default_mode: "Jog", per_mode: {} },
});
const css = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "styles/base.css"), "utf8");
const panelFor = (label) => screen.queryByText(`When I press ${label}…`);

test("every key in the List view can be reached with the keyboard", () => {
  render(<KeysTab initial={config()} initialView="List" />);
  const rows = document.querySelectorAll(".button-list tr");
  expect(rows.length).toBe(43);
  const reachable = [...rows].filter((r) => r.tabIndex >= 0 || r.querySelector("button, a[href], input, [tabindex='0']"));
  expect(reachable.length).toBe(43);
});

test("Enter or Space on a List row opens that key's panel", () => {
  render(<KeysTab initial={config()} initialView="List" />);
  const row = (label) => [...document.querySelectorAll(".button-list tr")].find((r) => r.cells[0].textContent === label);
  expect(panelFor("CUT")).toBeNull();
  fireEvent.keyDown(row("CUT"), { key: "Enter" });
  expect(panelFor("CUT")).toBeTruthy();
  fireEvent.keyDown(row("IN"), { key: " " });
  expect(panelFor("IN")).toBeTruthy();
  fireEvent.keyDown(row("OUT"), { key: "a" });
  expect(panelFor("IN")).toBeTruthy();
  expect(css).toMatch(/tr:focus-visible \{ outline: 2px solid var\(--accent\); outline-offset: -2px; \}/);
});

test("each key of the drawing is a tab stop that Enter or Space picks, and keeps focus once picked", () => {
  render(<KeysTab initial={config()} />);
  const keys = document.querySelectorAll(".device [data-binding]");
  expect(keys.length).toBe(43);
  for (const k of keys) {
    expect(k.getAttribute("tabindex")).toBe("0");
    expect(k.getAttribute("role")).toBe("button");
  }
  const cut = document.querySelector('.device [data-binding="Cut"]');
  expect(cut.getAttribute("aria-label")).toBe("CUT: Save");
  cut.focus();
  fireEvent.keyDown(cut, { key: "Enter" });
  expect(panelFor("CUT")).toBeTruthy();
  // The drawing is drawn again with CUT selected; focus stays on CUT.
  expect(document.activeElement.dataset.binding).toBe("Cut");
  expect(document.activeElement.getAttribute("aria-pressed")).toBe("true");
  fireEvent.keyDown(document.querySelector('.device [data-binding="In"]'), { key: " " });
  expect(panelFor("IN")).toBeTruthy();
  expect(css).toMatch(/\.device \.key:focus-visible \.key-face \{ stroke: var\(--accent-device\); stroke-width: 2px; \}/);
});

const simulator = async () => {
  const fake = createSimFake();
  window.__TAURI__ = { core: { invoke: fake.invoke } };
  render(<Simulator />);
  await waitFor(() => expect(screen.getByText(/^Page base/)).toBeTruthy());
  return () => [...screen.getByRole("list", { name: "What the Speed Editor did" }).children].map((li) => li.textContent);
};

test("a quick key in the Simulator presses with Enter or Space too", async () => {
  const log = await simulator();
  // A keyboard Enter/Space on a <button> reaches React as a click with no pointer (detail 0).
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "CUT" })));
  expect(log()).toContain("You pressed CUT");
});

test("the Simulator's wheel buttons turn one step on Enter or Space; a mouse click presses once", async () => {
  const log = await simulator();
  await act(async () => fireEvent.click(screen.getByRole("button", { name: /Turn wheel left/ })));
  await act(async () => fireEvent.click(screen.getByRole("button", { name: /Turn right/ })));
  expect(log()).toContain("You turned the wheel left");
  expect(log()).toContain("You turned the wheel right");
  // A mouse press is pointer down/up, then a click with detail 1: one press only.
  const inKey = screen.getByRole("button", { name: "IN" });
  await act(async () => fireEvent.pointerDown(inKey));
  await act(async () => fireEvent.pointerUp(inKey));
  await act(async () => fireEvent.click(inKey, { detail: 1 }));
  expect(log().filter((l) => l === "You pressed IN")).toHaveLength(1);
});
