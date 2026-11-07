// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import ResolveTab, { resolveDefault } from "./ResolveTab.jsx";
import WheelTab, {
  buttonDoes,
  buttonMode,
  liveButton,
  saveWith,
  sensitivityOf,
  setButtonKind,
  thresholdOf,
  wheelKind,
  withSensitivity,
} from "./WheelTab.jsx";

afterEach(cleanup);

const scroll = { left: { Scroll: { amount: -1 } }, right: { Scroll: { amount: 1 } }, threshold: 1, shuttle_max_rate_hz: null };
const config = () => ({
  version: 1,
  settings: { enabled: true, start_at_login: false, resolve_process_name: "Resolve", low_battery_threshold: 15, step_aside_for_resolve: true },
  layers: [{ id: "base", name: "Main", bindings: { Jog: { SetJogMode: { mode: "Jog" } } } }],
  base_layer: "base",
  jog: { default_mode: "Jog", per_mode: { Jog: scroll, Scroll: scroll } },
  overrides: { system_volume_step: 0.02, app_volume_step: 0.02, brightness_step: 5 },
});

test("each wheel button switches to its own mode unless it is set to Volume or Brightness", () => {
  const c = config();
  expect(buttonMode(c, "Shtl")).toBe("Shuttle");
  expect(wheelKind(c, "Jog")).toBe("scroll");
  expect(wheelKind(c, "Shuttle")).toBe("none");
  expect(buttonDoes(c, "Jog")).toBe("Scrolls");
  expect(liveButton(c)).toBe("Jog");
  const v = setButtonKind(c, "Scrl", "volume");
  expect(v.layers[0].bindings.Scrl).toEqual({ SetJogMode: { mode: "Volume" } });
  expect(buttonDoes(v, "Scrl")).toBe("Volume");
  // JOG is lit: setting it to Brightness keeps it the wheel's start mode
  const b = setButtonKind(c, "Jog", "brightness");
  expect(b.jog.default_mode).toBe("Brightness");
  expect(liveButton(b)).toBe("Jog");
  const keys = setButtonKind(c, "Jog", "keys");
  expect(keys.jog.per_mode.Jog).toMatchObject({ left: null, right: null });
  expect(setButtonKind(c, "Shtl", "none").jog.per_mode.Shuttle).toMatchObject({ left: null, right: null });
  expect(setButtonKind(c, "Shtl", "scroll").jog.per_mode.Shuttle).toMatchObject({ left: scroll.left, right: scroll.right });
});

test("the Wheel tab shows the three buttons, what turning does with the selected one lit, and Sensitivity", () => {
  render(<WheelTab initial={config()} />);
  expect(screen.getByText("When I turn the wheel…")).toBeTruthy();
  const buttons = screen.getAllByRole("button").filter((b) => b.className === "wheel-button");
  expect(buttons.map((b) => b.textContent)).toEqual(["SHTLNothing", "JOGScrolls", "SCRLScrolls"]);
  expect(buttons[1].getAttribute("aria-pressed")).toBe("true");
  expect(buttons[1].querySelector(".led-dot")).toBeTruthy();
  expect(screen.getByText("With JOG lit, turning the wheel…")).toBeTruthy();
  const radios = screen.getAllByRole("radio");
  expect(radios.map((r) => r.querySelector(".radio-title").textContent)).toEqual([
    "Scroll up and down",
    "Press shortcuts",
    "Change the volume",
    "Change screen brightness",
    "Nothing",
  ]);
  expect(radios[0].getAttribute("aria-checked")).toBe("true");
  expect(screen.getByLabelText("Sensitivity").value).toBe("100");
});

test("picking an option saves at once; Press shortcuts records left and right", async () => {
  const save = vi.fn(() => Promise.resolve(null));
  render(<WheelTab initial={config()} save={save} />);
  fireEvent.click(screen.getByText("Change the volume"));
  await vi.waitFor(() => expect(save).toHaveBeenCalledTimes(1));
  expect(save.mock.calls[0][0].layers[0].bindings.Jog).toEqual({ SetJogMode: { mode: "Volume" } });
  expect(save.mock.calls[0][0].jog.default_mode).toBe("Volume");
  fireEvent.click(screen.getByText("Press shortcuts"));
  await vi.waitFor(() => expect(save).toHaveBeenCalledTimes(2));
  expect(screen.getByText("Turning left presses")).toBeTruthy();
  const [left] = screen.getAllByPlaceholderText("Click here, then press the shortcut");
  fireEvent.keyDown(left, { key: "ArrowLeft" });
  await vi.waitFor(() => expect(save).toHaveBeenCalledTimes(3));
  expect(save.mock.calls[2][0].jog.per_mode.Jog.left).toEqual({ Hotkey: { combo: { modifiers: [], key: "Left" } } });
});

