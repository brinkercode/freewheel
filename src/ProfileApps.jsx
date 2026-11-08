import { useState } from "react";
import { AppPicker, appHue } from "./ActionEditor.jsx";
import Icon from "./Icon.jsx";
import {
  APP_NAME_HINT,
  PROFILE_COLORS,
  PROFILE_ICONS,
  THIS_OS,
  appOnOs,
  profileApps,
  profileLook,
  sameApp,
  setProfileApps,
  setProfileLook,
  typedApp,
} from "./keys.js";
import { profileApp, saveConfig, saveErrorText } from "./settingsApi.js";

const OS_NAMES = { windows: "Windows", macos: "macOS", linux: "Linux" };

/** The rule, as the engine applies it (crates/freewheel-core/src/profiles.rs). */
export const profileRule = (name) =>
  `When one of these apps is in front, your Speed Editor uses ${name}, starting on its first page. Any other app: back to Default, on the page it was on.`;

/** Shown where the app in front can't be seen (Linux on Wayland). */
export const NO_FRONT_APP = "On Wayland, freewheel can't see which app is in front, so profiles don't switch here.";

/** "Set for macOS only." for an app this computer's OS has no name for, else null. */
export function otherOsOnly(app, os = THIS_OS) {
  if (appOnOs(app, os)) return null;
  const names = Object.keys(OS_NAMES)
    .filter((o) => appOnOs(app, o))
    .map((o) => OS_NAMES[o]);
  return names.length ? `Set for ${names.join(" and ")} only.` : null;
}

/**
 * A profile's editor (the side panel while no key is picked on a profile):
 * its colour and icon, the apps it is for, the app picker shared with
 * 'Open an app', and a program name typed by hand. Every change saves.
 */
export default function ProfileApps({ config, profileId, save = saveConfig, onSaved, toApp = profileApp, supported = true, initialApps = null, loadApps, loadIcons, choose }) {
  const profile = (config.profiles ?? []).find((p) => p.id === profileId);
  const apps = profileApps(config, profileId);
  const [adding, setAdding] = useState(false);
  const [typed, setTyped] = useState("");
  const [error, setError] = useState(null);
  if (!profile) return null;
  const [what, example] = APP_NAME_HINT[THIS_OS];
  const look = profileLook(config, profileId);
  const commitLook = (change) => commit(setProfileLook(config, profileId, change));
  const commitApps = (next) => commit(setProfileApps(config, profileId, next));
  const commit = (changed) => {
    return Promise.resolve(save(changed)).then(
      () => (setError(null), onSaved?.(changed), true),
      (e) => (setError(saveErrorText(e)), false),
    );
  };
  const add = (app) => {
    if (!appOnOs(app)) return Promise.resolve(false);
    const same = apps.find((a) => sameApp(a, app));
    if (same) return Promise.resolve(setError(`${same.name} is already on ${profile.name}.`)).then(() => false);
    return commitApps([...apps, app]).then((ok) => (ok && setAdding(false), ok));
  };
  const pick = (choice) =>
    Promise.resolve(toApp(choice)).then(add, (e) => setError(`${choice.name} could not be added: ${String(e)}`));
  const addTyped = () => typed.trim() && add(typedApp(typed)).then((ok) => ok && setTyped(""));
  const pickerProps = Object.fromEntries(Object.entries({ initialApps, loadApps, loadIcons, choose }).filter(([, v]) => v !== undefined));
  const showPicker = adding || apps.length === 0;
  return (
    <section className="key-panel profile-panel" aria-label={`${profile.name} apps`}>
      <header className="panel-head">
        <span className="keycap profile-keycap" data-color={look.color} aria-hidden="true">
          <Icon name={look.icon} />
        </span>
        <div className="grow">
          <h2 className="panel-title">{`When is ${profile.name} used?`}</h2>
          <div className="panel-now">
            {apps.length === 0 ? "Add the apps it is for" : `For ${apps.length} app${apps.length === 1 ? "" : "s"}`}
          </div>
        </div>
      </header>
      <div className="panel-body">
        <p className="mode-sub">{profileRule(profile.name)}</p>
        <div className="field-label first">Colour</div>
        <div className="profile-swatches" role="radiogroup" aria-label="Colour">
          {PROFILE_COLORS.map((c) => (
            <button
              key={c}
              role="radio"
              aria-checked={look.color === c}
              aria-label={c}
              title={c[0].toUpperCase() + c.slice(1)}
              className="profile-swatch"
              data-color={c}
              onClick={() => look.color !== c && commitLook({ color: c })}
            />
          ))}
        </div>
        <div className="field-label">Icon</div>
        <div className="profile-icons" role="radiogroup" aria-label="Icon" data-color={look.color}>
          {PROFILE_ICONS.map((i) => (
            <button
              key={i}
              role="radio"
              aria-checked={look.icon === i}
              aria-label={i.replace("_", " ")}
              title={i.replace("_", " ")}
              className="profile-icon-choice"
              onClick={() => look.icon !== i && commitLook({ icon: i })}
            >
              <Icon name={i} />
            </button>
          ))}
        </div>
        {!supported && (
          <div className="panel-note small" role="note">
            {NO_FRONT_APP}
          </div>
        )}
        {apps.length > 0 && (
          <div className="app-list profile-apps" role="list" aria-label={`Apps for ${profile.name}`}>
            {apps.map((app, i) => (
              <div key={`${app.name}-${i}`} role="listitem" className="app-row profile-app">
                <span className="app-initial" data-hue={appHue(app.name)} aria-hidden="true">
                  {app.name.charAt(0).toUpperCase()}
                </span>
                <span className="grow">
                  {app.name}
                  <span className="profile-app-id">{appOnOs(app) && appOnOs(app) !== app.name ? appOnOs(app) : otherOsOnly(app)}</span>
                </span>
                <button className="icon-btn" title={`Remove ${app.name}`} aria-label={`Remove ${app.name}`} onClick={() => commitApps(apps.filter((_, j) => j !== i))}>
                  <Icon name="close" />
                </button>
              </div>
            ))}
          </div>
        )}
        {showPicker ? (
          <>
            <div className="field-label">Add an app</div>
            <AppPicker onPick={pick} {...pickerProps} />
            <div className="field-label">Or type its program name</div>
            <div className="input-row">
              <input
                aria-label="Program name"
                value={typed}
                placeholder={`like ${example}`}
                title={`${what[0].toUpperCase()}${what.slice(1)}, like ${example}`}
                onChange={(e) => setTyped(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && addTyped()}
              />
              <button className="btn-primary btn-md" onClick={addTyped}>
                Add
              </button>
            </div>
          </>
        ) : (
          <button className="btn-md profile-add-app" onClick={() => setAdding(true)}>
            <Icon name="add" />
            Add an app
          </button>
        )}
        {error && <p role="alert">{error}</p>}
        <div className="panel-note small with-icon">
          <Icon name="touch_app" />
          <span>{`Click a key in the drawing to choose what it does on this page of ${profile.name}.`}</span>
        </div>
      </div>
    </section>
  );
}
