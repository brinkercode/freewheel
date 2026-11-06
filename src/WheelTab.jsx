import { useEffect, useState } from "react";
import { ActionForm, JOG_MODES, blank, validate } from "./ActionEditor.jsx";
import { comboError, comboFromEvent, formatCombo } from "./keys.js";
import { getConfig, saveConfig, saveErrorText } from "./settingsApi.js";

// What a wheel turn can do; LaunchApp reuses the key panel's form.
export const SIDE_KINDS = { "": "Nothing", Scroll: "Scroll", Hotkey: "Send a shortcut", LaunchApp: "Open an app" };

// The dial steps: volume is a 0..1 fraction, brightness a percentage.
export const STEPS = [
  ["system_volume_step", "System volume", 100],
  ["app_volume_step", "App volume", 100],
  ["brightness_step", "Brightness", 1],
];

const NO_BINDING = { left: null, right: null, threshold: 1, shuttle_max_rate_hz: null };
export const bindingOf = (config, mode) => config.jog.per_mode[mode] ?? NO_BINDING;
const kindOf = (action) => (action ? Object.keys(action)[0] : "");
const number = (e) => (e.target.value === "" ? NaN : Number(e.target.value));

function sideError(action, config) {
  const kind = kindOf(action);
  if (kind === "Scroll" && (!Number.isInteger(action.Scroll.amount) || action.Scroll.amount === 0))
    return "Enter how far to scroll, like 1 or -1.";
  if (kind === "Hotkey") return comboError(action.Hotkey.combo);
  if (kind === "LaunchApp") return validate(action, config, null).path ?? null;
  return null;
}

/** Inline errors by field (`Jog.left`, `Shuttle.threshold`, `brightness_step`…); empty when it can be saved. */
export function validateWheel(config) {
  const errors = {};
  for (const mode of Object.keys(JOG_MODES)) {
    const b = bindingOf(config, mode);
    for (const side of ["left", "right"]) {
      const error = sideError(b[side], config);
      if (error) errors[`${mode}.${side}`] = error;
    }
    if (!Number.isInteger(b.threshold) || b.threshold < 1) errors[`${mode}.threshold`] = "Enter a whole number of 1 or more.";
  }
  for (const [field, , scale] of STEPS) {
    const percent = config.overrides[field] * scale;
    if (!(percent >= 1 && percent <= 100)) errors[field] = "Enter a step from 1 to 100.";
  }
  return errors;
}

/**
 * Saves `config` through `save` (save_config) unless `errors` has any.
 * Resolves to { errors } and, once saved, { config }.
 */
export async function saveWith(config, errors, save = saveConfig, what = "settings") {
  if (Object.keys(errors).length) return { errors };
  try {
    await save(config);
    return { errors: {}, config };
  } catch (e) {
    console.error("freewheel: saving the", what, "failed:", JSON.stringify(e));
    return { errors: { save: saveErrorText(e) } };
  }
}

/** The config with the wheel binding of `mode` patched. */
export const withJog = (config, mode, patch) => ({
  ...config,
  jog: { ...config.jog, per_mode: { ...config.jog.per_mode, [mode]: { ...bindingOf(config, mode), ...patch } } },
});

// The Arguments box keeps empty lines while typing; they are no argument.
const tidy = (action) => (action?.LaunchApp ? { LaunchApp: { ...action.LaunchApp, args: action.LaunchApp.args.filter(Boolean) } } : action);
const tidied = (config) =>
  Object.keys(config.jog.per_mode).reduce((c, mode) => {
    const b = bindingOf(c, mode);
    return withJog(c, mode, { left: tidy(b.left), right: tidy(b.right) });
  }, config);