test("Sensitivity maps Slow..Fast to 10..1 ticks per step on every mode", async () => {
  expect(thresholdOf(0)).toBe(10);
  expect(thresholdOf(100)).toBe(1);
  expect(sensitivityOf(thresholdOf(50))).toBeGreaterThan(40);
  expect(Object.values(withSensitivity(config(), 0).jog.per_mode).map((b) => b.threshold)).toEqual([10, 10]);
  const save = vi.fn(() => Promise.resolve(null));
  render(<WheelTab initial={config()} save={save} />);
  fireEvent.change(screen.getByLabelText("Sensitivity"), { target: { value: "0" } });
  await vi.waitFor(() => expect(save).toHaveBeenCalled());
  expect(save.mock.calls[0][0].jog.per_mode.Jog.threshold).toBe(10);
});

test("a rejected save shows inline and is logged", async () => {
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  render(<WheelTab initial={config()} save={() => Promise.reject({ kind: "SaveFailed", message: "disk full" })} />);
  fireEvent.click(screen.getByRole("radio", { name: /Nothing/ }));
  expect(await screen.findByText("Saving failed: disk full")).toBeTruthy();
  expect(error).toHaveBeenCalled();
  error.mockRestore();
});

test("the default save goes through the save_config command", async () => {
  const invoke = vi.fn(() => Promise.resolve(null));
  window.__TAURI__ = { core: { invoke } };
  try {
    await saveWith(config(), {});
  } finally {
    delete window.__TAURI__;
  }
  expect(invoke).toHaveBeenCalledWith("save_config", { config: expect.objectContaining({ version: 1 }) });
});

test("the Resolve tab: the step-aside switch, the tip and, under More, the program name with Reset", () => {
  expect(resolveDefault("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe("Resolve.exe");
  expect(resolveDefault("Mozilla/5.0 (Macintosh; Intel Mac OS X 15_0)")).toBe("Resolve");
  const onSetting = vi.fn();
  const c = config();
  render(<ResolveTab settings={{ ...c.settings, resolve_process_name: "MyResolve" }} onSetting={onSetting} osDefault="Resolve.exe" />);
  expect(screen.getByText("If Resolve doesn’t see the Speed Editor, unplug it and plug it back in.")).toBeTruthy();
  const sw = screen.getByRole("switch", { name: "Step aside automatically when DaVinci Resolve opens" });
  expect(sw.getAttribute("aria-checked")).toBe("true");
  fireEvent.click(sw);
  expect(onSetting).toHaveBeenCalledWith("step_aside_for_resolve", false);
  expect(screen.queryByLabelText("Resolve’s program name")).toBeNull();
  fireEvent.click(screen.getByText("More"));
  const field = screen.getByLabelText("Resolve’s program name");
  expect(field.value).toBe("MyResolve");
  expect(field.placeholder).toBe("Resolve.exe");
  fireEvent.click(screen.getByText("Reset"));
  expect(field.value).toBe("Resolve.exe");
  expect(onSetting).toHaveBeenLastCalledWith("resolve_process_name", "Resolve.exe");
});

test("the Resolve program name saves on Enter; an empty name shows an inline error", () => {
  const onSetting = vi.fn();
  render(<ResolveTab settings={config().settings} onSetting={onSetting} initialMore />);
  const field = screen.getByLabelText("Resolve’s program name");
  fireEvent.change(field, { target: { value: " " } });
  fireEvent.keyDown(field, { key: "Enter" });
  expect(screen.getByText("Enter Resolve's program name.")).toBeTruthy();
  expect(onSetting).not.toHaveBeenCalled();
  fireEvent.change(field, { target: { value: "Resolve Studio" } });
  fireEvent.keyDown(field, { key: "Enter" });
  expect(onSetting).toHaveBeenCalledWith("resolve_process_name", "Resolve Studio");
});

test("no rendered text on the Wheel or Resolve tab says 'layer'", () => {
  const c = config();
  const { container } = render(
    <>
      <WheelTab initial={c} />
      <ResolveTab settings={c.settings} initialMore />
    </>,
  );
  expect(container.textContent).not.toMatch(/layer/i);
});
