import { useEffect, useRef, useState } from "react";
import Icon, { iconSvg } from "./Icon.jsx";
import {
  COMBO_KINDS,
  LABELS,
  addPage,
  comboError,
  comboFromEvent,
  comboKeys,
  formatCombo,
  longLabel,
  pagesOf,
  profileOf,
  setBinding,
} from "./keys.js";
import { appIcons, chooseApp, getConfig, listApps, saveConfig, saveErrorText } from "./settingsApi.js";

// The key panel ("When I press X…") of the design handoff (screens-and-components
// 3b): the choices list, each choice's detail, the record box and the footer.
// Layers are "pages" in every visible string; "profile" is only for app profiles.

export const KINDS = {
  LaunchApp: "Open an app",
  LayerPush: "Go to a page",
  SetJogMode: "Set the wheel mode",
  OverrideToggle: "Turn the wheel into a dial",
};

export const JOG_MODES = { Jog: "Jog", Shuttle: "Shuttle", Scroll: "Scroll" };

// Windows and macOS (the only supported OSes) both ship all three controls:
// ISimpleAudioVolume on Windows, the process tap on macOS.
export const OVERRIDE_MODES = { SystemVolume: "System volume", AppVolume: "App volume", Brightness: "Brightness" };

const MAC = /Mac/.test(globalThis.navigator?.userAgent ?? "");
const LINUX = /Linux/.test(globalThis.navigator?.userAgent ?? "") && !/Android/.test(globalThis.navigator?.userAgent ?? "");
const kindOf = (action) => (action ? Object.keys(action)[0] : null);

/** A new action of `kind`, preset to its first valid choice. */
export function blank(kind, config, layerId) {
  switch (kind) {
    case "LaunchApp":
      return { LaunchApp: { path: "", args: [], enabled: true } };
    case "LayerPush":
      return { LayerPush: { target: pagesOf(config, profileOf(config, layerId)).find((l) => l.id !== layerId)?.id ?? "" } };
    case "SetJogMode":
      return { SetJogMode: { mode: "Scroll" } };
    default:
      return { OverrideToggle: { mode: "SystemVolume" } };
  }
}

/** Inline errors by field; empty when the action can be saved. */
export function validate(action, config, layerId) {
  const kind = kindOf(action);
  const v = action[kind];
  const errors = {};
  if (kind === "LaunchApp" && !v.path.trim()) errors.path = "Choose the app to open.";
  if (kind === "LayerPush" && (v.target === layerId || !pagesOf(config, profileOf(config, layerId)).some((l) => l.id === v.target)))
    errors.target = "Pick another page.";
  if (kind === "SetJogMode" && !(v.mode in JOG_MODES)) errors.mode = "Pick a wheel mode.";
  if (kind === "OverrideToggle" && !(v.mode in OVERRIDE_MODES)) errors.mode = "Pick what the wheel controls.";
  return errors;
}

/** The config with `key` on page `layerId` bound to `action`. */
export function withBinding(config, layerId, key, action) {
  return {
    ...config,
    layers: config.layers.map((l) => (l.id === layerId ? { ...l, bindings: { ...l.bindings, [key]: action } } : l)),
  };
}

/** The config with `key` on page `layerId` cleared ("Do nothing"). */
export function withoutBinding(config, layerId, key) {
  return {
    ...config,
    layers: config.layers.map((l) => {
      if (l.id !== layerId) return l;
      const bindings = { ...l.bindings };
      delete bindings[key];
      return { ...l, bindings };
    }),
  };
}

/**
 * Validates `action`, then saves the config with it through `save`
 * (save_config). Resolves to { errors } and, once saved, the new config.
 */
export async function saveAction({ config, layerId, key, action, save = saveConfig }) {
  const errors = validate(action, config, layerId);
  if (Object.keys(errors).length) return { errors };
  // The Arguments box keeps empty lines while typing; they are no argument.
  const saved = action.LaunchApp ? { LaunchApp: { ...action.LaunchApp, args: action.LaunchApp.args.filter(Boolean) } } : action;
  const next = withBinding(config, layerId, key, saved);
  try {
    await save(next);
    return { errors: {}, config: next };
  } catch (e) {
    console.error("freewheel: saving the action for", key, "failed:", JSON.stringify(e));
    return { errors: { save: saveErrorText(e) } };
  }
}

