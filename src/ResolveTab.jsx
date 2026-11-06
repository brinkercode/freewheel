import { useEffect, useState } from "react";
import { getConfig, saveConfig } from "./settingsApi.js";
import { saveWith } from "./WheelTab.jsx";

/** Resolve's program name on this OS, as Settings::default (config.rs) has it. */
export const resolveDefault = (userAgent = globalThis.navigator?.userAgent ?? "") =>
  /Windows/.test(userAgent) ? "Resolve.exe" : "Resolve";

export const validateResolve = (config) =>
  config.settings.resolve_process_name.trim() ? {} : { name: "Enter Resolve's program name." };

/** The DaVinci Resolve tab: the program name freewheel steps aside for, with its OS default. */
export default function ResolveTab({ initial = null, save = saveConfig, osDefault = resolveDefault() }) {
  const [config, setConfig] = useState(initial);
  const [errors, setErrors] = useState({});
  useEffect(() => {
    if (initial || !globalThis.window?.__TAURI__) return;
    getConfig().then(setConfig, (e) => console.error("freewheel: loading the config failed:", String(e)));
  }, [initial]);
  if (!config) return null;
  const setName = (resolve_process_name) => setConfig({ ...config, settings: { ...config.settings, resolve_process_name } });
  const onSave = () =>
    saveWith(config, validateResolve(config), save, "Resolve program name").then((r) => {
      setErrors(r.errors);
      if (r.config) setConfig(r.config);
    });
  return (
    <section className="resolve-tab">
      <p>freewheel steps aside while DaVinci Resolve is open, so Resolve can use the Speed Editor.</p>
      <p>Tip: if Resolve doesn't notice the Speed Editor, unplug it and plug it back in.</p>
      <label>
        Resolve's program name
        <input
          value={config.settings.resolve_process_name}
          placeholder={osDefault}
          aria-invalid={!!errors.name}
          onChange={(e) => setName(e.target.value)}
        />
        {errors.name && <span role="alert">{errors.name}</span>}
      </label>
      <p>{`Default on this computer: ${osDefault}`}</p>
      <button onClick={() => setName(osDefault)}>Reset</button>
      {errors.save && <p role="alert">{errors.save}</p>}
      <button onClick={onSave}>Save</button>
    </section>
  );
}
