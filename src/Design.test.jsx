// @vitest-environment jsdom
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { renderToString } from "react-dom/server";
import { afterEach, expect, test, vi } from "vitest";
import ActionEditor, { KIND_ROWS, KIND_TEXT, KeyPanel } from "./ActionEditor.jsx";
import Backup from "./Backup.jsx";
import { PageSelector, drawingMarkup } from "./KeysTab.jsx";
import Permissions, { PermissionsScreen } from "./Permissions.jsx";
import { NoticeBar, Toolbar } from "./SettingsWindow.jsx";

afterEach(cleanup);

const here = dirname(fileURLToPath(import.meta.url));
const read = (path) => readFileSync(join(here, path), "utf8");
const list = () => readdirSync(here);
const tokens = read("./styles/tokens.css");
const base = read("./styles/base.css");
const config = () => ({
  version: 1,
  base_layer: "base",
  layers: [
    { id: "base", name: "Main", bindings: { Cut: { Hotkey: { combo: { modifiers: ["Ctrl"], key: "C" } } } } },
    { id: "edit", name: "Editing", bindings: {} },
  ],
});

test("the handoff tokens and the base stylesheet are imported once, from the app entry", () => {
  expect(tokens).toBe(tokens.replace(/\r/g, ""));
  const main = read("./main.jsx");
  expect(main.match(/styles\/tokens\.css/g)).toHaveLength(1);
  expect(main.match(/styles\/base\.css/g)).toHaveLength(1);
  const others = list().filter((f) => /\.jsx?$/.test(f) && f !== "main.jsx" && !f.includes(".test."));
  for (const f of others) expect(read(`./${f}`)).not.toMatch(/tokens\.css|base\.css/);
});