// A website opens in the default browser through the OS opener.
const OPENER = MAC ? "/usr/bin/open" : LINUX ? "xdg-open" : "explorer.exe";
/** The OS opener for a user agent: open on a Mac, xdg-open on Linux, Explorer on Windows. */
export const openerFor = (ua) => (/Mac/.test(ua) ? "/usr/bin/open" : /Linux/.test(ua) && !/Android/.test(ua) ? "xdg-open" : "explorer.exe");
/** The LaunchApp that opens `url` (like "youtube.com") in the default browser. */
export const websiteAction = (url, opener = OPENER) => {
  const u = url.trim().replace(/^https?:\/\//, "").replace(/\/$/, "");
  return { LaunchApp: { path: opener, args: [`https://${u}`], enabled: true } };
};

/** An app's avatar colour, one of 12 hues (the stylesheet’s .app-initial[data-hue]), from its name. */
export function appHue(name) {
  let h = 0;
  for (const ch of name) h = (h * 31 + ch.codePointAt(0)) % 12;
  return h;
}

/** The apps whose name contains `query`, ignoring case. */
export const filterApps = (apps, query) => {
  const q = query.trim().toLowerCase();
  return q ? apps.filter((a) => a.name.toLowerCase().includes(q)) : apps;
};

/** The file whose icon an app shows: the .lnk / .app it opens, or the program itself (a Linux .desktop entry). */
export const iconSource = (app) => app.args[0] ?? app.path;

const sameLaunch = (v, app) => v.path === app.path && (v.args[0] ?? "") === (app.args[0] ?? "");
const toAction = (app) => ({ LaunchApp: { path: app.path, args: app.args, enabled: true } });

// The OS this window runs on, as a mappings file's per-OS `open` names it.
const THIS_OS = MAC ? "macos" : LINUX ? "linux" : "windows";
const OS_NAMES = [
  ["windows", "Windows"],
  ["macos", "macOS"],
  ["linux", "Linux"],
];

/** A new pick for a key restored from a per-OS file keeps the other OSes' apps. */
export const keepPerOs = (current, picked) =>
  current?.LaunchApp?.per_os && picked?.LaunchApp ? { LaunchApp: { ...picked.LaunchApp, per_os: current.LaunchApp.per_os } } : picked;

/** "Also set for macOS and Linux." when a per-OS file gave this key an app on other OSes, else null. */
export const otherOsNote = (perOs, os = THIS_OS) => {
  const names = OS_NAMES.filter(([k]) => k !== os && perOs?.[k]?.trim()).map(([, n]) => n);
  return names.length ? `Also set for ${names.join(" and ")}.` : null;
};
const tauriReady = () => Boolean(globalThis.window?.__TAURI__);

/**
 * The installed apps to search and pick (`list_apps`) and 'Choose file…'
 * (`choose_app`), shared by a key's 'Open an app' and a profile's apps.
 * `onPick` gets the picked app (as list_apps names it).
 */
export function AppPicker({ isCurrent = () => false, onPick, error = null, initialApps = null, loadApps = listApps, loadIcons = appIcons, choose = chooseApp }) {
  const [apps, setApps] = useState(initialApps);
  const [query, setQuery] = useState("");
  const [chooseError, setChooseError] = useState(null);
  // The apps' real icons (src-tauri app_icons.rs), by iconSource; letter avatars until they arrive.
  const [icons, setIcons] = useState({});
  useEffect(() => {
    if (initialApps || !tauriReady()) return;
    loadApps().then(setApps, (e) => console.error("freewheel: listing the apps failed:", String(e)));
  }, [initialApps, loadApps]);
  useEffect(() => {
    if (!apps?.length || !loadIcons || (loadIcons === appIcons && !tauriReady())) return undefined;
    let live = true;
    new Promise((resolve) => resolve(loadIcons(apps.map(iconSource)))).then(
      (found) => live && found && setIcons(found),
      (e) => console.error("freewheel: loading the app icons failed:", String(e)),
    );
    return () => {
      live = false;
    };
  }, [apps, loadIcons]);
  const shown = filterApps(apps ?? [], query);
  const chooseFile = () => {
    setChooseError(null);
    choose().then(
      (app) => app && onPick?.(app),
      (e) => setChooseError(`The file could not be chosen: ${String(e)}`),
    );
  };
  return (
    <>
      <div className="input-icon-box app-search">
        <Icon name="search" />
        <input aria-label="Search your apps" value={query} placeholder="Search your apps" onChange={(e) => setQuery(e.target.value)} />
      </div>
      <div className="app-list" role="listbox" aria-label="Your apps" aria-invalid={!!error}>
        {shown.map((app) => {
          const current = isCurrent(app);
          return (
            <button key={`${app.name}-${app.args[0] ?? app.path}`} role="option" aria-selected={current} className={`app-row${current ? " is-current" : ""}`} onClick={() => onPick?.(app)}>
              {icons[iconSource(app)] ? (
                <img className="app-icon" src={icons[iconSource(app)]} alt="" aria-hidden="true" draggable={false} />
              ) : (
                <span className="app-initial" data-hue={appHue(app.name)} aria-hidden="true">
                  {app.name.charAt(0).toUpperCase()}
                </span>
              )}
              <span className="grow">{app.name}</span>
              {current && <Icon name="check" className="icon app-check" />}
            </button>
          );
        })}
        {apps && shown.length === 0 && <div className="app-none">{query.trim() ? `No apps match “${query}”.` : "No apps found."}</div>}
        {!apps && <div className="app-none">Looking for your apps…</div>}
      </div>
      <button className="choose-file" onClick={chooseFile}>
        <Icon name="folder_open" />
        Choose file…
      </button>
      {error && <span role="alert">{error}</span>}
      {chooseError && <span role="alert">{chooseError}</span>}
    </>
  );
}

/**
 * The app picker (screens-and-components 3b): search the installed apps
 * (`list_apps`), pick one, or 'Choose file…' (`choose_app`); or a website
 * with Use. Each pick saves at once through `onUse`.
 */
export function ActionForm({ action, errors = {}, onUse, initialApps = null, loadApps = listApps, loadIcons = appIcons, choose = chooseApp }) {
  const v = action.LaunchApp;
  const web = v.path === OPENER && /^https?:\/\//.test(v.args[0] ?? "") ? v.args[0].replace(/^https?:\/\//, "") : "";
  const [site, setSite] = useState(web);
  const use = (picked) => onUse?.(keepPerOs(action, picked));
  const alsoSet = otherOsNote(v.per_os);
  const useSite = () => site.trim() && use(websiteAction(site));
  return (
    <>
      {v.enabled === false && v.path && (
        <div className="panel-note small" role="note">
          This key came from a restored file, so it is off and opens nothing. Pick the app again to turn it on.
        </div>
      )}
      {alsoSet && (
        <div className="panel-note small" role="note">
          {alsoSet} Picking an app here changes only this computer's.
        </div>
      )}
      <AppPicker
        isCurrent={(app) => sameLaunch(v, app)}
        onPick={(app) => use(toAction(app))}
        error={errors.path}
        initialApps={initialApps}
        loadApps={loadApps}
        loadIcons={loadIcons}
        choose={choose}
      />
      <div className="field-label">Or a website</div>
      <div className="input-row">
        <input
          aria-label="Website"
          value={site}
          placeholder="like youtube.com"
          onChange={(e) => setSite(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && useSite()}
        />
        <button className="btn-primary btn-md" onClick={useSite}>
          Use
        </button>
      </div>
    </>
  );
}

/**
 * The key panel's choices: [title, subtitle] per kind, from the prototype's CH table.
 * SetJogMode has no CH row (the Wheel tab sets the wheel buttons).
 */
export const KIND_TEXT = {
  Hotkey: ["Press a shortcut", "Like Copy, Save or Mute"],
  Held: ["Hold a key while I hold this one", "Good for Shift or Alt"],
  Latched: ["Turn a key on and off", "Press once to hold it, again to let go"],
  LaunchApp: ["Open an app or website", "Spotify, a file, a web page…"],
  OverrideToggle: ["Turn the wheel into…", "A volume or brightness knob"],
  LayerPush: ["Switch to another set of keys", "Pages like Music or Browser"],
  Clear: ["Do nothing", "Clear this key"],
  SetJogMode: [KINDS.SetJogMode, "Switches the wheel to Jog, Shuttle or Scroll."],
};
/** The choices in the prototype's CH order. */
export const KIND_ROWS = [...COMBO_KINDS, "LaunchApp", "OverrideToggle", "LayerPush", "Clear"];

/** Each choice's icon (bundled SVG, never fetched), as the CH table names it. */
export const KIND_ICON = Object.fromEntries(
  Object.entries({
    Hotkey: "keyboard",
    Held: "pan_tool",
    Latched: "toggle_on",
    LaunchApp: "apps",
    OverrideToggle: "volume_up",
    LayerPush: "tab",
    Clear: "block",
    SetJogMode: "wheel",
  }).map(([kind, name]) => [kind, [name, iconSvg(name)]]),
);

// The prototype's "Or pick a common one" shortcuts; Cmd stands in for Ctrl on a Mac.
const MOD = MAC ? "Meta" : "Ctrl";
export const PRESETS = [
  ["Copy", { modifiers: [MOD], key: "C" }],
  ["Paste", { modifiers: [MOD], key: "V" }],
  ["Undo", { modifiers: [MOD], key: "Z" }],
  ["Redo", MOD === "Meta" ? { modifiers: [MOD, "Shift"], key: "Z" } : { modifiers: [MOD], key: "Y" }],
  ["Save", { modifiers: [MOD], key: "S" }],
  ["Play/Pause", { modifiers: [], key: "PlayPause" }],
  ["Mute", { modifiers: [], key: "Mute" }],
];
/**
 * The prototype's Held and Latched "Quick picks": Shift, Ctrl (Meta on a Mac) and Alt alone,
 * labelled as its kn() does: ⇧ ⌘ ⌥ on a Mac, Shift Ctrl Alt elsewhere.
 */
const QUICK_LABELS = MOD === "Meta" ? ["⇧", "⌘", "⌥"] : ["Shift", "Ctrl", "Alt"];
export const QUICK_PICKS = ["Shift", MOD, "Alt"].map((m, i) => [QUICK_LABELS[i], { modifiers: [m], key: "" }]);

/** The detail view's [title, subtitle] for `kind` (the prototype's MT/MS); `key` is the printed name. */
export function modeText(kind, key) {
  const MS = {
    Hotkey: `When you press ${key}, freewheel presses this shortcut for you.`,
    Held: `The key stays held down for as long as you hold ${key}.`,
    Latched: `Press ${key} once to hold the key down. Press it again to let go.`,
    LaunchApp: "",
    OverrideToggle: `Press ${key} and the wheel becomes a knob for:`,
    LayerPush: "Pages are extra sets of keys, like “Music” or “Browser”. This key takes you to one.",
    SetJogMode: KIND_TEXT.SetJogMode[1],
  };
  return [KIND_TEXT[kind][0], MS[kind]];
}

const comboOf = (action) => (action && Object.values(action)[0]?.combo) || null;

/**
 * Saves `kind` + `combo` on `key` of page `pageId` through `save` (save_config).
 * Resolves to { config } or { error } (the inline error); an invalid combo is never sent.
 */
export async function saveKey({ config, pageId, key, kind, combo, save = saveConfig }) {
  const error = comboError(combo, kind);
  if (error) return { error };
  const next = setBinding(config, pageId, key, { [kind]: { combo } });
  try {
    await save(next);
    return { config: next };
  } catch (e) {
    console.error("freewheel: saving key", key, "failed:", saveErrorText(e));
    return { error: saveErrorText(e) };
  }
}

/** The combo as keycaps with "+" between, like the prototype's record box. */
export function Keycaps({ combo }) {
  const caps = comboKeys(combo);
  return (
    <span className="keycaps">
      {caps.map((k, i) => (
        <span key={k} className="keycaps-item">
          {i > 0 && <span className="keycaps-plus">+</span>}
          <kbd className="keycap-lg">{k}</kbd>
        </span>
      ))}
    </span>
  );
}

/**
 * The record box (shared): idle (dashed, Record) · listening (accent, pulsing,
 * live modifiers as keycaps, Cancel) · done (keycaps, "Click to record again").
 * A shortcut is modifiers + one key; `single` (Hold / Turn on and off) takes
 * any one key, a lone modifier too. `onRecord` gets the finished combo.
 */
export function RecordBox({ single = false, combo, error, onRecord, autoListen = false, compact = false, label = "Shortcut" }) {
  const [listening, setListening] = useState(false);
  const [live, setLive] = useState(null);
  const input = useRef(null);
  useEffect(() => {
    if (autoListen) input.current?.focus();
  }, [autoListen]);
  const record = (e) => {
    if (e.key === "Tab") return;
    e.preventDefault();
    const next = comboFromEvent(e);
    if (!next.key && !single) return setLive(next);
    // Holding a lone modifier: the modifier itself is the key held.
    const done = single && !next.key ? { modifiers: next.modifiers.slice(-1), key: "" } : next;
    setLive(null);
    onRecord(done);
    input.current?.blur();
  };
  const shown = listening ? live : combo;
  const recorded = shown && (shown.key || shown.modifiers.length > 0);
  return (
    <>
      <label className={`record-box${listening ? " is-listening" : recorded ? " is-done" : ""}${compact ? " is-compact" : ""}`}>
        <input
          ref={input}
          className="combo-capture"
          readOnly
          aria-label={label}
          placeholder="Click here, then press the shortcut"
          value={combo ? formatCombo(combo) : ""}
          onKeyDown={record}
          onFocus={() => (setListening(true), setLive(null))}
          onBlur={() => setListening(false)}
          aria-invalid={Boolean(error)}
        />
        <span className="record-face" aria-hidden="true">
          {listening && (
            <span className="record-listen">
              <span className="rec-dot" />
              {compact ? "Listening…" : single ? "Listening… press a key" : "Listening… press your shortcut"}
            </span>
          )}
          {recorded && <Keycaps combo={shown} />}
          {!listening && recorded && <span className="record-hint">Click to record again</span>}
          {!listening && !recorded && (
            <>
              <span className="record-idle">
                <span className="rec-dot" />
                Record
              </span>
              {!compact && (
                <span className="record-hint">{single ? "Then press one key, like Shift" : "Then press the keys together on your keyboard"}</span>
              )}
            </>
          )}
        </span>
      </label>
      {listening && (
        <button className="btn-ghost record-cancel" onMouseDown={(e) => e.preventDefault()} onClick={() => input.current?.blur()}>
          Cancel
        </button>
      )}
    </>
  );
}

const sameCombo = (a, b) => Boolean(a && b) && formatCombo(a) === formatCombo(b);
const WHEEL_BUTTONS = ["Shtl", "Jog", "Scrl"];
const WHEEL_KEY_OPTS = [
  ["SystemVolume", "volume_up", "System volume"],
  ["AppVolume", "graphic_eq", "Volume of the app I’m using"],
  ["Brightness", "light_mode", "Screen brightness"],
];
const PAGE_SUGGESTIONS = ["Music", "Browser", "Editing", "Video calls"];

/** A row of the key panel (choice, wheel knob, page): icon, title, sub and the current check. */
function OptionRow({ icon, title, current, onClick, children, className = "option-row" }) {
  return (
    <button className={`${className}${current ? " is-current" : ""}`} aria-pressed={current} onClick={onClick}>
      <Icon name={icon} className="icon option-icon" />
      <span className="option-title">{title}</span>
      {children}
      {current && <Icon className="icon choice-check" name="check_circle" />}
    </button>
  );
}

/**
 * The key panel: the choices list (KIND_ROWS); picking a kind opens its detail
 * view with 'All choices', the title and subtitle, then the record box, the app
 * picker, the knob or the pages. Every pick saves at once, as in the prototype.
 * `onSave(kind, combo)` saves a shortcut kind and resolves to { error };
 * `onSaveAction(action)` saves any other kind and resolves to { errors };
 * `onClear()` clears the key. A set key opens on the choices list.
 */
export function KeyPanel({
  keyName,
  action,
  config,
  layerId,
  onSave,
  onSaveAction,
  onClear,
  onClose,
  onWheel,
  onCreatePage,
  onShowPage,
  back = null,
  initialMode = null,
  initialError = null,
  initialListen = false,
}) {
  const boundKind = KIND_ROWS.find((k) => action && k in action) ?? (action ? null : "Clear");
  const [mode, setMode] = useState(initialMode);
  const [listen, setListen] = useState(initialListen);
  const [combo, setCombo] = useState(comboOf(action));
  const [error, setError] = useState(initialError);
  const [errors, setErrors] = useState({});
  const [justSaved, setJustSaved] = useState(false);
  const [newPage, setNewPage] = useState("");
  const printed = LABELS[keyName] ?? keyName;
  const pick = (k) => {
    if (k === "Clear") {
      setJustSaved(false);
      return onClear?.();
    }
    setMode(k);
    setError(null);
    setErrors({});
    // The record box listens at once for a new kind of shortcut.
    setListen(COMBO_KINDS.includes(k) && boundKind !== k);
    setCombo(boundKind === k ? comboOf(action) : null);
    return null;
  };
  const done = (r) => (r?.config && setJustSaved(true), r);
  const saveCombo = (c) => {
    setCombo(c);
    setError(null);
    setJustSaved(false);
    setListen(false);
    return onSave?.(mode, c).then(done).then((r) => setError(r?.error ?? null));
  };
  const saveOther = (a) => onSaveAction?.(a).then(done).then((r) => setErrors(r?.errors ?? {}));
  const isWheelButton = WHEEL_BUTTONS.includes(keyName);
  const [modeTitle, modeSub] = mode ? modeText(mode, printed) : [];
  const now = isWheelButton ? "A wheel button" : back ? `Right now: Back to ${back.name}` : action ? `Right now: ${longLabel(action, config ?? { layers: [] })}` : "Not set yet";
  const pageName = config?.layers.find((l) => l.id === layerId)?.name ?? "this page";
  let body;
  if (isWheelButton)
    body = (
      <>
        <div className="panel-note">{`${printed} is one of the three buttons above the wheel. Pressing it changes what the wheel does.`}</div>
        <button className="btn-primary btn-md" onClick={onWheel}>
          Set up the wheel
        </button>
      </>
    );
  else if (back)
    body = (
      <>
        <div className="back-card">
          <Icon name="undo" className="icon back-icon" />
          <div className="back-title">{`This key takes you back to “${back.name}”.`}</div>
          <div className="back-text">It stays this way so you can never get stuck on this page.</div>
        </div>
        <button className="btn-md" onClick={() => onShowPage?.(back.id)}>{`Show the ${back.name} page`}</button>
      </>
    );
  else if (mode === null)
    body = (
      <div className="choices" role="radiogroup" aria-label="Choices">
        {KIND_ROWS.map((k) => (
          <label key={k} className={`choice${boundKind === k ? " is-current" : ""}`}>
            <input type="radio" name="kind" className="choice-input" checked={boundKind === k} onChange={() => pick(k)} onClick={() => boundKind === k && pick(k)} />
            <Icon className="choice-icon" name={KIND_ICON[k][0]} />
            <span className="choice-text">
              <strong>{KIND_TEXT[k][0]}</strong>
              <span className="choice-sub">{KIND_TEXT[k][1]}</span>
            </span>
            {boundKind === k && <Icon className="choice-check" name="check_circle" />}
          </label>
        ))}
      </div>
    );
  else {
    // Page keys go to pages of the same profile.
    const others = config ? pagesOf(config, profileOf(config, layerId)).filter((l) => l.id !== layerId) : [];
    const names = (config?.layers ?? []).map((l) => l.name.toLowerCase());
    const create = (name) => onCreatePage?.(name.trim() || "New page").then(done);
    body = (
      <div className="choice-detail">
        <button className="back-link" onClick={() => (setMode(null), setListen(false))}>
          <Icon name="chevron_left" />
          All choices
        </button>
        <h3 className="mode-title">{modeTitle}</h3>
        {modeSub && <p className="mode-sub">{modeSub}</p>}
        {COMBO_KINDS.includes(mode) && (
          <>
            <RecordBox key={mode} single={mode !== "Hotkey"} combo={combo} error={error} onRecord={saveCombo} autoListen={listen} />
            <div className="field-label">{mode === "Hotkey" ? "Or pick a common one" : "Quick picks"}</div>
            <div className="presets">
              {/* The listening record box keeps focus until the click lands: blurring it on mousedown hid
                  its Cancel button, the presets moved up and the click missed them. */}
              {(mode === "Hotkey" ? PRESETS : QUICK_PICKS).map(([name, c]) => (
                <button
                  key={name}
                  className={`preset${sameCombo(c, combo) ? " is-current" : ""}`}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => (globalThis.document?.activeElement?.blur?.(), saveCombo(c))}
                >
                  <strong>{name}</strong>
                  {mode === "Hotkey" && <span className="preset-combo">{comboKeys(c).join(MAC ? "" : "+")}</span>}
                </button>
              ))}
            </div>
            {mode === "Latched" && (
              <div className="lit-card">
                <span className="mini-key" aria-hidden="true">
                  <span className="mini-key-led" />
                  <span className="mini-key-name">{printed}</span>
                </span>
                <p>While it’s on, the light on this key stays lit — so you can always see it’s held down.</p>
              </div>
            )}
          </>
        )}
        {mode === "LaunchApp" && (
          <ActionForm
            key={keyName}
            action={action?.LaunchApp ? action : blank("LaunchApp", config, layerId)}
            errors={errors}
            onUse={saveOther}
          />
        )}
        {mode === "OverrideToggle" && (
          <>
            {WHEEL_KEY_OPTS.map(([m, icon, title]) => (
              <OptionRow key={m} icon={icon} title={title} current={action?.OverrideToggle?.mode === m} onClick={() => saveOther({ OverrideToggle: { mode: m } })} />
            ))}
            <div className="panel-note small">{`Press ${printed} again to put the wheel back to normal.`}</div>
          </>
        )}
        {mode === "LayerPush" && (
          <>
            {others.length > 0 && <div className="field-label first">Go to a page</div>}
            {others.map((l) => (
              <OptionRow
                key={l.id}
                className="option-row page-row"
                icon="tab"
                title={l.name}
                current={action?.LayerPush?.target === l.id}
                onClick={() => saveOther({ LayerPush: { target: l.id } })}
              />
            ))}
            <div className="field-label">Make a new page</div>
            <div className="input-row">
              <input
                aria-label="New page name"
                value={newPage}
                placeholder="Name it, like Music"
                onChange={(e) => setNewPage(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && create(newPage).then(() => setNewPage(""))}
              />
              <button className="btn-primary btn-md" onClick={() => create(newPage).then(() => setNewPage(""))}>
                Create
              </button>
            </div>
            <div className="presets">
              {PAGE_SUGGESTIONS.filter((n) => !names.includes(n.toLowerCase())).map((n) => (
                <button key={n} className="suggestion" onClick={() => create(n)}>
                  {n}
                </button>
              ))}
            </div>
            <div className="panel-note small with-icon">
              <Icon name="undo" />
              <span>{`On that page, ${printed} becomes “Back”, so you can always return to ${pageName}.`}</span>
            </div>
          </>
        )}
        {error && <p role="alert">{error}</p>}
        {errors.save && <p role="alert">{errors.save}</p>}
      </div>
    );
  }
  const showTry = action && !isWheelButton && !back && !listen;
  return (
    <section id="action-editor" className="key-panel key-editor" aria-label={`${printed} action`}>
      <header className="panel-head">
        <kbd className="keycap">{printed}</kbd>
        <div className="grow">
          <h2 className="panel-title">{`When I press ${printed}…`}</h2>
          <div className="panel-now">{now}</div>
        </div>
        {onClose && (
          <button className="icon-btn" title="Close" aria-label="Close" onClick={onClose}>
            <Icon name="close" />
          </button>
        )}
      </header>
      <div className="panel-body">{body}</div>
      {showTry && (
        <div className={`panel-try${justSaved ? " is-saved" : ""}`}>
          <Icon name={justSaved ? "check_circle" : "touch_app"} />
          {justSaved ? `Saved. Press ${printed} on your Speed Editor to try it.` : `Try it: press ${printed} on your Speed Editor.`}
        </div>
      )}
    </section>
  );
}

/** "Pick a key", with the prototype's wheel tip while nothing is set up yet. */
export function PickAKey({ config }) {
  const empty = config?.layers.length === 1 && Object.keys(config.layers[0].bindings).length === 0;
  return (
    <div className="panel-empty">
      <span className="empty-icon">
        <Icon name="touch_app" />
      </span>
      <div className="empty-title">Pick a key</div>
      <p>Click any key in the drawing — or simply press it on your Speed Editor. Then choose what it does.</p>
      {empty && (
        <div className="tip">
          <Icon name="trip_origin" />
          <span>Your wheel already scrolls, like a mouse wheel. Give it a spin!</span>
        </div>
      )}
    </div>
  );
}

/** Saves `next` through `save`; resolves to { config } or { errors: { save } }. */
async function commit(next, save, what) {
  try {
    await save(next);
    return { errors: {}, config: next };
  } catch (e) {
    console.error("freewheel: saving", what, "failed:", saveErrorText(e));
    return { errors: { save: saveErrorText(e) } };
  }
}

/**
 * Edits what `keyName` does on `page` (else the base page) and saves it with
 * save_config. Invalid input and a rejected save show inline, never as a toast.
 */
export default function ActionEditor({
  keyName,
  page = null,
  initialConfig = null,
  load = getConfig,
  save = saveConfig,
  onSaved,
  onClose,
  onWheel,
  onShowPage,
  back = null,
  recording = false,
}) {
  const [config, setConfig] = useState(initialConfig);
  const layerId = page ?? config?.base_layer;
  const bound = config?.layers.find((l) => l.id === layerId)?.bindings[keyName];
  useEffect(() => {
    // The Keys tab owns the config: follow its renames, pages and key saves.
    if (initialConfig) setConfig(initialConfig);
    else Promise.resolve().then(load).then(setConfig, (e) => console.error("freewheel: loading the config failed:", String(e)));
  }, [initialConfig, load]);
  if (!keyName) return <PickAKey config={config} />;
  if (!config) return null;
  const saved = (r) => {
    if (!r.config) return r;
    setConfig(r.config);
    onSaved?.(r.config);
    return r;
  };
  const createPage = (name) => {
    const added = addPage(config, profileOf(config, layerId));
    const named = { ...added.config, layers: added.config.layers.map((l) => (l.id === added.id ? { ...l, name } : l)) };
    return commit(withBinding(named, layerId, keyName, { LayerPush: { target: added.id } }), save, "the new page").then(saved);
  };
  return (
    <KeyPanel
      key={`${layerId}-${keyName}`}
      keyName={keyName}
      action={bound}
      config={config}
      layerId={layerId}
      back={back}
      onClose={onClose}
      onWheel={onWheel}
      onShowPage={onShowPage}
      onSave={(kind, combo) => saveKey({ config, pageId: layerId, key: keyName, kind, combo, save }).then(saved)}
      onSaveAction={(action) => saveAction({ config, layerId, key: keyName, action, save }).then(saved)}
      onClear={() => commit(withoutBinding(config, layerId, keyName), save, `key ${keyName}`).then(saved)}
      onCreatePage={createPage}
      initialMode={recording ? "Hotkey" : null}
      initialListen={recording}
    />
  );
}
