import { renderToString } from "react-dom/server";
import { expect, test, vi } from "vitest";
import App from "./App.jsx";
import KeysTab from "./KeysTab.jsx";
import ActionEditor, { ActionForm, blank, saveAction, validate } from "./ActionEditor.jsx";

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

test("app launch takes a path, arguments one per line and an enabled toggle", () => {
  const html = editor({ LaunchApp: { path: "/Applications/Notes.app", args: ["--new", "a b"], enabled: false } });
  expect(html).toContain("When I press Cam1…");
  expect(html).toContain('value="/Applications/Notes.app"');
  expect(html).toMatch(/<textarea[^>]*>--new\na b<\/textarea>/);
  expect(html).toMatch(/type="checkbox"(?![^>]*checked)/);
  expect(blank("LaunchApp", config(), "base")).toEqual({ LaunchApp: { path: "", args: [], enabled: true } });
});

const find = (node, type) => {
  if (!node || typeof node !== "object") return null;
  if (Array.isArray(node)) return node.map((n) => find(n, type)).find(Boolean) ?? null;
  return node.type === type ? node : find(node.props?.children, type);
};

test("pressing Enter in Arguments starts a new line, and saving drops empty lines", async () => {
  const c = config();
  let action = blank("LaunchApp", c, "base");
  const type = (value) =>
    find(ActionForm({ action, config: c, layerId: "base", onChange: (a) => (action = a) }), "textarea").props.onChange({
      target: { value },
    });
  type("--new");
  type("--new\n");
  expect(find(ActionForm({ action, config: c, layerId: "base", onChange: () => {} }), "textarea").props.value).toBe("--new\n");
  type("--new\n\nb");
  action = { LaunchApp: { ...action.LaunchApp, path: "/bin/app" } };
  const save = vi.fn(() => Promise.resolve(null));
  await saveAction({ config: c, layerId: "base", key: "Cam1", action, save });
  expect(save.mock.calls[0][0].layers[0].bindings.Cam1.LaunchApp.args).toEqual(["--new", "b"]);
});

test("page push picks one of the other existing pages", () => {
  const html = editor({ LayerPush: { target: "edit" } });
  expect(html).toMatch(/<option value="edit" selected="">Editing<\/option>/);
  expect(html).not.toContain('value="base"');
  expect(blank("LayerPush", config(), "base")).toEqual({ LayerPush: { target: "edit" } });
});

test("set jog mode picks Jog, Shuttle or Scroll", () => {
  const html = editor({ SetJogMode: { mode: "Shuttle" } });
  for (const mode of ["Jog", "Shuttle", "Scroll"]) expect(html).toContain(`value="${mode}"`);
  expect(html).toMatch(/value="Shuttle" selected=""/);
});

test("override toggle picks system volume, app volume or brightness", () => {
  const html = editor({ OverrideToggle: { mode: "AppVolume" } });
  for (const mode of ["SystemVolume", "AppVolume", "Brightness"]) expect(html).toContain(`value="${mode}"`);
  expect(html).toMatch(/value="AppVolume" selected="">App volume/);
});

test("invalid input shows inline errors", () => {
  const c = config();
  expect(validate({ LaunchApp: { path: " ", args: [], enabled: true } }, c, "base")).toEqual({ path: "Choose the app to open." });
  expect(validate({ LayerPush: { target: "gone" } }, c, "base")).toEqual({ target: "Pick another page." });
  expect(validate({ LayerPush: { target: "base" } }, c, "base")).toEqual({ target: "Pick another page." });
  expect(validate({ SetJogMode: { mode: "Fast" } }, c, "base")).toHaveProperty("mode");
  expect(validate({ OverrideToggle: { mode: "Bass" } }, c, "base")).toHaveProperty("mode");
  expect(validate({ SetJogMode: { mode: "Jog" } }, c, "base")).toEqual({});
  const html = renderToString(
    <ActionForm action={blank("LaunchApp", c, "base")} config={c} layerId="base" errors={{ path: "Choose the app to open." }} onChange={() => {}} />,
  );
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
  const pages = editor(actions[1]);
  expect(text(pages)).toMatch(/Go to a page/);
  for (const action of actions) expect(text(editor(action))).not.toMatch(/layer/i);
  expect(text(renderToString(<App autoUpdate={false} />))).not.toMatch(/layer/i);
});

test("the Keys tab says to pick a key until one is selected", () => {
  expect(renderToString(<KeysTab initial={config()} />)).toContain("Pick a key");
});

test("the Keys tab edits the selected key on the selected page", () => {
  const c = config();
  c.layers[1].bindings = { Cam1: { SetJogMode: { mode: "Shuttle" } } };
  c.base_layer = "edit";
  const html = renderToString(<KeysTab initial={c} initialSelected="Cam1" />);
  expect(html).toContain("When I press Cam1…");
  expect(html).toMatch(/value="Shuttle" selected=""/);
  expect(text(html)).not.toMatch(/layer/i);
});
