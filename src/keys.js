// The Keys tab's model: the Speed Editor's keys, the pages (config layers)
// and the key-combo capture. The UI calls layers "pages" everywhere.

/** Binding name (config) to printed label, as core status.rs printed_label. */
export const LABELS = {
  SmartInsrt: "SMART INSRT", Appnd: "APPND", RiplOwr: "RIPL O/WR", CloseUp: "CLOSE UP",
  PlaceOnTop: "PLACE ON TOP", SrcOwr: "SRC O/WR", In: "IN", Out: "OUT", TrimIn: "TRIM IN",
  TrimOut: "TRIM OUT", Roll: "ROLL", SlipSrc: "SLIP SRC", SlipDest: "SLIP DEST",
  TransDur: "TRANS DUR", Cut: "CUT", Dis: "DIS", SmthCut: "SMTH CUT", Source: "SOURCE",
  Timeline: "TIMELINE", Shtl: "SHTL", Jog: "JOG", Scrl: "SCRL", Esc: "ESC", SyncBin: "SYNC BIN",
  AudioLevel: "AUDIO LEVEL", FullView: "FULL VIEW", Trans: "TRANS", Split: "SPLIT", Snap: "SNAP",
  RiplDel: "RIPL DEL", Cam1: "CAM 1", Cam2: "CAM 2", Cam3: "CAM 3", Cam4: "CAM 4", Cam5: "CAM 5",
  Cam6: "CAM 6", Cam7: "CAM 7", Cam8: "CAM 8", Cam9: "CAM 9", LiveOwr: "LIVE O/WR",
  VideoOnly: "VIDEO ONLY", AudioOnly: "AUDIO ONLY", StopPlay: "STOP/PLAY",
};

/** The device drawing: three blocks of rows, as the keys sit on the Speed Editor. */
export const DRAWING = [
  [
    ["SmartInsrt", "Appnd", "RiplOwr"],
    ["CloseUp", "PlaceOnTop", "SrcOwr"],
    ["In", "Out"],
    ["TrimIn", "TrimOut", "Roll"],
    ["SlipSrc", "SlipDest", "TransDur"],
    ["Cut", "Dis", "SmthCut"],
  ],
  [
    ["Esc", "SyncBin", "AudioLevel", "FullView"],
    ["Trans", "Split", "Snap", "RiplDel"],
    ["Cam7", "Cam8", "Cam9", "LiveOwr"],
    ["Cam4", "Cam5", "Cam6", "VideoOnly"],
    ["Cam1", "Cam2", "Cam3", "AudioOnly"],
    ["StopPlay"],
  ],
  [
    ["Source", "Timeline"],
    ["Shtl", "Jog", "Scrl"],
  ],
];

export const KEYS = Object.keys(LABELS);
export const COMBO_KINDS = ["Hotkey", "Held", "Latched"];
export const PRESS_CHIP_MS = 2000;

const MODIFIERS = [["ctrlKey", "Ctrl"], ["shiftKey", "Shift"], ["altKey", "Alt"], ["metaKey", "Meta"]];
const MODIFIER_KEYS = ["Control", "Shift", "Alt", "Meta", "OS", "AltGraph"];
// KeyboardEvent.key to the names the injector (src-tauri inject.rs) sends.
const NAMED = {
  Enter: "Enter", Tab: "Tab", " ": "Space", Escape: "Escape", Backspace: "Backspace",
  Delete: "Delete", Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown",
  ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right",
  ...Object.fromEntries(Array.from({ length: 12 }, (_, i) => [`F${i + 1}`, `F${i + 1}`])),
};
const SENDABLE = new Set(Object.values(NAMED));

/** The combo a keydown records: its modifiers plus one key ("" while only modifiers are down). */
export function comboFromEvent(e) {
  const modifiers = MODIFIERS.filter(([flag]) => e[flag]).map(([, name]) => name);
  if (MODIFIER_KEYS.includes(e.key)) return { modifiers, key: "" };
  const key = NAMED[e.key] ?? (e.key.length === 1 ? e.key.toUpperCase() : e.key);
  return { modifiers, key };
}

/** "Ctrl+Shift+K". */
export const formatCombo = (combo) => [...combo.modifiers, combo.key].filter(Boolean).join("+");

/** Why a combo cannot be saved for `kind`, or null. Held and Latched may hold one modifier alone. */
export function comboError(combo, kind = "Hotkey") {
  if (!combo || (!combo.key && combo.modifiers.length === 0)) return "Press the shortcut you want first.";
  if (!combo.key && kind !== "Hotkey" && combo.modifiers.length === 1) return null;
  if (!combo.key) return "Add a key to go with the modifier, like Ctrl+K.";
  if (combo.key.length !== 1 && !SENDABLE.has(combo.key)) return `freewheel can't send the ${combo.key} key.`;
  return null;
}

const pageName = (config, id) => config.layers.find((l) => l.id === id)?.name ?? id;

/** What a key does, in plain words, for the button list and the drawing. */
export function describeAction(action, config) {
  if (!action) return "Not set";
  const [kind, v] = Object.entries(action)[0];
  switch (kind) {
    case "Hotkey":
      return formatCombo(v.combo);
    case "Held":
      return `Hold ${formatCombo(v.combo)}`;
    case "Latched":
      return `Lock ${formatCombo(v.combo)}`;
    case "Scroll":
      return v.amount < 0 ? "Scroll up" : "Scroll down";
    case "LaunchApp":
      return "Open an app";
    case "LayerPush":
      return `Go to page ${pageName(config, v.target)}`;
    case "SetJogMode":
      return `Wheel: ${v.mode}`;
    case "OverrideToggle":
      return { SystemVolume: "Wheel: volume", AppVolume: "Wheel: app volume", Brightness: "Wheel: brightness" }[v.mode];
    default:
      return kind;
  }
}

const withLayers = (config, layers) => ({ ...config, layers });

/** Adds a page named "Page N"; returns the new config and the page's id. */
export function addPage(config) {
  let n = config.layers.length + 1;
  while (config.layers.some((l) => l.id === `page-${n}`)) n += 1;
  const id = `page-${n}`;
  return { config: withLayers(config, [...config.layers, { id, name: `Page ${n}`, bindings: {} }]), id };
}

export const renamePage = (config, id, name) =>
  withLayers(config, config.layers.map((l) => (l.id === id ? { ...l, name } : l)));

/** Deletes a page; `error` says why it cannot be: the first page or a page a key goes to. */
export function deletePage(config, id) {
  if (id === config.base_layer) return { error: "The first page can't be deleted." };
  for (const layer of config.layers) {
    if (layer.id === id) continue;
    for (const [key, action] of Object.entries(layer.bindings)) {
      if (action?.LayerPush?.target === id)
        return {
          error: `${pageName(config, id)} can't be deleted: ${LABELS[key] ?? key} on ${layer.name} goes to it. Change that key first.`,
        };
    }
  }
  return { config: withLayers(config, config.layers.filter((l) => l.id !== id)) };
}

export const setBinding = (config, pageId, key, action) =>
  withLayers(
    config,
    config.layers.map((l) => (l.id === pageId ? { ...l, bindings: { ...l.bindings, [key]: action } } : l)),
  );
