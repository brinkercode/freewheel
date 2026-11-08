// The General tab: appearance, start at login, the version and updates.
import Icon from "./Icon.jsx";
import { Switch } from "./ResolveTab.jsx";
import { UpdateProgress } from "./UpdateBanner.jsx";
import { releasePage } from "./whatsNew.js";

// The prototype's appearance icons (bundled SVGs, never fetched).
const ICONS = { System: "contrast", Light: "light_mode", Dark: "dark_mode" };

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

/** The line under 'freewheel <version>' for each update state (the prototype's updateSub); null is idle. */
export function updateText(update) {
  const v = update?.version;
  switch (update?.status) {
    case "checking":
      return "Checking…";
    case "current":
      return "You’re up to date.";
    case "available":
      return `freewheel ${v} is available.`;
    case "downloading":
      return update.progress == null ? "Downloading the new version…" : `Downloading the new version… ${update.progress}%`;
    case "ready":
      return "A new version is ready.";
    case "restarting":
      return "Restarting freewheel…";
    case "error":
      if (update.notConfigured) return "This test build can’t update itself. Released versions of freewheel can.";
      if (update.notWritable)
        return `freewheel can’t update itself in ${update.folder || "this folder"}. Move the freewheel folder somewhere you can change, like Documents, or download freewheel ${v} from GitHub.`;
      if (update.badSignature) return `The download of freewheel ${v} couldn’t be verified, so nothing was changed. Try again.`;
      if (update.cantInstall) return cantInstallText(update);
      if (update.during === "download") return `freewheel ${v} didn’t download. Check your internet connection and try again.`;
      if (update.during === "install") return `freewheel ${v} didn’t install. Try again.`;
      if (update.noManifest) return "Couldn’t check for updates. Try again later.";
      return "Couldn’t check for updates. Check your internet connection and try again.";
    default:
      return "freewheel checks for updates by itself.";
  }
}

/** Why this copy of freewheel can't update itself (the updater's reason), and where the new version is. */
export function cantInstallText(update) {
  const why = update.reason || "freewheel can’t update itself here";
  return update.version ? `freewheel ${update.version} is available. ${why}: download it from GitHub.` : `${why}: download the new version from GitHub.`;
}

/** The version card's button for each update state: what it says, does and whether it spins. */
export function updateButton(update, { onCheck, onDownload, onRestart, onReleasePage } = {}) {
  const status = update?.status;
  if (status === "error" && update.cantInstall && update.version)
    return { label: "Open the download page", primary: true, onClick: onReleasePage && (() => onReleasePage(update.version)), disabled: !onReleasePage };
  if (status === "checking") return { label: "Check for updates", spin: true, disabled: true };
  if (status === "downloading") return { label: "Downloading…", spin: true, disabled: true };
  if (status === "restarting") return { label: "Restarting…", spin: true, disabled: true };
  if (status === "available" || (status === "error" && update.during === "download"))
    return { label: status === "error" ? "Try again" : "Download and install", primary: true, onClick: onDownload, disabled: !onDownload };
  if (status === "ready" || (status === "error" && update.during === "install"))
    return { label: status === "error" ? "Try again" : "Restart to update", primary: true, onClick: onRestart, disabled: !onRestart };
  return { label: status === "error" ? "Try again" : "Check for updates", onClick: onCheck, disabled: !onCheck };
}

export default function General({ settings, onSetting, startAtLogin, onStartAtLogin, version, update, onCheck, onDownload, onRestart, onReleasePage, onWhatsNew, trayHint = null }) {
  const appearance = settings?.appearance ?? "System";
  const button = updateButton(update, { onCheck, onDownload, onRestart, onReleasePage });
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
                  <Icon name={ICONS[o.id]} />
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
        {/* Linux without a tray host (GNOME without the AppIndicator extension): say where the tray menu went. */}
        {trayHint && (
          <section className="card card-stack" aria-label="Tray">
            <div className="card-title">No tray icon</div>
            <div className="card-sub">{trayHint}</div>
          </section>
        )}
        <section className="card card-stack version-card">
          <div className="row">
            <div className="grow">
              <div className="card-title">{version ? `freewheel ${version}` : "freewheel"}</div>
              <div className={`card-sub${update?.status === "error" ? " is-error" : ""}`} role="status" data-update={update?.status ?? "idle"}>
                {updateText(update)}
              </div>
              {update?.status === "downloading" && <UpdateProgress progress={update.progress} />}
              {onWhatsNew && version && (
                <a
                  className="card-sub whats-new-link"
                  href={releasePage(version)}
                  onClick={(event) => {
                    event.preventDefault();
                    onWhatsNew();
                  }}
                >
                  What’s new in v{version}
                </a>
              )}
            </div>
            <button
              className={`update-button${button.primary ? " btn-primary" : ""}`}
              disabled={button.disabled}
              onClick={button.onClick}
            >
              {button.spin && <span className="spinner" aria-hidden="true" />}
              {button.label}
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
