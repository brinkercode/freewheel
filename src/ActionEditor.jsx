import { useEffect, useState } from "react";
import Icon, { iconSvg } from "./Icon.jsx";
import { COMBO_KINDS, LABELS, comboError, comboFromEvent, describeAction, formatCombo, setBinding } from "./keys.js";
import { getConfig, saveConfig, saveErrorText } from "./settingsApi.js";

// The action editors for app launch, page push (a Layer in bindings.ts),
// wheel mode and wheel override. Layers are "pages" in every visible string.

export const KINDS = {
  LaunchApp: "Open an app",
  LayerPush: "Go to a page",
  SetJogMode: "Set the wheel mode",
  OverrideToggle: "Turn the wheel into a dial",
};

export const JOG_MODES = { Jog: "Jog", Shuttle: "Shuttle", Scroll: "Scroll" };

// Windows and macOS (the only supported OSes) both ship all three controls:
// ISimpleAudioVolume on Windows, the process tap on macOS (freewheel-m4-9).
export const OVERRIDE_MODES = { SystemVolume: "System volume", AppVolume: "App volume", Brightness: "Brightness" };

const kindOf = (action) => (action ? Object.keys(action)[0] : null);

/** A new action of `kind`, preset to its first valid choice. */
export function blank(kind, config, layerId) {
  switch (kind) {
    case "LaunchApp":
      return { LaunchApp: { path: "", args: [], enabled: true } };
    case "LayerPush":
      return { LayerPush: { target: config.layers.find((l) => l.id !== layerId)?.id ?? "" } };
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
  if (kind === "LayerPush" && (v.target === layerId || !config.layers.some((l) => l.id === v.target)))
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

function Choice({ label, value, options, error, onChange }) {
  return (
    <label>
      {label}
      <select value={value} aria-invalid={!!error} onChange={(e) => onChange(e.target.value)}>
        {Object.entries(options).map(([id, text]) => (
          <option key={id} value={id}>
            {text}
          </option>
        ))}
      </select>
      {error && <span role="alert">{error}</span>}
    </label>
  );
}

/** The fields of one action; `onChange` gets the whole new action. */
export function ActionForm({ action, config, layerId, errors = {}, onChange }) {
  const kind = kindOf(action);
  const v = action[kind];
  const set = (patch) => onChange({ [kind]: { ...v, ...patch } });
  if (kind === "LaunchApp")
    return (
      <fieldset>
        <label>
          App
          <input value={v.path} aria-invalid={!!errors.path} onChange={(e) => set({ path: e.target.value })} />
          {errors.path && <span role="alert">{errors.path}</span>}
        </label>
        <label>
          Arguments, one per line
          <textarea value={v.args.join("\n")} onChange={(e) => set({ args: e.target.value.split("\n") })} />
        </label>
        <label>
          <input type="checkbox" checked={v.enabled} onChange={(e) => set({ enabled: e.target.checked })} />
          Allow this key to open the app
        </label>
      </fieldset>
    );
  if (kind === "LayerPush") {
    const pages = Object.fromEntries(config.layers.filter((l) => l.id !== layerId).map((l) => [l.id, l.name]));
    return (
      <fieldset>
        <Choice label="Page" value={v.target} options={pages} error={errors.target} onChange={(target) => set({ target })} />
        <p>Press the same key again to go back.</p>
      </fieldset>
    );
  }
  if (kind === "SetJogMode")
    return <Choice label="Wheel mode" value={v.mode} options={JOG_MODES} error={errors.mode} onChange={(mode) => set({ mode })} />;
  return <Choice label="The wheel controls" value={v.mode} options={OVERRIDE_MODES} error={errors.mode} onChange={(mode) => set({ mode })} />;
}

/**
 * The key panel's choices: [title, subtitle] per kind, from the prototype's CH
 * table. SetJogMode has no CH row (the prototype sets it on the wheel buttons).
 */
export const KIND_TEXT = {
  Hotkey: ["Press a shortcut", "Like Copy, Save or Mute"],
  Held: ["Hold a key while I hold this one", "Good for Shift or Alt"],
  Latched: ["Turn a key on and off", "Press once to hold it, again to let go"],
  LaunchApp: ["Open an app or website", "Spotify, a file, a web page…"],
  OverrideToggle: ["Turn the wheel into…", "A volume or brightness knob"],
  LayerPush: ["Switch to another set of keys", "Pages like Music or Browser"],
  SetJogMode: [KINDS.SetJogMode, "Switches the wheel to Jog, Shuttle or Scroll."],
};
/** The choices in the prototype's CH order. */
export const KIND_ROWS = [...COMBO_KINDS, "LaunchApp", "OverrideToggle", "LayerPush", "SetJogMode"];

/** Each choice's icon (bundled SVG, never fetched), as the CH table names it. */
export const KIND_ICON = Object.fromEntries(
  Object.entries({
    Hotkey: "keyboard",
    Held: "pan_tool",
    Latched: "toggle_on",
    LaunchApp: "apps",
    OverrideToggle: "volume_up",
    LayerPush: "tab",
    SetJogMode: "wheel",
  }).map(([kind, name]) => [kind, [name, iconSvg(name)]]),
);

// The prototype's "Or pick a common one" shortcuts; Cmd stands in for Ctrl on a Mac.
const MOD = /Mac/.test(globalThis.navigator?.userAgent ?? "") ? "Meta" : "Ctrl";
export const PRESETS = [
  ["Copy", { modifiers: [MOD], key: "C" }],
  ["Paste", { modifiers: [MOD], key: "V" }],
  ["Undo", { modifiers: [MOD], key: "Z" }],
  ["Redo", MOD === "Meta" ? { modifiers: [MOD, "Shift"], key: "Z" } : { modifiers: [MOD], key: "Y" }],
  ["Save", { modifiers: [MOD], key: "S" }],
];

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
  const error = comboError(combo);
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
function Keycaps({ combo }) {
  const caps = [...combo.modifiers, combo.key].filter(Boolean);
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
 * The record box: click it (its input takes the focus), then press the
 * shortcut. Shows Record, Listening… or the recorded keycaps.
 */
function RecordBox({ kind, combo, error, onRecord }) {
  const [listening, setListening] = useState(false);
  const shortcut = kind === "Hotkey";
  const record = (e) => {
    if (e.key === "Tab") return;
    e.preventDefault();
    const next = comboFromEvent(e);
    onRecord(next);
    if (next.key) setListening(false);
  };
  const recorded = combo && (combo.key || combo.modifiers.length > 0);
  return (
    <>
      <label className={`record-box${listening ? " is-listening" : ""}`}>
        <input
          className="combo-capture"
          readOnly
          aria-label="Shortcut"
          placeholder="Click here, then press the shortcut"
          value={combo ? formatCombo(combo) : ""}
          onKeyDown={record}
          onFocus={() => setListening(true)}
          onBlur={() => setListening(false)}
          aria-invalid={Boolean(error)}
        />
        <span className="record-face" aria-hidden="true">
          {listening ? (
            <span className="record-listen">
              <span className="rec-dot" />
              {shortcut ? "Listening… press your shortcut" : "Listening… press a key"}
            </span>
          ) : recorded ? (
            <>
              <Keycaps combo={combo} />
              <span className="record-hint">Click to record again</span>
            </>
          ) : (
            <>
              <span className="record-idle">
                <span className="rec-dot" />
                Record
              </span>
              <span className="record-hint">{shortcut ? "Then press the keys together on your keyboard" : "Then press one key, like Shift"}</span>
            </>
          )}
        </span>
      </label>
      {listening && (
        <button className="btn-ghost record-cancel" onMouseDown={(e) => e.preventDefault()} onClick={() => document.activeElement?.blur()}>
          Cancel
        </button>
      )}
    </>
  );
}

const sameCombo = (a, b) => Boolean(a && b) && formatCombo(a) === formatCombo(b);

/**
 * The key panel: one choices list (KIND_ROWS); picking a kind opens its detail
 * view with '‹ All choices', the title and subtitle, the record box or the
 * ActionForm fields, and one Save. `onSave(kind, combo)` saves a shortcut kind
 * and resolves to { error }; `onSaveAction(action)` saves any other kind and
 * resolves to { errors }. A set key opens on its own choice.
 */
export function KeyPanel({ keyName, action, config, layerId, onSave, onSaveAction, onClose, initialMode, initialError = null }) {
  const boundKind = KIND_ROWS.find((k) => action && k in action) ?? null;
  const [mode, setMode] = useState(initialMode === undefined ? boundKind : initialMode);
  const [combo, setCombo] = useState(comboOf(action));
  const [draft, setDraft] = useState(boundKind && !COMBO_KINDS.includes(boundKind) ? action : null);
  const [error, setError] = useState(initialError);
  const [errors, setErrors] = useState({});
  const pick = (k) => {
    setMode(k);
    setError(null);
    setErrors({});
    if (!COMBO_KINDS.includes(k)) setDraft(boundKind === k ? action : blank(k, config, layerId));
  };
  const [justSaved, setJustSaved] = useState(false);
  const printed = LABELS[keyName] ?? keyName;
  const [modeTitle, modeSub] = mode ? modeText(mode, printed) : [];
  const done = (r) => (r?.config && setJustSaved(true), r);
  const save = () =>
    COMBO_KINDS.includes(mode)
      ? onSave?.(mode, combo).then(done).then((r) => setError(r?.error ?? null))
      : onSaveAction?.(draft).then(done).then((r) => setErrors(r?.errors ?? {}));
  const record = (c) => (setCombo(c), setError(null), setJustSaved(false));
  return (
    <section id="action-editor" className="key-panel key-editor" aria-label={`${printed} action`}>
      <header className="panel-head">
        <kbd className="keycap">{printed}</kbd>
        <div className="grow">
          <h2 className="panel-title">{`When I press ${keyName}…`}</h2>
          <div className="panel-now">{action ? `Right now: ${describeAction(action, config ?? { layers: [] })}` : "Not set yet"}</div>
        </div>
        {onClose && (
          <button className="icon-btn" title="Close" aria-label="Close" onClick={onClose}>
            <Icon name="close" />
          </button>
        )}
      </header>
      <div className="panel-body">
      {mode === null ? (
        <div className="choices" role="radiogroup" aria-label="Choices">
          {KIND_ROWS.map((k) => (
            <label key={k} className={`choice${boundKind === k ? " is-current" : ""}`}>
              <input type="radio" name="kind" className="choice-input" checked={boundKind === k} onChange={() => pick(k)} />
              <Icon className="choice-icon" name={KIND_ICON[k][0]} />
              <span className="choice-text">
                <strong>{KIND_TEXT[k][0]}</strong>
                <span className="choice-sub">{KIND_TEXT[k][1]}</span>
              </span>
              {boundKind === k && <Icon className="choice-check" name="check_circle" />}
            </label>
          ))}
        </div>
      ) : (
        <div className="choice-detail">
          <button className="back-link" onClick={() => setMode(null)}>
            ‹ All choices
          </button>
          <h3 className="mode-title">{modeTitle}</h3>
          {modeSub && <p className="mode-sub">{modeSub}</p>}
          {COMBO_KINDS.includes(mode) ? (
            <RecordBox key={mode} kind={mode} combo={combo} error={error} onRecord={record} />
          ) : (
            draft && <ActionForm action={draft} config={config} layerId={layerId} errors={errors} onChange={setDraft} />
          )}
          {mode === "Hotkey" && (
            <>
              <div className="field-label">Or pick a common one</div>
              <div className="presets">
                {PRESETS.map(([name, c]) => (
                  <button key={name} className={`preset${sameCombo(c, combo) ? " is-current" : ""}`} onClick={() => record(c)}>
                    <strong>{name}</strong>
                    <span className="preset-combo">{formatCombo(c)}</span>
                  </button>
                ))}
              </div>
            </>
          )}
          {mode === "Latched" && (
            <div className="lit-card">
              <span className="mini-key" aria-hidden="true">
                <span className="mini-key-led" />
                <span className="mini-key-name">{printed}</span>
              </span>
              <p>While it’s on, the light on this key stays lit — so you can always see it’s held down.</p>
            </div>
          )}
          {error && <p role="alert">{error}</p>}
          {errors.save && <p role="alert">{errors.save}</p>}
          <button className="btn-primary" onClick={save}>
            Save
          </button>
        </div>
      )}
      </div>
      {action && (
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

/**
 * Edits what `keyName` does on `page` (else the base page) and saves it with
 * save_config. Invalid input and a rejected save show inline, never as a toast.
 */
export default function ActionEditor({ keyName, page = null, initialConfig = null, load = getConfig, save = saveConfig, onSaved, onClose }) {
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
  return (
    <KeyPanel
      key={`${layerId}-${keyName}`}
      keyName={keyName}
      action={bound}
      config={config}
      layerId={layerId}
      onClose={onClose}
      onSave={(kind, combo) => saveKey({ config, pageId: layerId, key: keyName, kind, combo, save }).then(saved)}
      onSaveAction={(action) => saveAction({ config, layerId, key: keyName, action, save }).then(saved)}
    />
  );
}
