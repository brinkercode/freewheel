// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import ResolveTab, { resolveDefault } from "./ResolveTab.jsx";
import WheelTab, { saveWith, validateWheel } from "./WheelTab.jsx";

afterEach(cleanup);

const config = () => ({
  version: 1,
  settings: { enabled: true, start_at_login: false, resolve_process_name: "Resolve", low_battery_threshold: 15 },
  layers: [{ id: "base", name: "Main", bindings: {} }],
  base_layer: "base",
  jog: {
    default_mode: "Scroll",
    per_mode: { Scroll: { left: { Scroll: { amount: -1 } }, right: { Scroll: { amount: 1 } }, threshold: 1, shuttle_max_rate_hz: null } },
  },
  overrides: { system_volume_step: 0.02, app_volume_step: 0.02, brightness_step: 5 },
});

test("each wheel mode has left and right action editors and a threshold or sensitivity input", () => {
  render(<WheelTab initial={config()} />);
  for (const mode of ["Jog", "Shuttle", "Scroll"]) expect(screen.getByText(mode, { selector: "legend" })).toBeTruthy();
  expect(screen.getAllByLabelText("Turn left")).toHaveLength(3);
  expect(screen.getAllByLabelText("Turn right")).toHaveLength(3);
  expect(screen.getAllByLabelText("Ticks per step")).toHaveLength(2);
  expect(screen.getAllByLabelText("Sensitivity")).toHaveLength(1);
  expect(screen.getAllByLabelText("Lines").map((i) => i.value)).toEqual(["-1", "1"]);
});

test("the dial steps show as percentages", () => {
  render(<WheelTab initial={config()} />);
  expect(screen.getByLabelText("System volume (%)").value).toBe("2");
  expect(screen.getByLabelText("App volume (%)").value).toBe("2");
  expect(screen.getByLabelText("Brightness (%)").value).toBe("5");
});

test("saving the wheel calls save_config with the edited modes and steps", async () => {
  const save = vi.fn(() => Promise.resolve(null));
  render(<WheelTab initial={config()} save={save} />);
  fireEvent.change(screen.getAllByLabelText("Turn left")[0], { target: { value: "Hotkey" } });
  fireEvent.keyDown(screen.getByPlaceholderText("Click here, then press the shortcut"), { key: "k", ctrlKey: true });
  fireEvent.change(screen.getAllByLabelText("Sensitivity")[0], { target: { value: "8" } });
  fireEvent.change(screen.getByLabelText("Brightness (%)"), { target: { value: "10" } });
  fireEvent.change(screen.getByLabelText("System volume (%)"), { target: { value: "5" } });
  fireEvent.click(screen.getByText("Save"));
  await vi.waitFor(() => expect(save).toHaveBeenCalledTimes(1));
  const saved = save.mock.calls[0][0];
  expect(saved.jog.per_mode.Jog.left).toEqual({ Hotkey: { combo: { modifiers: ["Ctrl"], key: "K" } } });
  expect(saved.jog.per_mode.Shuttle.threshold).toBe(8);
  expect(saved.jog.per_mode.Scroll.right).toEqual({ Scroll: { amount: 1 } });
  expect(saved.overrides).toEqual({ system_volume_step: 0.05, app_volume_step: 0.02, brightness_step: 10 });
});

test("invalid wheel input shows inline errors and is never saved", async () => {
  const save = vi.fn(() => Promise.resolve(null));
  render(<WheelTab initial={config()} save={save} />);
  fireEvent.change(screen.getAllByLabelText("Ticks per step")[0], { target: { value: "0" } });
  fireEvent.change(screen.getAllByLabelText("Turn right")[0], { target: { value: "LaunchApp" } });
  fireEvent.change(screen.getByLabelText("Brightness (%)"), { target: { value: "" } });
  fireEvent.click(screen.getByText("Save"));
  expect(await screen.findByText("Enter a whole number of 1 or more.")).toBeTruthy();
  expect(screen.getByText("Choose the app to open.")).toBeTruthy();
  expect(screen.getByText("Enter a step from 1 to 100.")).toBeTruthy();
  expect(save).not.toHaveBeenCalled();
  const c = config();
  c.jog.per_mode.Scroll.left = { Scroll: { amount: 0 } };
  expect(validateWheel(c)).toEqual({ "Scroll.left": "Enter how far to scroll, like 1 or -1." });
  expect(validateWheel(config())).toEqual({});
});

test("a rejected save shows inline and is logged", async () => {
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  render(<WheelTab initial={config()} save={() => Promise.reject({ kind: "SaveFailed", message: "disk full" })} />);
  fireEvent.click(screen.getByText("Save"));
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

test("the Resolve program name field shows the OS default and Reset restores it", () => {
  expect(resolveDefault("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe("Resolve.exe");
  expect(resolveDefault("Mozilla/5.0 (Macintosh; Intel Mac OS X 15_0)")).toBe("Resolve");
  const c = config();
  c.settings.resolve_process_name = "MyResolve";
  render(<ResolveTab initial={c} osDefault="Resolve.exe" />);
  const field = screen.getByLabelText("Resolve's program name");
  expect(field.placeholder).toBe("Resolve.exe");
  expect(screen.getByText("Default on this computer: Resolve.exe")).toBeTruthy();
  fireEvent.click(screen.getByText("Reset"));
  expect(field.value).toBe("Resolve.exe");
});

test("saving the Resolve name calls save_config; an empty name shows an inline error", async () => {
  const save = vi.fn(() => Promise.resolve(null));
  render(<ResolveTab initial={config()} save={save} osDefault="Resolve" />);
  const field = screen.getByLabelText("Resolve's program name");
  fireEvent.change(field, { target: { value: " " } });
  fireEvent.click(screen.getByText("Save"));
  expect(await screen.findByText("Enter Resolve's program name.")).toBeTruthy();
  expect(save).not.toHaveBeenCalled();
  fireEvent.change(field, { target: { value: "Resolve Studio" } });
  fireEvent.click(screen.getByText("Save"));
  await vi.waitFor(() => expect(save).toHaveBeenCalledTimes(1));
  expect(save.mock.calls[0][0].settings.resolve_process_name).toBe("Resolve Studio");
});

test("no rendered text on the Wheel or Resolve tab says 'layer'", () => {
  const c = config();
  c.jog.per_mode.Jog = { left: { LaunchApp: { path: "", args: [], enabled: true } }, right: null, threshold: 1, shuttle_max_rate_hz: null };
  const { container } = render(
    <>
      <WheelTab initial={c} />
      <ResolveTab initial={c} />
    </>,
  );
  expect(container.textContent).not.toMatch(/layer/i);
});
