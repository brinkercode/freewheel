// The Keys tab's model: the Speed Editor's keys, the pages (config layers),
// the app profiles (each with its own pages; the pages no profile has are
// the Default profile) and the key-combo capture. The UI calls layers
// "pages" everywhere; "profile" is only for app profiles.

import { iconName } from "./symbols.js";

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
// What a mappings file may also press (core mappings_file/names.rs, SCHEMA.md).
const FILE_KEYS = [
  ...Array.from({ length: 12 }, (_, i) => `F${i + 13}`),
  ...Array.from({ length: 10 }, (_, i) => `Num${i}`),
  "NumAdd", "NumSubtract", "NumMultiply", "NumDivide", "NumDecimal", "NumEnter", "NumLock", "ScrollLock",
  "Insert", "PrintScreen", "Pause", "CapsLock",
];
const SENDABLE = new Set([...Object.values(NAMED), ...FILE_KEYS]);

/** A character in upper case, unless that is more than one character ("ß" stays "ß", not "SS"). */
const upperOne = (c) => {
  const up = c.toUpperCase();
  return up.length === 1 ? up : c;
};

/** The combo a keydown records: its modifiers plus one key ("" while only modifiers are down). */
export function comboFromEvent(e) {
  const modifiers = MODIFIERS.filter(([flag]) => e[flag]).map(([, name]) => name);
  if (MODIFIER_KEYS.includes(e.key)) return { modifiers, key: "" };
  const key = NAMED[e.key] ?? (e.key.length === 1 ? upperOne(e.key) : e.key);
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
const LINUX = /Linux/.test(globalThis.navigator?.userAgent ?? "") && !/Android/.test(globalThis.navigator?.userAgent ?? "");
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
/** A key as the prototype's kn() names it: ⌘ ⌃ ⇧ ⌥ on a Mac, Ctrl Shift Alt Win on Windows (Super on Linux). */
export const keyName = (k) => MEDIA_NAMES[k] ?? (MAC ? MAC_KEYS[k] : { Meta: LINUX ? "Super" : "Win" }[k]) ?? k;
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
  let name = base.replace(/\.(exe|app|lnk|bat|cmd|AppImage)$/i, "");
  // A Linux desktop entry: "org.gnome.Nautilus.desktop" is Nautilus.
  if (/\.desktop$/i.test(name)) name = name.replace(/\.desktop$/i, "").split(".").pop();
  return name.charAt(0).toUpperCase() + name.slice(1);
}

