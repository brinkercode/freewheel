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
  // The media keys, sent as the OS media keys (core inject.rs MEDIA_KEYS).
  MediaPlayPause: "PlayPause", AudioVolumeMute: "Mute", AudioVolumeUp: "VolumeUp",
  AudioVolumeDown: "VolumeDown", MediaTrackNext: "NextTrack", MediaTrackPrevious: "PrevTrack",
};
/** What a media key is called on screen, as the prototype names it. */
export const MEDIA_NAMES = {
  PlayPause: "Play/Pause", Mute: "Mute", VolumeUp: "Volume up", VolumeDown: "Volume down",
  NextTrack: "Next track", PrevTrack: "Previous track",
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

const MAC = /Mac/.test(globalThis.navigator?.userAgent ?? "");
const MOD = MAC ? "Meta" : "Ctrl";
// The prototype's named shortcuts: a combo shows by its name, like "Undo".
const NAMED_COMBOS = [
  ["Copy", [MOD], "C"],
  ["Paste", [MOD], "V"],
  ["Undo", [MOD], "Z"],
  ["Redo", MAC ? [MOD, "Shift"] : [MOD], MAC ? "Z" : "Y"],
  ["Save", [MOD], "S"],
  ...Object.entries(MEDIA_NAMES).map(([key, name]) => [name, [], key]),
];
const MAC_KEYS = { Meta: "⌘", Ctrl: "⌃", Shift: "⇧", Alt: "⌥" };
/** A key as the prototype's kn() names it: ⌘ ⌃ ⇧ ⌥ on a Mac, Ctrl Shift Alt Win elsewhere. */
export const keyName = (k) => MEDIA_NAMES[k] ?? (MAC ? MAC_KEYS[k] : { Meta: "Win" }[k]) ?? k;
/** The combo's keys, each named as keyName does. */
export const comboKeys = (combo) => [...combo.modifiers, combo.key].filter(Boolean).map(keyName);
/** "Ctrl + Z" (the prototype's combo with a separator). */
export const comboText = (combo, sep = MAC ? "" : "+") => comboKeys(combo).join(sep);

/** The prototype's name for a combo ("Undo"), or null. */
export function comboName(combo) {
  const text = formatCombo(combo);
  return NAMED_COMBOS.find(([, mods, key]) => formatCombo({ modifiers: mods, key }) === text)?.[0] ?? null;
}

/** An app's name from its path: "C:\\…\\Spotify.exe" is Spotify; a web address stays as it is. */
export function appName(path) {
  const p = (path ?? "").trim();
  if (/^(https?:\/\/|www\.)|^[a-z0-9-]+(\.[a-z0-9-]+)+(\/.*)?$/i.test(p)) return p.replace(/^https?:\/\//, "").replace(/\/$/, "");
  const base = p.split(/[\\/]/).filter(Boolean).pop() ?? "";
  const name = base.replace(/\.(exe|app|lnk|bat|cmd)$/i, "");
  return name.charAt(0).toUpperCase() + name.slice(1);
}

/** What a LaunchApp opens: the website it opens in the browser, else the app's name. */
export function launchName(v) {
  // A website, a Start Menu shortcut or a .app opens through the OS opener: name what it opens.
  if (/(^|[\\/])(explorer\.exe|open)$/i.test(v.path) && v.args?.[0]) return appName(v.args[0]);
  return appName(v.path);
}

const WK = { SystemVolume: "a volume knob", AppVolume: "an app volume knob", Brightness: "a brightness knob" };
const WKS = { SystemVolume: "Volume knob", AppVolume: "App volume", Brightness: "Brightness" };
export const JOG_LABEL = { Jog: "JOG", Shuttle: "SHTL", Scroll: "SCRL", Volume: "Volume", Brightness: "Brightness" };

/**
 * The page a key on `pageId` takes you back to: a key that goes to this page
 * from another page goes back when pressed again here (engine.rs push_layer).
 */
export function backTarget(config, pageId, key) {
  const page = config.layers.find((l) => l.id === pageId);
  if (!page || page.bindings[key]) return null;
  return config.layers.find((l) => l.id !== pageId && l.bindings[key]?.LayerPush?.target === pageId) ?? null;
}

/** What a key does, short, for the drawing (the prototype's shortL). */
export function shortLabel(action, config) {
  if (!action) return "Not set";
  const [kind, v] = Object.entries(action)[0];
  switch (kind) {
    case "Hotkey":
      return comboName(v.combo) ?? comboText(v.combo);
    case "Held":
      return `Hold ${comboText(v.combo)}`;
    case "Latched":
      return `${comboText(v.combo)} on/off`;
    case "LaunchApp": {
      const opens = `Opens ${launchName(v).replace(/^(Google|Microsoft) /, "").replace(" Studio", "")}`;
      return v.enabled === false ? `${opens} (off)` : opens;
    }
    case "LayerPush":
      return `→ ${pageName(config, v.target)}`;
    case "OverrideToggle":
      return WKS[v.mode];
    case "SetJogMode":
      return `Wheel: ${JOG_LABEL[v.mode] ?? v.mode}`;
    default:
      return describeAction(action, config);
  }
}

/** What a key does, in a sentence, for the list and the key panel (the prototype's longL). */
export function longLabel(action, config) {
  if (!action) return "Not set";
  const [kind, v] = Object.entries(action)[0];
  switch (kind) {
    case "Hotkey": {
      const name = comboName(v.combo);
      const keys = comboText(v.combo, " + ");
      if (name) return name === keys ? name : `${name} (${keys})`;
      return `Presses ${keys}`;
    }
    case "Held":
      return `Holds ${comboText(v.combo, " + ")} while held`;
    case "Latched":
      return `Turns ${comboText(v.combo, " + ")} on and off`;
    case "LaunchApp":
      return v.enabled === false ? `Opens ${launchName(v)} (off: restored from a file)` : `Opens ${launchName(v)}`;
    case "LayerPush":
      return `Goes to the ${pageName(config, v.target)} page`;
    case "OverrideToggle":
      return `Turns the wheel into ${WK[v.mode]}`;
    case "SetJogMode":
      return `Switches the wheel to ${JOG_LABEL[v.mode] ?? v.mode}`;
    default:
      return describeAction(action, config);
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
