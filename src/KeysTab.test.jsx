import { readFileSync } from "node:fs";
import { renderToString } from "react-dom/server";
import { expect, test, vi } from "vitest";
import KeysTab, { ButtonList, KeyEditor, PageSelector, handlePress, saveKey } from "./KeysTab.jsx";
import Status from "./Status.jsx";
import { DRAWING, KEYS, LABELS, addPage, comboError, comboFromEvent, deletePage, formatCombo, renamePage } from "./keys.js";
import { saveErrorText } from "./settingsApi.js";

const combo = (key, ...modifiers) => ({ modifiers, key });
const config = {
  version: 1,
  base_layer: "base",
  layers: [
    { id: "base", name: "Main", bindings: { Cut: { Hotkey: { combo: combo("C", "Ctrl") } }, Cam1: { LayerPush: { target: "edit" } } } },
    { id: "edit", name: "Editing", bindings: { Cut: { Held: { combo: combo("Space") } } } },
  ],
};
const text = (html) => html.replace(/<[^>]*>/g, " ");
const rows = (html) => html.match(/<tr/g).length;

test("every Speed Editor key has its printed label and one place in the drawing", () => {
  const core = readFileSync(new URL("../crates/freewheel-core/src/status.rs", import.meta.url), "utf8");
  const printed = Object.fromEntries([...core.matchAll(/Key::(\w+) => "([^"]+)"/g)].map((m) => [m[1], m[2]]));
  expect(LABELS).toEqual(printed);
  expect(KEYS).toHaveLength(43);
  expect(DRAWING.flat(2).sort()).toEqual([...KEYS].sort());
});

test("the page selector adds, renames and deletes pages", () => {
  const html = renderToString(<PageSelector config={config} page="edit" />);
  for (const t of [">Main</option>", ">Editing</option>", 'value="Editing"', ">Add page</button>", ">Delete page</button>"]) expect(html).toContain(t);
  const added = addPage(config);
  expect(added.id).toBe("page-3");
  expect(added.config.layers.at(-1)).toEqual({ id: "page-3", name: "Page 3", bindings: {} });
  expect(renamePage(config, "base", "Colour").layers[0].name).toBe("Colour");
  expect(deletePage(added.config, "page-3").config.layers.map((l) => l.id)).toEqual(["base", "edit"]);
});

test("deleting a page a key goes to, or the first page, shows an error", () => {
  const { error } = deletePage(config, "edit");
  expect(error).toBe("Editing can't be deleted: CAM 1 on Main goes to it. Change that key first.");
  expect(deletePage(config, "base").error).toBe("The first page can't be deleted.");
  expect(renderToString(<PageSelector config={config} page="edit" error={error} />)).toMatch(/role="alert">Editing can&#x27;t be deleted/);
});

test("no rendered text says 'layer'", () => {
  const views = [
    renderToString(<KeysTab initial={config} initialSelected="Cut" initialChip="You pressed CUT" />),
    renderToString(<KeysTab initial={config} initialView="List" />),
    renderToString(<PageSelector config={config} page="edit" error={deletePage(config, "edit").error} />),
    renderToString(<Status initial={{ connection: "Connected", battery: null, layer: "base", override_errors: [] }} />),
    saveErrorText({ kind: "UnknownKey", layer: "base", key: "Turbo" }),
    saveErrorText({ kind: "BadLayerReference", location: "Cam1", target: "gone" }),
  ];
  for (const html of views) expect(text(html)).not.toMatch(/layer/i);
});

test("the list has one row per Speed Editor key with its action on the selected page", () => {
  const base = renderToString(<ButtonList config={config} page="base" />);
  expect(rows(base)).toBe(43);
  expect(base).toContain("<td>CUT</td><td>Ctrl+C</td>");
  expect(base).toContain("<td>CAM 1</td><td>Go to page Editing</td>");
  expect(base).toContain("<td>STOP/PLAY</td><td>Not set</td>");
  expect(renderToString(<ButtonList config={config} page="edit" />)).toContain("<td>CUT</td><td>Hold Space</td>");
});

test("clicking a row opens the action editor for that key", () => {
  const onSelect = vi.fn();
  const table = ButtonList({ config, page: "base", onSelect });
  const cut = table.props.children.props.children.find((row) => row.key === "Cut");
  cut.props.onClick();
  expect(onSelect).toHaveBeenCalledWith("Cut");
  expect(renderToString(<KeysTab initial={config} />)).not.toContain("key-editor");
  const open = renderToString(<KeysTab initial={config} initialSelected="Cut" />);
  expect(open).toContain("What should CUT do?");
  expect(open).toContain('value="Ctrl+C"');
});

test("the drawing is the main view, with a Drawing/List toggle", () => {
  const html = renderToString(<KeysTab initial={config} />);
  expect(html).toContain('class="device"');
  expect(html).not.toContain("<table");
  expect(html).toMatch(/aria-pressed="true"[^>]*>Drawing</);
  expect(html).toContain(">List</button>");
  const list = renderToString(<KeysTab initial={config} initialView="List" />);
  expect(rows(list)).toBe(43);
  expect(list).not.toContain('class="device"');
});

test("a physical press shows 'You pressed X' for 2 s and selects that key", () => {
  vi.useFakeTimers();
  try {
    const select = vi.fn();
    const setChip = vi.fn();
    const timer = { current: null };
    handlePress({ key: "Cut", label: "CUT" }, { select, setChip, timer });
    expect(select).toHaveBeenCalledWith("Cut");
    expect(setChip).toHaveBeenLastCalledWith("You pressed CUT");
    vi.advanceTimersByTime(1000);
    handlePress({ key: "Cam1", label: "CAM 1" }, { select, setChip, timer });
    vi.advanceTimersByTime(1999);
    expect(setChip).toHaveBeenLastCalledWith("You pressed CAM 1");
    vi.advanceTimersByTime(1);
    expect(setChip).toHaveBeenLastCalledWith(null);
  } finally {
    vi.useRealTimers();
  }
  expect(renderToString(<KeysTab initial={config} initialChip="You pressed CUT" />)).toMatch(/press-chip" role="status">You pressed CUT</);
});

test("the editor offers Hotkey, Held and Latched", () => {
  const html = renderToString(<KeyEditor keyName="Cut" action={config.layers[1].bindings.Cut} />);
  for (const kind of ["Hotkey", "Held", "Latched"]) expect(html).toContain(`<strong>${kind}</strong>`);
  expect(html).toMatch(/checked=""[^>]*\/><strong>Held/);
});

test("capture records modifiers plus one key and shows it", () => {
  const recorded = comboFromEvent({ key: "k", ctrlKey: true, shiftKey: true });
  expect(recorded).toEqual(combo("K", "Ctrl", "Shift"));
  expect(formatCombo(recorded)).toBe("Ctrl+Shift+K");
  expect(comboFromEvent({ key: "Shift", shiftKey: true })).toEqual(combo("", "Shift"));
  expect(comboFromEvent({ key: "ArrowUp", altKey: true, metaKey: true })).toEqual(combo("Up", "Alt", "Meta"));
  expect(comboFromEvent({ key: " " })).toEqual(combo("Space"));
  const html = renderToString(<KeyEditor keyName="Cut" action={{ Hotkey: { combo: recorded } }} />);
  expect(html).toContain('value="Ctrl+Shift+K"');
});

test("an invalid combo shows an inline error and is never saved", async () => {
  expect(comboError(null)).toBe("Press the shortcut you want first.");
  expect(comboError(combo("", "Ctrl"))).toBe("Add a key to go with the modifier, like Ctrl+K.");
  expect(comboError(combo("CapsLock"))).toBe("freewheel can't send the CapsLock key.");
  expect(comboError(combo("F12", "Shift"))).toBeNull();
  const save = vi.fn(async () => {});
  const r = await saveKey({ config, pageId: "base", key: "Out", kind: "Latched", combo: combo("", "Ctrl"), save });
  expect(r.error).toBe("Add a key to go with the modifier, like Ctrl+K.");
  expect(save).not.toHaveBeenCalled();
  const html = renderToString(<KeyEditor keyName="Out" initialError={r.error} />);
  expect(html).toMatch(/aria-invalid="true"/);
  expect(html).toContain('role="alert">Add a key to go with the modifier, like Ctrl+K.</p>');
});

test("saving calls save_config with the key's new action", async () => {
  const save = vi.fn(async () => {});
  const r = await saveKey({ config, pageId: "edit", key: "Out", kind: "Latched", combo: combo("K", "Ctrl"), save });
  expect(save).toHaveBeenCalledWith(r.config);
  expect(r.config.layers[1].bindings.Out).toEqual({ Latched: { combo: combo("K", "Ctrl") } });
  expect(r.config.layers[1].bindings.Cut).toEqual(config.layers[1].bindings.Cut);
  const invoke = vi.fn(async () => {});
  vi.stubGlobal("window", { __TAURI__: { core: { invoke } } });
  try {
    await saveKey({ config, pageId: "base", key: "In", kind: "Hotkey", combo: combo("I") });
    expect(invoke).toHaveBeenCalledWith("save_config", { config: expect.objectContaining({ version: 1 }) });
  } finally {
    vi.unstubAllGlobals();
  }
  const rejected = vi.fn(async () => Promise.reject({ kind: "SaveFailed", message: "disk full" }));
  const log = vi.spyOn(console, "error").mockImplementation(() => {});
  expect((await saveKey({ config, pageId: "base", key: "In", kind: "Hotkey", combo: combo("I"), save: rejected })).error).toBe(
    "Saving failed: disk full",
  );
  expect(log).toHaveBeenCalled();
  log.mockRestore();
});
