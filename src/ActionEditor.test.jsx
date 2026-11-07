// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { renderToString } from "react-dom/server";
import { expect, test, vi } from "vitest";
import App from "./App.jsx";
import KeysTab from "./KeysTab.jsx";
import ActionEditor, { ActionForm, KeyPanel, appHue, blank, filterApps, openerFor, saveAction, validate, websiteAction } from "./ActionEditor.jsx";

const config = (bindings = {}) => ({
  version: 1,
  settings: { enabled: true, start_at_login: false, resolve_process_name: "Resolve", low_battery_threshold: 15 },
  layers: [
    { id: "base", name: "Main", bindings },
    { id: "edit", name: "Editing", bindings: {} },
  ],
  base_layer: "base",
  jog: { default_mode: "Scroll", per_mode: {} },
  overrides: { system_volume_step: 0.02, app_volume_step: 0.02, brightness_step: 5 },
});

const editor = (action) => renderToString(<ActionEditor keyName="Cam1" initialConfig={config({ Cam1: action })} />);
const text = (html) => html.replace(/<[^>]*>/g, " ");

test("Open an app or website: search the installed apps, Choose file…, or a website with Use", () => {
  const html = editor({ LaunchApp: { path: "/Applications/Notes.app", args: ["--new"], enabled: true } });
  expect(html).toContain("When I press CAM 1…");
  expect(html).toContain("Right now: Opens Notes");
  expect(blank("LaunchApp", config(), "base")).toEqual({ LaunchApp: { path: "", args: [], enabled: true } });
  const apps = [
    { name: "Spotify", path: "explorer.exe", args: ["C:/Start Menu/Spotify.lnk"] },
    { name: "Notepad", path: "explorer.exe", args: ["C:/Start Menu/Notepad.lnk"] },
  ];
  const form = renderToString(<ActionForm action={{ LaunchApp: { path: "explorer.exe", args: ["C:/Start Menu/Spotify.lnk"], enabled: true } }} initialApps={apps} />);
  expect(form).toContain('placeholder="Search your apps"');
  expect(form).toContain("Choose file…");
  expect(form).toContain('placeholder="like youtube.com"');
  expect(form.match(/>Use<\/button>/g)).toHaveLength(1);
  // The current app is checked.
  expect(form).toMatch(/aria-selected="true"[^>]*>.*Spotify/);
  expect(filterApps(apps, " note ")).toEqual([apps[1]]);
  expect(filterApps(apps, "")).toEqual(apps);
  expect(appHue("Spotify")).toBe(appHue("Spotify"));
  expect(appHue("Spotify")).toBeLessThan(12);
  expect(websiteAction(" https://youtube.com/ ").LaunchApp.args).toEqual(["https://youtube.com"]);
});

test("picking an app, Choose file… and a website each save at once", async () => {
  const onUse = vi.fn();
  const apps = [{ name: "Spotify", path: "explorer.exe", args: ["C:/Start Menu/Spotify.lnk"] }];
  const choose = vi.fn(() => Promise.resolve({ name: "Budget", path: "C:/Tools/Budget.exe", args: [] }));
  render(<ActionForm action={blank("LaunchApp", config(), "base")} initialApps={apps} choose={choose} onUse={onUse} />);
  fireEvent.click(screen.getByText("Spotify"));
  expect(onUse).toHaveBeenLastCalledWith({ LaunchApp: { path: "explorer.exe", args: ["C:/Start Menu/Spotify.lnk"], enabled: true } });
  fireEvent.change(screen.getByLabelText("Search your apps"), { target: { value: "zz" } });
  expect(screen.getByText("No apps match “zz”.")).toBeTruthy();
  fireEvent.click(screen.getByText("Choose file…"));
  await vi.waitFor(() => expect(onUse.mock.lastCall[0].LaunchApp.path).toBe("C:/Tools/Budget.exe"));
  fireEvent.change(screen.getByLabelText("Website"), { target: { value: "youtube.com" } });
  fireEvent.click(screen.getByText("Use"));
  expect(onUse.mock.lastCall[0].LaunchApp.args).toEqual(["https://youtube.com"]);
  cleanup();
});

test("saving drops empty arguments", async () => {
  const c = config();
  const action = { LaunchApp: { path: "/bin/app", args: ["--new", "", "b"], enabled: true } };
  const save = vi.fn(() => Promise.resolve(null));
  await saveAction({ config: c, layerId: "base", key: "Cam1", action, save });
  expect(save.mock.calls[0][0].layers[0].bindings.Cam1.LaunchApp.args).toEqual(["--new", "b"]);
});

test("Switch to another set of keys lists the other pages, the current one checked", () => {
  render(<KeyPanel keyName="Cam1" action={{ LayerPush: { target: "edit" } }} config={config()} layerId="base" initialMode="LayerPush" />);
  const row = screen.getByText("Editing").closest("button");
  expect(row.getAttribute("aria-pressed")).toBe("true");
  expect(screen.queryByText("Main")).toBeNull();
  expect(screen.getByText("On that page, CAM 1 becomes “Back”, so you can always return to Main.")).toBeTruthy();
  expect(screen.getByText("Make a new page")).toBeTruthy();
  expect(blank("LayerPush", config(), "base")).toEqual({ LayerPush: { target: "edit" } });
  cleanup();
});

