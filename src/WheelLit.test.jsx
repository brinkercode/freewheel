// @vitest-environment jsdom
// The Wheel tab's "Lit on your Speed Editor" follows the device (#204).
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import WheelTab, { litButton } from "./WheelTab.jsx";

afterEach(cleanup);

const scroll = { left: { Scroll: { amount: -1 } }, right: { Scroll: { amount: 1 } }, threshold: 3, shuttle_max_rate_hz: null };
const config = () => ({
  version: 1,
  base_layer: "main",
  layers: [{ id: "main", name: "Main", bindings: { Jog: { SetJogMode: { mode: "Jog" } }, Scrl: { SetJogMode: { mode: "Volume" } } } }],
  jog: { default_mode: "Jog", per_mode: { Jog: scroll, Scroll: scroll } },
});
const status = (connection, lit) => ({ connection, layer: "main", lit, wheel_override: null, paused: false });
const litLabel = () => screen.getByTitle("Lit on your Speed Editor").closest("button");
const pressed = () => screen.getByRole("group", { name: "Wheel buttons" }).querySelector('[aria-pressed="true"]');

test("the Wheel tab's lit dot is the button lit on the Speed Editor", () => {
  // The Speed Editor has SCRL lit (the user pressed it); the config starts in JOG.
  render(<WheelTab initial={config()} status={status("Connected", ["Scrl"])} />);
  expect(litLabel().textContent).toMatch(/^SCRL/);
  // It opens on that button too.
  expect(pressed().textContent).toMatch(/^SCRL/);
  expect(screen.getByText("With SCRL lit, turning the wheel…")).toBeTruthy();
  expect(screen.getAllByTitle("Lit on your Speed Editor")).toHaveLength(1);
});

test("the dot moves when the device lights another wheel button; a button picked by hand stays shown", () => {
  const { rerender } = render(<WheelTab initial={config()} status={status("Connected", ["Scrl"])} />);
  rerender(<WheelTab initial={config()} status={status("Connected", ["Cam1", "Shtl"])} />);
  expect(litLabel().textContent).toMatch(/^SHTL/);
  expect(pressed().textContent).toMatch(/^SHTL/);
  fireEvent.click(screen.getByRole("button", { name: /^JOG/ }));
  rerender(<WheelTab initial={config()} status={status("Connected", ["Scrl"])} />);
  expect(litLabel().textContent).toMatch(/^SCRL/);
  expect(pressed().textContent).toMatch(/^JOG/);
});

test("with no Speed Editor connected, the dot is on the button the config starts in", () => {
  expect(litButton(config(), null)).toBe("Jog");
  expect(litButton(config(), status("Absent", ["Scrl"]))).toBe("Jog");
  expect(litButton(config(), status("Connected", []))).toBe("Jog");
  expect(litButton(config(), status("Connected", ["Scrl"]))).toBe("Scrl");
  render(<WheelTab initial={config()} status={status("Absent", [])} />);
  expect(litLabel().textContent).toMatch(/^JOG/);
});