test("the theme switches with prefers-color-scheme: dark values in a dark media query", () => {
  const dark = tokens.match(/@media \(prefers-color-scheme: dark\)\{([\s\S]*?)\n\}/)[1];
  expect(dark).toContain("--bg:#1e1e20");
  expect(dark).toContain("--ink:#f1f1ef");
  expect(tokens).toMatch(/:root\{[\s\S]*--bg:#f7f7f5/);
  // the base stylesheet paints with the tokens, so it follows the theme
  expect(base).toMatch(/body \{[^}]*color: var\(--ink\);[^}]*background: var\(--bg\);/);
  expect(base).toContain("font-family: var(--font-win)");
  expect(base).toContain("font-family: var(--font-mac)");
});

test("no inline style={{...}} in the app's components", () => {
  for (const f of list().filter((f) => f.endsWith(".jsx") && !f.includes(".test."))) {
    expect(read(`./${f}`), f).not.toMatch(/style=\{\{/);
  }
});

test("the Keys drawing is the handoff SVG with is-selected, is-pressed and is-lit as the README documents", () => {
  const svg = drawingMarkup({ labels: { Cut: "Ctrl+C" }, selected: "Cut", pressed: "Cam1", lit: ["Jog"] });
  expect(svg).toMatch(/<g id="key-CUT" data-key="CUT"[^>]*class="key is-selected"/);
  expect(svg).toMatch(/<g id="key-CAM1" data-key="CAM1"[^>]*class="key is-pressed is-unset"/);
  expect(svg).toMatch(/<g id="key-JOG" data-key="JOG"[^>]*class="key is-lit is-unset"/);
  expect(svg).toMatch(/>Ctrl\+C<\/tspan><\/text>/);
  expect(svg.match(/class="key[ "]/g)).toHaveLength(43);
  expect(base).toMatch(/\.key\.is-selected \.key-face \{ fill: var\(--key-dark-sel\); \}/);
  // the ring: 2px of the panel colour between the face and a 2px --accent-device stroke
  expect(svg.match(/class="key-ring"/g)).toHaveLength(1);
  expect(svg).toMatch(/<g id="key-CUT"[^>]*>\s*<rect class="key-ring" x="34" y="336.8" width="52" height="52" rx="8"><\/rect><rect class="key-face" x="37"/);
  expect(base).toMatch(/\.key-ring \{ fill: var\(--dev-panel\); stroke: var\(--accent-device\); stroke-width: 2px; \}/);
  expect(base).toMatch(/\.key\.is-pressed \.key-face \{ fill: var\(--accent-device\); \}/);
  expect(base).toMatch(/\.key\.is-lit \.key-led \{ fill: var\(--led-lit\);/);
});

test("the key panel is one choices list, one entry per KIND_ROWS kind, with no 'What should X do?' caption", () => {
  render(<ActionEditor keyName="Out" initialConfig={config()} />);
  const rows = screen.getAllByRole("radio");
  expect(rows).toHaveLength(KIND_ROWS.length);
  for (const k of KIND_ROWS) expect(screen.getByText(KIND_TEXT[k][0])).toBeTruthy();
  expect(document.body.textContent).not.toMatch(/What should .* do\?/);
});

test("picking a kind shows 'All choices', the title and the subtitle, and no Save; 'All choices' goes back", () => {
  render(<ActionEditor keyName="Out" initialConfig={config()} />);
  fireEvent.click(screen.getAllByRole("radio")[KIND_ROWS.indexOf("OverrideToggle")]);
  expect(screen.getByText("All choices")).toBeTruthy();
  expect(screen.getByText(KIND_TEXT.OverrideToggle[0])).toBeTruthy();
  expect(screen.getByText("Press OUT and the wheel becomes a knob for:")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
  expect(screen.queryAllByRole("radio")).toHaveLength(0);
  fireEvent.click(screen.getByText("All choices"));
  expect(screen.getAllByRole("radio")).toHaveLength(KIND_ROWS.length);
});

test("every pick saves at once through save_config; Do nothing clears the key", async () => {
  const save = vi.fn(() => Promise.resolve(null));
  const onSaved = vi.fn();
  render(<ActionEditor keyName="Out" initialConfig={config()} save={save} onSaved={onSaved} />);
  fireEvent.click(screen.getAllByRole("radio")[KIND_ROWS.indexOf("OverrideToggle")]);
  fireEvent.click(screen.getByText("Screen brightness"));
  await vi.waitFor(() => expect(onSaved).toHaveBeenCalled());
  expect(save.mock.calls[0][0].layers[0].bindings.Out).toEqual({ OverrideToggle: { mode: "Brightness" } });
  cleanup();
  render(<ActionEditor keyName="Cut" initialConfig={config()} save={save} />);
  fireEvent.click(screen.getByText("Do nothing"));
  await vi.waitFor(() => expect(save).toHaveBeenCalledTimes(2));
  expect(save.mock.calls[1][0].layers[0].bindings).toEqual({});
  cleanup();
  const onSave = vi.fn(() => Promise.resolve({}));
  render(<KeyPanel keyName="Out" config={config()} layerId="base" onSave={onSave} initialMode={null} />);
  fireEvent.click(screen.getAllByRole("radio")[KIND_ROWS.indexOf("Held")]);
  expect(screen.getByText("The key stays held down for as long as you hold OUT.")).toBeTruthy();
  fireEvent.keyDown(screen.getByPlaceholderText("Click here, then press the shortcut"), { key: "k", ctrlKey: true });
  expect(onSave).toHaveBeenCalledWith("Held", { modifiers: ["Ctrl"], key: "K" });
});

test("a wheel button explains itself and offers 'Set up the wheel'; a way-back key is locked", () => {
  const onWheel = vi.fn();
  render(<KeyPanel keyName="Jog" config={config()} layerId="base" onWheel={onWheel} />);
  expect(screen.getByText("A wheel button")).toBeTruthy();
  expect(screen.getByText("JOG is one of the three buttons above the wheel. Pressing it changes what the wheel does.")).toBeTruthy();
  fireEvent.click(screen.getByText("Set up the wheel"));
  expect(onWheel).toHaveBeenCalled();
  cleanup();
  const onShowPage = vi.fn();
  render(<KeyPanel keyName="Cut" config={config()} layerId="edit" back={{ id: "base", name: "Main" }} onShowPage={onShowPage} />);
  expect(screen.getByText("This key takes you back to “Main”.")).toBeTruthy();
  expect(screen.getByText("It stays this way so you can never get stuck on this page.")).toBeTruthy();
  fireEvent.click(screen.getByText("Show the Main page"));
  expect(onShowPage).toHaveBeenCalledWith("base");
  expect(screen.queryAllByRole("radio")).toHaveLength(0);
});

const CH = [
  ["Hotkey", "keyboard", "Press a shortcut", "Like Copy, Save or Mute"],
  ["Held", "pan_tool", "Hold a key while I hold this one", "Good for Shift or Alt"],
  ["Latched", "toggle_on", "Turn a key on and off", "Press once to hold it, again to let go"],
  ["LaunchApp", "apps", "Open an app or website", "Spotify, a file, a web page…"],
  ["OverrideToggle", "volume_up", "Turn the wheel into…", "A volume or brightness knob"],
  ["LayerPush", "tab", "Switch to another set of keys", "Pages like Music or Browser"],
  ["Clear", "block", "Do nothing", "Clear this key"],
];

test("each choice row has the prototype's CH icon, title and subtitle; only the current kind has a check", () => {
  render(<ActionEditor keyName="Cut" initialConfig={config()} />);
  const rows = screen.getAllByRole("radio").map((r) => r.closest("label"));
  expect(rows).toHaveLength(KIND_ROWS.length);
  for (const [kind, icon, title, sub] of CH) {
    const row = rows[KIND_ROWS.indexOf(kind)];
    expect(row.querySelector("strong").textContent).toBe(title);
    expect(row.querySelector(".choice-sub").textContent).toBe(sub);
    expect(row.querySelector(`.choice-icon[data-icon="${icon}"] svg`)).toBeTruthy();
  }
  const checks = document.querySelectorAll(".choice-check svg");
  expect(checks).toHaveLength(1);
  expect(checks[0].closest("label")).toBe(rows[KIND_ROWS.indexOf("Hotkey")]);
  expect(document.body.innerHTML).not.toMatch(/<img|<link|url\(/);
});

test("the detail view uses the prototype's MT/MS title and subtitle with the key's name", () => {
  const MTMS = {
    Hotkey: ["Press a shortcut", "When you press CUT, freewheel presses this shortcut for you."],
    Held: ["Hold a key while I hold this one", "The key stays held down for as long as you hold CUT."],
    Latched: ["Turn a key on and off", "Press CUT once to hold the key down. Press it again to let go."],
    OverrideToggle: ["Turn the wheel into…", "Press CUT and the wheel becomes a knob for:"],
    LayerPush: ["Switch to another set of keys", "Pages are extra sets of keys, like “Music” or “Browser”. This key takes you to one."],
  };
  render(<KeyPanel keyName="Cut" config={config()} layerId="base" initialMode={null} />);
  for (const [kind, [title, sub]] of Object.entries(MTMS)) {
    fireEvent.click(screen.getAllByRole("radio")[KIND_ROWS.indexOf(kind)]);
    expect(document.querySelector(".mode-title").textContent).toBe(title);
    expect(document.querySelector(".mode-sub").textContent).toBe(sub);
    fireEvent.click(screen.getByText("All choices"));
  }
  fireEvent.click(screen.getAllByRole("radio")[KIND_ROWS.indexOf("LaunchApp")]);
  expect(document.querySelector(".mode-title").textContent).toBe("Open an app or website");
  expect(document.querySelector(".mode-sub")).toBeNull();
});

test("the permissions screen uses the prototype's titles and helper text", () => {
  const html = renderToString(<PermissionsScreen initial={{ Accessibility: "Missing", InputMonitoring: "Missing" }} button="Done" />);
  for (const t of [
    "freewheel needs your permission",
    "Allow freewheel to press keys for you",
    "Allow freewheel to notice your Speed Editor",
    "So it knows when you press a key. Opens Privacy &amp; Security → ",
  ])
    expect(html).toContain(t);
  expect(renderToString(<PermissionsScreen initial={{ Accessibility: "Granted" }} button="Start using freewheel" />)).toContain("Let freewheel do its job");
});

test("every switch knob is white with the prototype's small shadow, in light and dark", () => {
  const knob = base.match(/\.switch-knob \{([^}]*)\}/)[1];
  expect(knob).toContain("background: var(--key-white-sel);");
  expect(knob).toContain("box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);");
  expect(tokens).toMatch(/--key-white-sel:#ffffff/);
  // the dark blocks never redefine it
  for (const dark of tokens.match(/\[data-theme="dark"\][^}]*\}|@media \(prefers-color-scheme: dark\)\{[\s\S]*?\n\}/g)) expect(dark).not.toContain("--key-white-sel");
});

test("the toolbar has the prototype's tab icons; the status opens the permission fix only when one is missing", () => {
  const onFix = vi.fn();
  const state = { enabled: true, permissionMissing: true, connected: true };
  render(<Toolbar tab="Keys" onTab={() => {}} state={state} onFix={onFix} />);
  const icons = [...document.querySelectorAll(".tab")].map((t) => t.dataset.icon);
  expect(icons).toEqual(["keyboard", "trip_origin", "movie", "tune", "help"]);
  // each icon is a bundled SVG painted as a mask, never fetched
  for (const i of icons) expect(base).toContain(`.tab[data-icon="${i}"] { --mask: url("../assets/icons/${i}.svg"); }`);
  fireEvent.click(screen.getByText("Needs permission (click to fix)"));
  expect(onFix).toHaveBeenCalledTimes(1);
  cleanup();
  render(<Toolbar tab="Keys" onTab={() => {}} state={{ ...state, permissionMissing: false }} onFix={onFix} />);
  fireEvent.click(screen.getByText("Connected"));
  expect(onFix).toHaveBeenCalledTimes(1);
  cleanup();
  render(<NoticeBar state={{ ...state, enabled: false, permissionMissing: false }} />);
  expect(document.querySelector(".notice-icon").dataset.icon).toBe("power_settings_new");
});

test("the key panel header shows the keycap, 'When I press X…', what it does now and a Close icon", () => {
  const onClose = vi.fn();
  render(<KeyPanel keyName="Cut" action={config().layers[0].bindings.Cut} config={config()} layerId="base" onClose={onClose} />);
  expect(document.querySelector(".keycap").textContent).toBe("CUT");
  expect(screen.getByText("When I press CUT…")).toBeTruthy();
  expect(screen.getByText("Right now: Copy (Ctrl + C)")).toBeTruthy();
  expect(screen.getByText("Try it: press CUT on your Speed Editor.")).toBeTruthy();
  fireEvent.click(screen.getByTitle("Close"));
  expect(onClose).toHaveBeenCalled();
  cleanup();
  render(<KeyPanel keyName="Out" config={config()} layerId="base" initialMode={null} />);
  expect(screen.getByText("Not set yet")).toBeTruthy();
  expect(document.querySelector(".panel-try")).toBeNull();
});

test("'Pick a key' shows the prototype's copy, and the wheel tip only on an empty setup", () => {
  const empty = { version: 1, base_layer: "base", layers: [{ id: "base", name: "Main", bindings: {} }] };
  render(<ActionEditor keyName={null} initialConfig={empty} />);
  expect(screen.getByText("Pick a key")).toBeTruthy();
  expect(screen.getByText("Click any key in the drawing — or simply press it on your Speed Editor. Then choose what it does.")).toBeTruthy();
  expect(screen.getByText("Your wheel already scrolls, like a mouse wheel. Give it a spin!")).toBeTruthy();
  cleanup();
  render(<ActionEditor keyName={null} initialConfig={config()} />);
  expect(screen.queryByText("Your wheel already scrolls, like a mouse wheel. Give it a spin!")).toBeNull();
});

test("the record box shows Record, then Listening…, then keycaps, saving at once; a common shortcut saves too", async () => {
  const onSave = vi.fn(() => Promise.resolve({ config: config() }));
  render(<KeyPanel keyName="Out" config={config()} layerId="base" onSave={onSave} initialMode="Hotkey" />);
  expect(screen.getByText("Record")).toBeTruthy();
  expect(screen.getByText("Then press the keys together on your keyboard")).toBeTruthy();
  const input = screen.getByPlaceholderText("Click here, then press the shortcut");
  fireEvent.focus(input);
  expect(screen.getByText("Listening… press your shortcut")).toBeTruthy();
  expect(screen.getByText("Cancel")).toBeTruthy();
  // modifiers alone show live, and wait for the key
  fireEvent.keyDown(input, { key: "Control", ctrlKey: true });
  expect([...document.querySelectorAll(".keycap-lg")].map((k) => k.textContent)).toEqual(["Ctrl"]);
  expect(onSave).not.toHaveBeenCalled();
  fireEvent.keyDown(input, { key: "k", ctrlKey: true });
  expect(onSave).toHaveBeenCalledWith("Hotkey", { modifiers: ["Ctrl"], key: "K" });
  fireEvent.blur(input);
  expect([...document.querySelectorAll(".keycap-lg")].map((k) => k.textContent)).toEqual(["Ctrl", "K"]);
  expect(screen.getByText("Click to record again")).toBeTruthy();
  expect(screen.getByText("Or pick a common one")).toBeTruthy();
  expect([...document.querySelectorAll(".preset strong")].map((p) => p.textContent)).toEqual(["Copy", "Paste", "Undo", "Redo", "Save", "Play/Pause", "Mute"]);
  fireEvent.click(screen.getByText("Paste").closest("button"));
  expect(screen.getByText("Paste").closest("button").className).toContain("is-current");
  expect(onSave).toHaveBeenLastCalledWith("Hotkey", { modifiers: ["Ctrl"], key: "V" });
  expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
  cleanup();
  render(<KeyPanel keyName="Out" config={config()} layerId="base" initialMode="Held" />);
  expect(screen.getByText("Then press one key, like Shift")).toBeTruthy();
  expect(screen.queryByText("Or pick a common one")).toBeNull();
});

test("Held records one key, a lone modifier too", () => {
  const onSave = vi.fn(() => Promise.resolve({}));
  render(<KeyPanel keyName="Out" config={config()} layerId="base" onSave={onSave} initialMode="Held" />);
  fireEvent.keyDown(screen.getByPlaceholderText("Click here, then press the shortcut"), { key: "Shift", shiftKey: true });
  expect(onSave).toHaveBeenCalledWith("Held", { modifiers: ["Shift"], key: "" });
});

test("Held and Latched show the 'Quick picks' chips Shift, Ctrl/Cmd and Alt; Latched shows the lit-key card", () => {
  for (const kind of ["Held", "Latched"]) {
    render(<KeyPanel keyName="Out" config={config()} layerId="base" initialMode={kind} />);
    const title = screen.getByText("Quick picks");
    expect(title.className).toBe("field-label");
    expect(title.nextElementSibling.className).toBe("presets");
    // Not a Mac in jsdom: the platform modifier is Ctrl.
    expect([...document.querySelectorAll(".presets .preset strong")].map((p) => p.textContent)).toEqual(["Shift", "Ctrl", "Alt"]);
    expect(document.querySelectorAll(".preset-combo")).toHaveLength(0);
    expect(screen.queryByText("Or pick a common one")).toBeNull();
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
    const card = document.querySelector(".lit-card");
    if (kind === "Held") expect(card).toBeNull();
    else {
      expect(card.querySelector(".mini-key .mini-key-led")).toBeTruthy();
      expect(card.querySelector(".mini-key-name").textContent).toBe("OUT");
      expect(card.textContent).toContain("While it’s on, the light on this key stays lit — so you can always see it’s held down.");
    }
    cleanup();
  }
  render(<KeyPanel keyName="Out" config={config()} layerId="base" initialMode="Hotkey" />);
  expect(screen.queryByText("Quick picks")).toBeNull();
  expect(document.querySelector(".lit-card")).toBeNull();
  cleanup();
  for (const kind of ["LaunchApp", "LayerPush", "OverrideToggle"]) {
    render(<KeyPanel keyName="Out" config={config()} layerId="base" initialMode={kind} />);
    expect(screen.queryByText("Quick picks")).toBeNull();
    expect(document.querySelectorAll(".preset")).toHaveLength(0);
    cleanup();
  }
  expect(base).toMatch(/\.lit-card \{[^}]*background: var\(--sunk\)/);
  expect(base).toMatch(/\.mini-key-led \{[^}]*background: var\(--led-lit\)/);
});

test("a Quick pick sets the combo to that modifier alone, saves it and shows as active", async () => {
  for (const kind of ["Held", "Latched"]) {
    const onSave = vi.fn(() => Promise.resolve({ config: config() }));
    render(<KeyPanel keyName="Out" config={config()} layerId="base" onSave={onSave} initialMode={kind} />);
    for (const [name, modifier] of [["Shift", "Shift"], ["Ctrl", "Ctrl"], ["Alt", "Alt"]]) {
      fireEvent.click(screen.getByText(name).closest("button"));
      expect(onSave).toHaveBeenLastCalledWith(kind, { modifiers: [modifier], key: "" });
      const active = [...document.querySelectorAll(".preset.is-current")].map((b) => b.textContent);
      expect(active).toEqual([name]);
    }
    expect(onSave).toHaveBeenCalledTimes(3);
    cleanup();
  }
  // Through the editor: pick Held, tap Shift, and save_config gets the lone modifier.
  const save = vi.fn(() => Promise.resolve());
  render(<ActionEditor keyName="Out" initialConfig={config()} save={save} />);
  fireEvent.click(screen.getByText(KIND_TEXT.Held[0]));
  fireEvent.click(screen.getByText("Shift").closest("button"));
  await vi.waitFor(() => expect(save).toHaveBeenCalled());
  expect(save.mock.calls[0][0].layers[0].bindings.Out).toEqual({ Held: { combo: { modifiers: ["Shift"], key: "" } } });
  expect(await screen.findByText("Saved. Press OUT on your Speed Editor to try it.")).toBeTruthy();
});

test("the Ctrl/Cmd quick pick is Cmd (Meta) on a Mac and Ctrl elsewhere", async () => {
  vi.resetModules();
  vi.stubGlobal("navigator", { userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)" });
  try {
    const { QUICK_PICKS, KeyPanel: MacPanel } = await import("./ActionEditor.jsx");
    expect(QUICK_PICKS.map(([, c]) => c.modifiers)).toEqual([["Shift"], ["Meta"], ["Alt"]]);
    // The chips read as the prototype's kn() on a Mac: ⇧ ⌘ ⌥.
    expect(QUICK_PICKS.map(([label]) => label)).toEqual(["⇧", "⌘", "⌥"]);
    const onSave = vi.fn(() => Promise.resolve({ config: config() }));
    render(<MacPanel keyName="Out" config={config()} layerId="base" onSave={onSave} initialMode="Held" />);
    expect([...document.querySelectorAll(".presets .preset strong")].map((p) => p.textContent)).toEqual(["⇧", "⌘", "⌥"]);
    fireEvent.click(screen.getByText("⌘").closest("button"));
    expect(onSave).toHaveBeenLastCalledWith("Held", { modifiers: ["Meta"], key: "" });
    cleanup();
  } finally {
    vi.unstubAllGlobals();
    vi.resetModules();
  }
  const { QUICK_PICKS } = await import("./ActionEditor.jsx");
  expect(QUICK_PICKS.map(([, c]) => c.modifiers)).toEqual([["Shift"], ["Ctrl"], ["Alt"]]);
});

test("the in-settings Permissions section shows one .card per permission with the PermissionsScreen copy and a .btn-primary", () => {
  const invoke = vi.fn(() => Promise.resolve(null));
  vi.stubGlobal("window", { ...window, __TAURI__: { core: { invoke } } });
  try {
    render(<Permissions initial={{ Accessibility: "Missing", InputMonitoring: "Granted" }} />);
    const cards = [...document.querySelectorAll("#permissions .card")];
    expect(cards).toHaveLength(2);
    expect(cards[0].querySelector(".card-title").textContent).toBe("Allow freewheel to press keys for you");
    expect(cards[0].querySelector(".card-sub").textContent).toBe(
      "So your Speed Editor can send shortcuts and change the volume. Opens Privacy & Security → Accessibility.",
    );
    expect(cards[1].querySelector(".card-title").textContent).toBe("Allow freewheel to notice your Speed Editor");
    expect(cards[1].querySelector(".card-sub").textContent).toBe("So it knows when you press a key. Opens Privacy & Security → Input Monitoring.");
    expect(cards[1].querySelector('.allowed [data-icon="check_circle"] svg')).toBeTruthy();
    expect(cards[1].querySelector(".allowed").textContent.trim()).toBe("Allowed");
    expect(cards[1].querySelector("button")).toBeNull();
    const open = cards[0].querySelector("button.btn-primary");
    expect(open.textContent).toBe("Open System Settings");
    fireEvent.click(open);
    expect(invoke).toHaveBeenCalledWith("open_permission_pane", { permission: "Accessibility" });
  } finally {
    cleanup();
    vi.unstubAllGlobals();
  }
});

test("deleting a page asks inline first, with the prototype's copy", () => {
  const onDelete = vi.fn();
  render(<PageSelector config={config()} page="edit" onDelete={onDelete} />);
  fireEvent.click(screen.getByTitle("Delete page"));
  expect(screen.getByText("Delete “Editing”? Its keys will be cleared.")).toBeTruthy();
  fireEvent.click(screen.getByText("Keep it"));
  expect(onDelete).not.toHaveBeenCalled();
  fireEvent.click(screen.getByTitle("Delete page"));
  fireEvent.click(screen.getByText("Delete"));
  expect(onDelete).toHaveBeenCalledTimes(1);
  cleanup();
  render(<PageSelector config={{ ...config(), layers: config().layers.slice(0, 1) }} page="base" />);
  expect(screen.getByText("What each key does")).toBeTruthy();
  expect(screen.queryByRole("tablist")).toBeNull();
});

test("Help & backup confirms a backup inline with a check, as the prototype does", async () => {
  vi.stubGlobal("window", { ...window, __TAURI__: { core: { invoke: vi.fn(async () => true) } } });
  try {
    render(<Backup />);
    fireEvent.click(screen.getByText("Back up…"));
    const line = await screen.findByText("Your keys are backed up.");
    expect(line.closest(".success-line").querySelector('[data-icon="check_circle"] svg')).toBeTruthy();
  } finally {
    vi.unstubAllGlobals();
  }
});

test("Help & backup shows the prototype's heading, section and copy", () => {
  const html = renderToString(<Backup />);
  for (const t of [
    "Help &amp; backup",
    "Back up my keys",
    "Keep a copy of all your keys and pages in one file — handy for a new computer.",
    ">Back up…</button>",
    ">Restore from a backup…</button>",
  ])
    expect(html).toContain(t);
});
