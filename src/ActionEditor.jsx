import { useEffect, useState } from "react";
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
// ISimpleAudioVolume on Windows, the process tap on macOS (an earlier change).
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
 * Edits what `keyName` does on the base page and saves it with save_config.
 * Invalid input and a rejected save show inline, never as a toast.
 */
export default function ActionEditor({ keyName, page = null, initialConfig = null, load = getConfig, save = saveConfig, onSaved }) {
  const [config, setConfig] = useState(initialConfig);
  const layerId = page ?? config?.base_layer;
  const bound = config?.layers.find((l) => l.id === layerId)?.bindings[keyName];
  const [action, setAction] = useState(null);
  const [errors, setErrors] = useState({});
  useEffect(() => {
    // The Keys tab owns the config: follow its renames, pages and key saves.
    if (initialConfig) setConfig(initialConfig);
    else Promise.resolve().then(load).then(setConfig, (e) => console.error("freewheel: loading the config failed:", String(e)));
  }, [initialConfig, load]);
  useEffect(() => {
    setAction(null);
    setErrors({});
  }, [keyName]);
  if (!keyName) return <p>Pick a key</p>;
  if (!config) return null;
  const current = action ?? (kindOf(bound) in KINDS ? bound : null);
  const onSave = () =>
    saveAction({ config, layerId, key: keyName, action: current, save }).then((r) => {
      setErrors(r.errors);
      if (!r.config) return;
      setConfig(r.config);
      onSaved?.(r.config);
    });
  return (
    <section id="action-editor">
      <h2>{`When I press ${keyName}…`}</h2>
      <Choice
        label="This key will"
        value={kindOf(current) ?? ""}
        options={{ "": "Choose…", ...KINDS }}
        onChange={(kind) => {
          setErrors({});
          setAction(kind ? blank(kind, config, layerId) : null);
        }}
      />
      {current && <ActionForm action={current} config={config} layerId={layerId} errors={errors} onChange={setAction} />}
      {errors.save && <p role="alert">{errors.save}</p>}
      <button disabled={!current} onClick={onSave}>
        Save
      </button>
    </section>
  );
}
