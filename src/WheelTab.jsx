import { useEffect, useState } from "react";
import { RecordBox } from "./ActionEditor.jsx";
import Icon from "./Icon.jsx";
import { comboError } from "./keys.js";
import { getConfig, saveConfig, saveErrorText } from "./settingsApi.js";

// The Wheel tab (screens-and-components 3c). The three buttons above the wheel
// (SHTL, JOG, SCRL) each switch the wheel to a JogMode with a SetJogMode
// binding on the first page; what the wheel does in a mode is jog.per_mode
// (left and right actions), or a Volume / Brightness mode (the dial controls).

/** [key, printed name, the JogMode it naturally switches to]. */
export const WHEEL_BUTTONS = [
  ["Shtl", "SHTL", "Shuttle"],
  ["Jog", "JOG", "Jog"],
  ["Scrl", "SCRL", "Scroll"],
];

/** What the wheel does, as the prototype's WMODE names it. */
export const WHEEL_DOES = { scroll: "Scrolls", keys: "Shortcuts", volume: "Volume", brightness: "Brightness", none: "Nothing" };

export const WHEEL_OPTIONS = [
  ["scroll", "swap_vert", "Scroll up and down", "like a mouse wheel · usual choice"],
  ["keys", "keyboard", "Press shortcuts", "one for left, one for right"],
  ["volume", "volume_up", "Change the volume", ""],
  ["brightness", "light_mode", "Change screen brightness", ""],
  ["none", "block", "Nothing", ""],
];

const NO_BINDING = { left: null, right: null, threshold: 1, shuttle_max_rate_hz: null };
export const bindingOf = (config, mode) => config.jog?.per_mode?.[mode] ?? NO_BINDING;
const basePage = (config) => config.layers.find((l) => l.id === config.base_layer) ?? config.layers[0];

/** The JogMode wheel button `key` switches to. */
export function buttonMode(config, key) {
  const natural = WHEEL_BUTTONS.find(([k]) => k === key)[2];
  return basePage(config)?.bindings?.[key]?.SetJogMode?.mode ?? natural;
}

/** What the wheel does in `mode`: scroll, keys, volume, brightness or none. */
export function wheelKind(config, mode) {
  if (mode === "Volume") return "volume";
  if (mode === "Brightness") return "brightness";
  const b = bindingOf(config, mode);
  if (!b.left && !b.right) return "none";
  if (b.left?.Scroll && b.right?.Scroll) return "scroll";
  return "keys";
}

/** What wheel button `key` makes the wheel do, in the prototype's words. */
export const buttonDoes = (config, key) => WHEEL_DOES[wheelKind(config, buttonMode(config, key))];

/** The wheel button whose mode the wheel starts in (lit on the device). */
export function liveButton(config) {
  return WHEEL_BUTTONS.find(([k]) => buttonMode(config, k) === (config.jog?.default_mode ?? "Scroll"))?.[0] ?? null;
}

/** The config with the `mode` binding patched. */
export const withJog = (config, mode, patch) => ({
  ...config,
  jog: { ...config.jog, per_mode: { ...config.jog.per_mode, [mode]: { ...bindingOf(config, mode), ...patch } } },
});

const withButton = (config, key, mode) => {
  const base = basePage(config);
  return {
    ...config,
    layers: config.layers.map((l) => (l.id === base.id ? { ...l, bindings: { ...l.bindings, [key]: { SetJogMode: { mode } } } } : l)),
  };
};

/**
 * The config with wheel button `key` set to `kind`: Volume and Brightness
 * switch to their own JogMode; scroll, keys and none keep the button's own
 * mode with its left and right actions. The wheel's start mode follows.
 */
export function setButtonKind(config, key, kind) {
  const natural = WHEEL_BUTTONS.find(([k]) => k === key)[2];
  const before = buttonMode(config, key);
  const mode = kind === "volume" ? "Volume" : kind === "brightness" ? "Brightness" : natural;
  let next = withButton(config, key, mode);
  if (kind === "scroll") next = withJog(next, mode, { left: { Scroll: { amount: -1 } }, right: { Scroll: { amount: 1 } } });
  if (kind === "none") next = withJog(next, mode, { left: null, right: null });
  if (kind === "keys") {
    const b = bindingOf(config, mode);
    next = withJog(next, mode, { left: b.left?.Hotkey ? b.left : null, right: b.right?.Hotkey ? b.right : null });
  }
  if (config.jog.default_mode === before) next = { ...next, jog: { ...next.jog, default_mode: mode } };
  return next;
}

// Sensitivity: the slider is 0 (slow) .. 100 (fast); the wheel takes 10 .. 1 ticks per step.
export const sensitivityOf = (threshold) => Math.round(((10 - Math.min(10, Math.max(1, threshold))) / 9) * 100);
export const thresholdOf = (sensitivity) => Math.max(1, Math.min(10, Math.round(10 - (sensitivity / 100) * 9)));

