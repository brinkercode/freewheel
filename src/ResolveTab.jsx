import { useEffect, useState } from "react";
import Icon from "./Icon.jsx";

// The DaVinci Resolve tab (screens-and-components 3d): the step-aside switch,
// the unplug tip and, under More, Resolve's program name with Reset.

/** Resolve's program name on this OS, as Settings::default (config.rs) has it. */
export const resolveDefault = (userAgent = globalThis.navigator?.userAgent ?? "") =>
  /Windows/.test(userAgent) ? "Resolve.exe" : "Resolve";

export const validateResolve = (name) => (name.trim() ? {} : { name: "Enter Resolve's program name." });

/** The shared On/Off switch. */
export function Switch({ label, on, onChange }) {
  return (
    <button
      role="switch"
      className="switch"
      aria-label={label}
      aria-checked={on === true}
      disabled={on == null || !onChange}
      onClick={() => onChange(!on)}
    >
      <span className="switch-knob" />
    </button>
  );
}

/** `settings` is the config's settings; `onSetting(key, value)` saves one (General.jsx changeSetting). */
export default function ResolveTab({ settings, onSetting, osDefault = resolveDefault(), initialMore = false }) {
  const [more, setMore] = useState(initialMore);
  const [name, setName] = useState(settings?.resolve_process_name ?? "");
  const [errors, setErrors] = useState({});
  useEffect(() => {
    if (settings) setName(settings.resolve_process_name);
  }, [settings?.resolve_process_name]);
  const saveName = (value) => {
    const e = validateResolve(value);
    setErrors(e);
    if (!e.name && value !== settings?.resolve_process_name) onSetting?.("resolve_process_name", value.trim());
  };
  return (
    <div className="tab-page" data-screen-label="DaVinci Resolve">
      <div className="tab-column">
        <h2 className="tab-title">DaVinci Resolve</h2>
        <section className="card card-row card-top">
          <div className="grow">
            <div className="card-title">Step aside automatically when DaVinci Resolve opens</div>
            <div className="card-sub relaxed">
              When you open Resolve, freewheel lets go of your Speed Editor so Resolve can use it. Your shortcuts come back when
              you close Resolve.
            </div>
          </div>
          <Switch
            label="Step aside automatically when DaVinci Resolve opens"
            on={settings ? settings.step_aside_for_resolve !== false : null}
            onChange={onSetting && ((on) => onSetting("step_aside_for_resolve", on))}
          />
        </section>
        <div className="tip-card">
          <Icon name="lightbulb" />
          <span>If Resolve doesn’t see the Speed Editor, unplug it and plug it back in.</span>
        </div>
        <button className="more" aria-expanded={more} onClick={() => setMore(!more)}>
          More
          <Icon name={more ? "expand_less" : "expand_more"} />
        </button>
        {more && (
          <section className="card card-stack">
            <div>
              <div className="card-title">Resolve’s program name</div>
              <div className="card-sub small">Only change this if support asks you to.</div>
            </div>
            <div className="input-row">
              <input
                className="program-name"
                aria-label="Resolve’s program name"
                value={name}
                placeholder={osDefault}
                aria-invalid={!!errors.name}
                onChange={(e) => setName(e.target.value)}
                onBlur={(e) => saveName(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && saveName(e.target.value)}
              />
              <button onClick={() => (setName(osDefault), saveName(osDefault))}>Reset</button>
            </div>
            {errors.name && <span role="alert">{errors.name}</span>}
          </section>
        )}
      </div>
    </div>
  );
}