test("Turn the wheel into… offers system volume, app volume and brightness; picking one saves it", () => {
  const onSaveAction = vi.fn(() => Promise.resolve({}));
  render(<KeyPanel keyName="Cam1" action={{ OverrideToggle: { mode: "AppVolume" } }} config={config()} layerId="base" initialMode="OverrideToggle" onSaveAction={onSaveAction} />);
  expect(screen.getByText("Volume of the app I’m using").closest("button").getAttribute("aria-pressed")).toBe("true");
  fireEvent.click(screen.getByText("Screen brightness"));
  expect(onSaveAction).toHaveBeenCalledWith({ OverrideToggle: { mode: "Brightness" } });
  expect(screen.getByText("Press CAM 1 again to put the wheel back to normal.")).toBeTruthy();
  cleanup();
});

test("invalid input shows inline errors", () => {
  const c = config();
  expect(validate({ LaunchApp: { path: " ", args: [], enabled: true } }, c, "base")).toEqual({ path: "Choose the app to open." });
  expect(validate({ LayerPush: { target: "gone" } }, c, "base")).toEqual({ target: "Pick another page." });
  expect(validate({ LayerPush: { target: "base" } }, c, "base")).toEqual({ target: "Pick another page." });
  expect(validate({ SetJogMode: { mode: "Fast" } }, c, "base")).toHaveProperty("mode");
  expect(validate({ OverrideToggle: { mode: "Bass" } }, c, "base")).toHaveProperty("mode");
  expect(validate({ SetJogMode: { mode: "Jog" } }, c, "base")).toEqual({});
  const html = renderToString(<ActionForm action={blank("LaunchApp", c, "base")} initialApps={[]} errors={{ path: "Choose the app to open." }} />);
  expect(html).toMatch(/aria-invalid="true"/);
  expect(html).toContain('<span role="alert">Choose the app to open.</span>');
});

test("saving calls save_config with the new binding, and not when the input is invalid", async () => {
  const save = vi.fn(() => Promise.resolve(null));
  const action = { SetJogMode: { mode: "Jog" } };
  const saved = await saveAction({ config: config(), layerId: "base", key: "Cam1", action, save });
  expect(save).toHaveBeenCalledTimes(1);
  expect(save.mock.calls[0][0].layers[0].bindings).toEqual({ Cam1: action });
  expect(saved).toEqual({ errors: {}, config: save.mock.calls[0][0] });

  const bad = await saveAction({ config: config(), layerId: "base", key: "Cam1", action: { LayerPush: { target: "gone" } }, save });
  expect(bad).toEqual({ errors: { target: "Pick another page." } });
  expect(save).toHaveBeenCalledTimes(1);

  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  const reject = () => Promise.reject({ kind: "SaveFailed", message: "disk full" });
  const failed = await saveAction({ config: config(), layerId: "base", key: "Cam1", action, save: reject });
  expect(failed).toEqual({ errors: { save: "Saving failed: disk full" } });
  expect(error).toHaveBeenCalled();
  error.mockRestore();
});

test("the default save goes through the save_config command", async () => {
  const invoke = vi.fn(() => Promise.resolve(null));
  globalThis.window = { __TAURI__: { core: { invoke } } };
  try {
    await saveAction({ config: config(), layerId: "base", key: "Cam1", action: { SetJogMode: { mode: "Jog" } } });
  } finally {
    delete globalThis.window;
  }
  expect(invoke).toHaveBeenCalledWith("save_config", { config: expect.objectContaining({ version: 1 }) });
});

test("no rendered text says 'layer'; layers are pages", () => {
  const actions = [
    { LaunchApp: { path: "", args: [], enabled: true } },
    { LayerPush: { target: "edit" } },
    { SetJogMode: { mode: "Jog" } },
    { OverrideToggle: { mode: "Brightness" } },
  ];
  for (const action of actions) expect(text(editor(action))).not.toMatch(/layer/i);
  for (const mode of ["LaunchApp", "LayerPush", "OverrideToggle"])
    expect(text(renderToString(<KeyPanel keyName="Cam1" config={config()} layerId="base" initialMode={mode} />))).not.toMatch(/layer/i);
  expect(text(renderToString(<App autoUpdate={false} />))).not.toMatch(/layer/i);
});

test("the Keys tab says to pick a key until one is selected", () => {
  expect(renderToString(<KeysTab initial={config()} />)).toContain("Pick a key");
});

test("the Keys tab edits the selected key on the selected page", () => {
  const c = config();
  c.layers[1].bindings = { Cam1: { OverrideToggle: { mode: "Brightness" } } };
  c.base_layer = "edit";
  const html = renderToString(<KeysTab initial={c} initialSelected="Cam1" />);
  expect(html).toContain("When I press CAM 1…");
  expect(html).toContain("Right now: Turns the wheel into a brightness knob");
  expect(text(html)).not.toMatch(/layer/i);
});

test("the Simulator's Record a key opens the key panel on Press a shortcut, recording", () => {
  const cfg = { version: 1, base_layer: "base", layers: [{ id: "base", name: "Main", bindings: {} }] };
  const idle = renderToString(<ActionEditor keyName="Cut" initialConfig={cfg} />);
  expect(idle).not.toContain("record-box");
  const html = renderToString(<ActionEditor keyName="Cut" initialConfig={cfg} recording />);
  expect(html).toContain("All choices");
  expect(html).toContain("record-box");
  expect(html).toContain("When you press CUT, freewheel presses this");
});

test("a website opens with each OS's opener (xdg-open on Linux)", () => {
  expect(openerFor("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)")).toBe("/usr/bin/open");
  expect(openerFor("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15")).toBe("xdg-open");
  expect(openerFor("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe("explorer.exe");
  expect(openerFor("Mozilla/5.0 (Linux; Android 14)")).toBe("explorer.exe");
  expect(websiteAction("example.com", "xdg-open").LaunchApp).toEqual({ path: "xdg-open", args: ["https://example.com"], enabled: true });
});
