// The General tab: appearance, start at login, the version and updates.

// Icons are inline SVG, bundled with the app: no icon font.
const ICONS = {
  System: "M3 4h18v12H3zM8 20h8M12 16v4",
  Light: "M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8M12 2v2M12 20v2M2 12h2M20 12h2",
  Dark: "M20 14A8 8 0 1 1 10 4a6 6 0 0 0 10 10z",
};

export const APPEARANCES = [
  { id: "System", label: "Match my computer" },
  { id: "Light", label: "Light" },
  { id: "Dark", label: "Dark" },
];

const SCHEME = { System: "light dark", Light: "light", Dark: "dark" };

/** Applies the appearance to the window at once through the native colour scheme. */
export function applyAppearance(appearance, root = globalThis.document?.documentElement) {
  if (!root) return;
  root.style.colorScheme = SCHEME[appearance] ?? SCHEME.System;
  root.dataset.appearance = appearance;
}

/** Changes one setting: shows it at once, then saves the config; a failed save is logged. */
export async function changeSetting({ config, key, value, setConfig, save, load, root, log = console.error }) {
  if (key === "appearance") applyAppearance(value, root);
  try {
    // The running config, read fresh: mappings changed since this window loaded must not be saved over.
    const current = load ? await load() : config;
    const next = { ...current, settings: { ...current.settings, [key]: value } };
    setConfig(next);
    await save(next);
  } catch (error) {
    log("freewheel: saving", key, "failed:", error);
  }
}

/** Sets the OS login item, then shows what the OS really has; a failure is logged and the switch re-reads the OS. */
export async function switchLogin({ on, set, get, setLogin, setConfig, log = console.error }) {
  try {
    setLogin(await set(on));
    setConfig((c) => c && { ...c, settings: { ...c.settings, start_at_login: on } });
  } catch (error) {
    log("freewheel: start at login failed:", error);
    setLogin(await get().catch(() => null));
  }
}

/** The line under 'freewheel <version>' for each update state; null is idle. */
export function updateText(update) {
  switch (update?.status) {
    case "checking":
      return "Checking for updates…";
    case "current":
      return "You're up to date.";
    case "available":
      return `freewheel ${update.version} is available.`;
    case "downloading":
      return update.progress == null
        ? `Downloading freewheel ${update.version}…`
        : `Downloading freewheel ${update.version}… ${update.progress}%`;
    case "ready":
      return `freewheel ${update.version} is ready. Restart to update.`;
    case "restarting":
      return "Restarting…";
    default:
      return "freewheel checks for updates when it starts.";
  }
}

function Switch({ label, on, onChange }) {
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

export default function General({ settings, onSetting, startAtLogin, onStartAtLogin, version, update, onCheck }) {
  const appearance = settings?.appearance ?? "System";
  const busy = update?.status === "checking" || update?.status === "downloading";
  return (
    <div className="tab-page" data-screen-label="General">
      <div className="tab-column">
        <h2 className="tab-title">General</h2>
        <section className="card card-stack">
          <div>
            <div className="card-title">Appearance</div>
            <div className="card-sub">Choose light or dark, or let freewheel follow your computer.</div>
          </div>
          <div className="appearance-grid">
            {APPEARANCES.map((o) => (
              <button
                key={o.id}
                className="appearance-option"
                aria-pressed={o.id === appearance}
                disabled={!onSetting}
                onClick={() => onSetting("appearance", o.id)}
              >
                <span className="preview" data-swatch={o.id}>
                  <span className="preview-accent" />
                  <span className="preview-bar" />
                </span>
                <span className="appearance-label">
                  <svg className="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
                    <path d={ICONS[o.id]} />
                  </svg>
                  {o.label}
                </span>
              </button>
            ))}
          </div>
        </section>
        <section className="card card-row">
          <div className="card-title grow">Start freewheel when I log in</div>
          <Switch label="Start freewheel when I log in" on={startAtLogin} onChange={onStartAtLogin} />
        </section>
        <section className="card card-stack">
          <div className="row">
            <div className="grow">
              <div className="card-title">{version ? `freewheel ${version}` : "freewheel"}</div>
              <div className="card-sub">{updateText(update)}</div>
            </div>
            <button className="btn" disabled={busy || !onCheck} onClick={onCheck}>
              {update?.status === "checking" && <span className="spinner" />}
              Check for updates
            </button>
          </div>
          <div className="divider" />
          <div className="row">
            <div className="grow">
              <div className="card-title">Update automatically</div>
              <div className="card-sub">New versions download by themselves. You choose when to restart.</div>
            </div>
            <Switch
              label="Update automatically"
              on={settings ? settings.auto_update !== false : true}
              onChange={onSetting && ((on) => onSetting("auto_update", on))}
            />
          </div>
        </section>
      </div>
    </div>
  );
}