/** What a LaunchApp opens: the website it opens in the browser, else the app's name. */
export function launchName(v) {
  // A website, a Start Menu shortcut, a .app or a Linux file opens through the OS opener: name what it opens.
  if (/(^|[\\/])(explorer\.exe|open|xdg-open)$/i.test(v.path) && v.args?.[0]) return appName(v.args[0]);
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

/** The profile page `layerId` belongs to (null: the Default profile). */
export const profileOf = (config, layerId) => (config.profiles ?? []).find((p) => p.pages.includes(layerId))?.id ?? null;

/** The pages of profile `profileId` (null: the Default profile's), in order. */
export function pagesOf(config, profileId = null) {
  const profile = profileId && (config.profiles ?? []).find((p) => p.id === profileId);
  if (profile) return profile.pages.map((id) => config.layers.find((l) => l.id === id)).filter(Boolean);
  return config.layers.filter((l) => !profileOf(config, l.id));
}

/** The first page of profile `profileId` (null: the Default profile's). */
export const firstPage = (config, profileId = null) =>
  profileId ? ((config.profiles ?? []).find((p) => p.id === profileId)?.pages[0] ?? config.base_layer) : config.base_layer;

/** The config with these profiles (none: `profiles` left out, as config.json does). */
function withProfiles(config, profiles) {
  const { profiles: _old, ...rest } = config;
  return profiles.length ? { ...rest, profiles } : rest;
}

const freshId = (taken, base) => {
  let id = base;
  for (let n = 2; taken(id); n += 1) id = `${base}-${n}`;
  return id;
};

/**
 * Adds a page named "Page N" to profile `profileId` (null: Default), N the
 * first number from its page count + 1 that no page of the profile is named;
 * returns the new config and the page's id.
 */
export function addPage(config, profileId = null) {
  const pages = pagesOf(config, profileId);
  let n = pages.length + 1;
  while (config.layers.some((l) => l.id === `page-${n}`)) n += 1;
  const id = `page-${n}`;
  let k = pages.length + 1;
  while (pages.some((l) => l.name.trim().toLowerCase() === `page ${k}`)) k += 1;
  const added = withLayers(config, [...config.layers, { id, name: `Page ${k}`, bindings: {} }]);
  if (!profileId) return { config: added, id };
  const profiles = (config.profiles ?? []).map((p) => (p.id === profileId ? { ...p, pages: [...p.pages, id] } : p));
  return { config: withProfiles(added, profiles), id };
}

export const renamePage = (config, id, name) =>
  withLayers(config, config.layers.map((l) => (l.id === id ? { ...l, name } : l)));

/** Deletes a page; `error` says why it cannot be: a profile's first page or a page a key goes to. */
export function deletePage(config, id) {
  if (id === config.base_layer || (config.profiles ?? []).some((p) => p.pages[0] === id)) return { error: "The first page can't be deleted." };
  for (const layer of config.layers) {
    if (layer.id === id) continue;
    for (const [key, action] of Object.entries(layer.bindings)) {
      if (action?.LayerPush?.target === id)
        return {
          error: `${pageName(config, id)} can't be deleted: ${LABELS[key] ?? key} on ${layer.name} goes to it. Change that key first.`,
        };
    }
  }
  const layers = withLayers(config, config.layers.filter((l) => l.id !== id));
  if (!profileOf(config, id)) return { config: layers };
  return { config: withProfiles(layers, config.profiles.map((p) => ({ ...p, pages: p.pages.filter((l) => l !== id) }))) };
}

/** What the Keys tab calls the pages no profile has. */
export const DEFAULT_PROFILE = "Default";

/** A profile's colours (config.rs PROFILE_COLORS); Default uses the app's accent. */
export const PROFILE_COLORS = ["rose", "orange", "amber", "green", "teal", "blue", "indigo", "violet"];
/** The icons the profile editor suggests (config.rs SUGGESTED_ICONS); any bundled icon name works (src/symbols.js). */
export const SUGGESTED_ICONS = ["brush", "code", "music_note", "sports_esports", "movie", "edit", "photo_camera", "terminal"];
/** Default's icon. */
export const DEFAULT_ICON = "keyboard";
/** The icon a profile without one (or with a name the font lacks) shows. */
export const PLAIN_ICON = "tab";

/** How profile `id` looks (null: Default): { color: a PROFILE_COLORS name or "accent", icon: a bundled icon name }. */
export function profileLook(config, id) {
  const p = id && (config.profiles ?? []).find((x) => x.id === id);
  if (!p) return { color: "accent", icon: DEFAULT_ICON };
  return { color: PROFILE_COLORS.includes(p.color) ? p.color : "accent", icon: iconName(p.icon) ?? PLAIN_ICON };
}

/** The config with profile `id`'s colour and/or icon set. */
export const setProfileLook = (config, id, look) =>
  withProfiles(
    config,
    (config.profiles ?? []).map((p) => (p.id === id ? { ...p, ...look } : p)),
  );

/** Adds a profile "Profile N" with one empty page "Main" and the next colour and icon; returns the config, its id and its page's id. */
export function addProfile(config) {
  const profiles = config.profiles ?? [];
  let n = profiles.length + 1;
  while (profiles.some((p) => p.name.toLowerCase() === `profile ${n}`)) n += 1;
  const id = freshId((x) => profiles.some((p) => p.id === x), `profile-${n}`);
  const page = freshId((x) => config.layers.some((l) => l.id === x), `${id}-main`);
  const next = withLayers(config, [...config.layers, { id: page, name: "Main", bindings: {} }]);
  const k = profiles.length;
  const profile = { id, name: `Profile ${n}`, pages: [page], color: PROFILE_COLORS[k % PROFILE_COLORS.length], icon: SUGGESTED_ICONS[k % SUGGESTED_ICONS.length] };
  return { config: withProfiles(next, [...profiles, profile]), id, page };
}

export const renameProfile = (config, id, name) =>
  withProfiles(config, (config.profiles ?? []).map((p) => (p.id === id ? { ...p, name } : p)));

/** Deletes a profile and its pages (Default can't be deleted). */
export function deleteProfile(config, id) {
  const profile = (config.profiles ?? []).find((p) => p.id === id);
  if (!profile) return { error: "The Default profile can't be deleted." };
  const layers = withLayers(config, config.layers.filter((l) => !profile.pages.includes(l.id)));
  return { config: withProfiles(layers, config.profiles.filter((p) => p.id !== id)) };
}

/** Why a profile name can't be used, or null. */
export function profileNameError(config, id, name) {
  const n = name.trim().toLowerCase();
  if (!n) return "Give the profile a name.";
  if (n === DEFAULT_PROFILE.toLowerCase()) return "Default is the profile for every other app: pick another name.";
  if ((config.profiles ?? []).some((p) => p.id !== id && p.name.trim().toLowerCase() === n)) return `There is already a profile named ${name.trim()}.`;
  return null;
}

/** The apps a profile is for. */
export const profileApps = (config, id) => (config.profiles ?? []).find((p) => p.id === id)?.apps ?? [];

/** The config with profile `id`'s apps replaced (an empty list leaves `apps` out, as config.json does). */
export const setProfileApps = (config, id, apps) =>
  withProfiles(
    config,
    (config.profiles ?? []).map((p) => {
      if (p.id !== id) return p;
      const { apps: _old, ...rest } = p;
      return apps.length ? { ...rest, apps } : rest;
    }),
  );

/** The OS this window runs on, as a profile app names it. */
export const THIS_OS = MAC ? "macos" : LINUX ? "linux" : "windows";

/** What a profile app is called on `os` (null: not set for that OS). */
export const appOnOs = (app, os = THIS_OS) => app?.[os]?.trim() || null;

/** A profile app typed by hand: its program name on this OS. */
export const typedApp = (name, os = THIS_OS) => ({ name: name.trim(), [os]: name.trim() });

/** Whether two profile apps are the same on `os` (names ignore case, as the matching does). */
export const sameApp = (a, b, os = THIS_OS) => {
  const x = appOnOs(a, os);
  const y = appOnOs(b, os);
  return Boolean(x && y) && x.toLowerCase().replace(/\.exe$/, "") === y.toLowerCase().replace(/\.exe$/, "");
};

/** How the profile editor names this OS's program names, with an example. */
export const APP_NAME_HINT = {
  windows: ["the program file", "notepad.exe"],
  macos: ["the bundle id", "com.apple.TextEdit"],
  linux: ["the program or its window class", "gedit"],
};

export const setBinding = (config, pageId, key, action) =>
  withLayers(
    config,
    config.layers.map((l) => (l.id === pageId ? { ...l, bindings: { ...l.bindings, [key]: action } } : l)),
  );