/** The config with every wheel mode's ticks per step set from `sensitivity`. */
export function withSensitivity(config, sensitivity) {
  const threshold = thresholdOf(sensitivity);
  return Object.keys(config.jog.per_mode).reduce((c, mode) => withJog(c, mode, { threshold }), config);
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

const comboOf = (action) => action?.Hotkey?.combo ?? null;

/** The Wheel tab: which button, what turning does with it lit, and the sensitivity. */
export default function WheelTab({ initial = null, save = saveConfig }) {
  const [config, setConfig] = useState(initial);
  const [button, setButton] = useState(null);
  const [errors, setErrors] = useState({});
  const [listen, setListen] = useState(null);
  // "Press shortcuts" picked, nothing recorded yet: both sides are still empty.
  const [keysFor, setKeysFor] = useState(null);
  useEffect(() => {
    if (initial || !globalThis.window?.__TAURI__) return;
    getConfig().then(setConfig, (e) => console.error("freewheel: loading the config failed:", String(e)));
  }, [initial]);
  if (!config) return null;
  const selected = button ?? liveButton(config) ?? "Jog";
  const printed = WHEEL_BUTTONS.find(([k]) => k === selected)[1];
  const live = liveButton(config);
  const mode = buttonMode(config, selected);
  const stored = wheelKind(config, mode);
  const kind = stored === "none" && keysFor === selected ? "keys" : stored;
  const binding = bindingOf(config, mode);
  const apply = (next, error = {}) => {
    setConfig(next);
    return saveWith(next, error, save, "wheel settings").then((r) => setErrors(r.errors));
  };
  const pickKind = (k) => {
    const next = setButtonKind(config, selected, k);
    const b = bindingOf(next, buttonMode(next, selected));
    setListen(k === "keys" && !b.left ? "left" : null);
    setKeysFor(k === "keys" ? selected : null);
    return apply(next);
  };
  const recordSide = (side) => (combo) => {
    const error = comboError(combo);
    if (error) return setErrors({ [side]: error });
    setListen(null);
    return apply(withJog(config, mode, { [side]: { Hotkey: { combo } } }));
  };
  const sensitivity = sensitivityOf(bindingOf(config, config.jog.default_mode).threshold);
  return (
    <div className="tab-page" data-screen-label="Wheel">
      <div className="tab-column">
        <h2 className="tab-title">When I turn the wheel…</h2>
        <section className="card card-stack wheel-card">
          <div className="card-text">
            The three buttons above the wheel can each make it do something different. Press one on your Speed Editor to switch.
          </div>
          <div className="wheel-buttons" role="group" aria-label="Wheel buttons">
            {WHEEL_BUTTONS.map(([key, label]) => (
              <button key={key} className="wheel-button" aria-pressed={key === selected} onClick={() => (setButton(key), setListen(null))}>
                <span className="wheel-button-label">
                  {label}
                  {key === live && <span className="led-dot" title="Lit on your Speed Editor" />}
                </span>
                <span className="wheel-button-does">{buttonDoes(config, key)}</span>
              </button>
            ))}
          </div>
          <div className="field-label">{`With ${printed} lit, turning the wheel…`}</div>
          <div className="radio-rows" role="radiogroup" aria-label={`With ${printed} lit, turning the wheel…`}>
            {WHEEL_OPTIONS.map(([k, icon, title, sub]) => (
              <button key={k} role="radio" aria-checked={k === kind} className={`radio-row${k === kind ? " is-current" : ""}`} onClick={() => pickKind(k)}>
                <span className="radio" />
                <Icon name={icon} className="icon radio-icon" />
                <span className="grow">
                  <span className="radio-title">{title}</span> <span className="radio-sub">{sub}</span>
                </span>
              </button>
            ))}
          </div>
          {kind === "keys" && (
            <div className="wheel-records">
              {[
                ["left", "rotate_left", "Turning left presses"],
                ["right", "rotate_right", "Turning right presses"],
              ].map(([side, icon, label]) => (
                <div key={`${selected}-${side}`} className="wheel-record">
                  <div className="wheel-record-label">
                    <Icon name={icon} />
                    {label}
                  </div>
                  <RecordBox compact label={label} combo={comboOf(binding[side])} error={errors[side]} autoListen={listen === side} onRecord={recordSide(side)} />
                  {errors[side] && <span role="alert">{errors[side]}</span>}
                </div>
              ))}
            </div>
          )}
          {errors.save && <p role="alert">{errors.save}</p>}
        </section>
        <section className="card card-stack">
          <div>
            <div className="card-title">Sensitivity</div>
            <div className="card-sub small">How much happens for each turn of the wheel.</div>
          </div>
          <div className="slider-row">
            <span>Slow</span>
            <input
              type="range"
              min="0"
              max="100"
              aria-label="Sensitivity"
              value={sensitivity}
              onChange={(e) => apply(withSensitivity(config, Number(e.target.value)))}
            />
            <span>Fast</span>
          </div>
        </section>
      </div>
    </div>
  );
}
