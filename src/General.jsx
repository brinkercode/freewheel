// The General tab: appearance, start at login, the version and updates.

const panel = {
  background: "var(--panel, Canvas)",
  border: "1px solid var(--line, GrayText)",
  borderRadius: 12,
  padding: 18,
  display: "flex",
};
const title = { fontSize: 14, fontWeight: 600 };
const sub = { fontSize: 13, color: "var(--ink2, GrayText)", marginTop: 3 };

// Icons are inline SVG, bundled with the app: no icon font.
const ICONS = {
  System: "M3 4h18v12H3zM8 20h8M12 16v4",
  Light: "M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8M12 2v2M12 20v2M2 12h2M20 12h2",
  Dark: "M20 14A8 8 0 1 1 10 4a6 6 0 0 0 10 10z",
};

export const APPEARANCES = [
  { id: "System", label: "Match my computer", swatch: "linear-gradient(90deg, #fff 50%, #1c1c1e 50%)", bar: "#8e8e93" },
  { id: "Light", label: "Light", swatch: "#fff", bar: "#d1d1d6" },
  { id: "Dark", label: "Dark", swatch: "#1c1c1e", bar: "#48484a" },
];

const SCHEME = { System: "light dark", Light: "light", Dark: "dark" };

/** Applies the appearance to the window at once through the native colour scheme. */
export function applyAppearance(appearance, root = globalThis.document?.documentElement) {
  if (!root) return;
  root.style.colorScheme = SCHEME[appearance] ?? SCHEME.System;
  root.dataset.appearance = appearance;
}

/** Changes one setting: shows it at once, then saves the config; a failed save is logged. */
export async function changeSetting({ config, key, value, setConfig, save, root, log = console.error }) {
  const next = { ...config, settings: { ...config.settings, [key]: value } };
  setConfig(next);
  if (key === "appearance") applyAppearance(value, root);
  try {
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
      aria-label={label}
      aria-checked={on === true}
      disabled={on == null || !onChange}
      onClick={() => onChange(!on)}
      style={{
        width: 40,
        height: 22,
        borderRadius: 11,
        border: "none",
        padding: 0,
        position: "relative",
        cursor: "pointer",
        flex: "none",
        background: on ? "var(--accent, AccentColor)" : "var(--line2, GrayText)",
      }}
    >
      <span
        className="switch-knob"
        style={{
          position: "absolute",
          top: 3,
          left: on ? 21 : 3,
          width: 16,
          height: 16,
          borderRadius: "50%",
          background: "#fff",
          boxShadow: "0 1px 2px rgba(0,0,0,.3)",
          transition: "left .15s",
        }}
      />
    </button>
  );
}

export default function General({ settings, onSetting, startAtLogin, onStartAtLogin, version, update, onCheck }) {
  const appearance = settings?.appearance ?? "System";
  const busy = update?.status === "checking" || update?.status === "downloading";
  return (
    <div style={{ flex: 1, overflow: "auto", padding: "24px 28px" }} data-screen-label="General">
      <div style={{ maxWidth: 640, margin: "0 auto", display: "flex", flexDirection: "column", gap: 14 }}>
        <h2 style={{ margin: 0, fontSize: 19, fontWeight: 600 }}>General</h2>
        <section style={{ ...panel, flexDirection: "column", gap: 12 }}>
          <div>
            <div style={title}>Appearance</div>
            <div style={sub}>Choose light or dark, or let freewheel follow your computer.</div>
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(3,minmax(0,1fr))", gap: 8 }}>
            {APPEARANCES.map((o) => (
              <button
                key={o.id}
                aria-pressed={o.id === appearance}
                disabled={!onSetting}
                onClick={() => onSetting("appearance", o.id)}
                style={{
                  display: "flex",
                  flexDirection: "column",
                  alignItems: "stretch",
                  gap: 8,
                  padding: 8,
                  borderRadius: 10,
                  border: `2px solid ${o.id === appearance ? "var(--accent, AccentColor)" : "var(--line, GrayText)"}`,
                  background: "var(--panel, Canvas)",
                  cursor: "pointer",
                }}
              >
                <span
                  className="preview"
                  style={{
                    height: 54,
                    borderRadius: 6,
                    background: o.swatch,
                    border: "1px solid var(--line, GrayText)",
                    display: "flex",
                    alignItems: "flex-end",
                    padding: 6,
                    gap: 4,
                  }}
                >
                  <span style={{ width: "40%", height: 6, borderRadius: 3, background: "var(--accent, AccentColor)" }} />
                  <span style={{ width: "25%", height: 6, borderRadius: 3, background: o.bar }} />
                </span>
                <span style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13, fontWeight: 600 }}>
                  <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
                    <path d={ICONS[o.id]} />
                  </svg>
                  {o.label}
                </span>
              </button>
            ))}
          </div>
        </section>
        <section style={{ ...panel, gap: 16, alignItems: "center" }}>
          <div style={{ ...title, flex: 1 }}>Start freewheel when I log in</div>
          <Switch label="Start freewheel when I log in" on={startAtLogin} onChange={onStartAtLogin} />
        </section>
        <section style={{ ...panel, flexDirection: "column", gap: 14 }}>
          <div style={{ display: "flex", gap: 16, alignItems: "center" }}>
            <div style={{ flex: 1 }}>
              <div style={title}>{version ? `freewheel ${version}` : "freewheel"}</div>
              <div style={sub}>{updateText(update)}</div>
            </div>
            <button
              disabled={busy || !onCheck}
              onClick={onCheck}
              style={{ height: 36, padding: "0 16px", borderRadius: 8, display: "flex", alignItems: "center", gap: 8, flex: "none" }}
            >
              {update?.status === "checking" && (
                <span
                  className="spinner"
                  style={{
                    width: 14,
                    height: 14,
                    borderRadius: "50%",
                    border: "2px solid var(--line2, GrayText)",
                    borderTopColor: "var(--accent, AccentColor)",
                    animation: "fwspin .9s linear infinite",
                  }}
                />
              )}
              Check for updates
            </button>
          </div>
          <div style={{ height: 1, background: "var(--line, GrayText)" }} />
          <div style={{ display: "flex", gap: 16, alignItems: "center" }}>
            <div style={{ flex: 1 }}>
              <div style={title}>Update automatically</div>
              <div style={sub}>New versions download by themselves. You choose when to restart.</div>
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