/** One side of the wheel: what turning it this way does. */
export function SideEditor({ label, action, sign, config, error, onChange }) {
  const kind = kindOf(action);
  const pick = (k) => {
    if (!k) return onChange(null);
    if (k === "Scroll") return onChange({ Scroll: { amount: sign } });
    if (k === "Hotkey") return onChange({ Hotkey: { combo: { modifiers: [], key: "" } } });
    return onChange(blank(k, config, null));
  };
  return (
    <fieldset>
      <label>
        {label}
        <select value={kind} aria-invalid={!!error} onChange={(e) => pick(e.target.value)}>
          {Object.entries(SIDE_KINDS).map(([id, text]) => (
            <option key={id} value={id}>
              {text}
            </option>
          ))}
        </select>
      </label>
      {kind === "Scroll" && (
        <label>
          Lines
          <input
            type="number"
            value={action.Scroll.amount}
            onChange={(e) => onChange({ Scroll: { amount: number(e) } })}
          />
        </label>
      )}
      {kind === "Hotkey" && (
        <label>
          Shortcut{" "}
          <input
            className="combo-capture"
            readOnly
            placeholder="Click here, then press the shortcut"
            value={formatCombo(action.Hotkey.combo)}
            onKeyDown={(e) => {
              if (e.key === "Tab") return;
              e.preventDefault();
              onChange({ Hotkey: { combo: comboFromEvent(e) } });
            }}
          />
        </label>
      )}
      {kind === "LaunchApp" && <ActionForm action={action} config={config} layerId={null} onChange={onChange} />}
      {error && <span role="alert">{error}</span>}
    </fieldset>
  );
}

/** The Wheel tab: each mode's left and right actions and threshold, and the dial steps. */
export default function WheelTab({ initial = null, save = saveConfig }) {
  const [config, setConfig] = useState(initial);
  const [errors, setErrors] = useState({});
  useEffect(() => {
    if (initial || !globalThis.window?.__TAURI__) return;
    getConfig().then(setConfig, (e) => console.error("freewheel: loading the config failed:", String(e)));
  }, [initial]);
  if (!config) return null;
  const onSave = () => {
    const next = tidied(config);
    return saveWith(next, validateWheel(next), save, "wheel settings").then((r) => {
      setErrors(r.errors);
      if (r.config) setConfig(r.config);
    });
  };
  return (
    <section className="wheel-tab">
      {Object.entries(JOG_MODES).map(([mode, name]) => {
        const b = bindingOf(config, mode);
        const set = (patch) => setConfig(withJog(config, mode, patch));
        const thresholdError = errors[`${mode}.threshold`];
        return (
          <fieldset key={mode}>
            <legend>{name}</legend>
            <SideEditor label="Turn left" sign={-1} action={b.left} config={config} error={errors[`${mode}.left`]} onChange={(left) => set({ left })} />
            <SideEditor label="Turn right" sign={1} action={b.right} config={config} error={errors[`${mode}.right`]} onChange={(right) => set({ right })} />
            <label>
              {mode === "Shuttle" ? "Sensitivity" : "Ticks per step"}
              <input type="number" min="1" value={Number.isNaN(b.threshold) ? "" : b.threshold} aria-invalid={!!thresholdError} onChange={(e) => set({ threshold: number(e) })} />
              {thresholdError && <span role="alert">{thresholdError}</span>}
            </label>
          </fieldset>
        );
      })}
      <fieldset>
        <legend>When the wheel is a dial, each step changes</legend>
        {STEPS.map(([field, label, scale]) => {
          const value = config.overrides[field] * scale;
          return (
            <label key={field}>
              {`${label} (%)`}
              <input
                type="number"
                min="1"
                max="100"
                value={Number.isNaN(value) ? "" : Math.round(value)}
                aria-invalid={!!errors[field]}
                onChange={(e) => setConfig({ ...config, overrides: { ...config.overrides, [field]: number(e) / scale } })}
              />
              {errors[field] && <span role="alert">{errors[field]}</span>}
            </label>
          );
        })}
      </fieldset>
      {errors.save && <p role="alert">{errors.save}</p>}
      <button onClick={onSave}>Save</button>
    </section>
  );
}
