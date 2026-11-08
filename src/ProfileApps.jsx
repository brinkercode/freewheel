import { useRef, useState } from "react";
import { AppPicker, appHue } from "./ActionEditor.jsx";
import Icon, { SymbolIcon } from "./Icon.jsx";
import { ICON_CATALOGUE, bareIcon, iconLabel, iconName, searchIcons, writtenIcon } from "./symbols.js";
import {
  APP_NAME_HINT,
  PROFILE_COLORS,
  SUGGESTED_ICONS,
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
          <SymbolIcon name={look.icon} />
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
        <IconPicker icon={look.icon} color={look.color} onPick={(i) => look.icon !== i && commitLook({ icon: i })} />
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

/** What the icon search says about `query` (null: nothing to say). */
export function iconSearchNote(query, results) {
  const bare = bareIcon(query);
  if (!bare) return null;
  const named = /^\s*:.*:\s*$/.test(query);
  if (named && !iconName(query)) return `There is no icon called ${writtenIcon(bare)}. Pick one below, or browse all icons for its name.`;
  if (!results.length) return `No icon matches "${query.trim()}". Try another word, or browse all icons.`;
  return null;
}

/** Opens Google's icon catalogue in the default browser (the user clicked the link; capabilities/settings.json allows only it and GitHub). */
export const openIconCatalogue = () =>
  globalThis.window.__TAURI__.core.invoke("plugin:opener|open_url", { url: ICON_CATALOGUE }).catch((e) => console.error("freewheel: opening the icon catalogue failed:", String(e)));

/**
 * A profile's icon: a search over every bundled icon (type a word or a
 * :name:; arrows move through the results, Enter picks), a row of
 * suggestions and a link to the catalogue of every icon.
 */
export function IconPicker({ icon, color, onPick, open = openIconCatalogue, columns = 8 }) {
  const [query, setQuery] = useState("");
  const [at, setAt] = useState(0);
  const grid = useRef(null);
  const results = searchIcons(query);
  const note = iconSearchNote(query, results);
  const pick = (name) => (onPick(name), setQuery(""), setAt(0));
  const focus = (i) => {
    const n = Math.max(0, Math.min(results.length - 1, i));
    setAt(n);
    grid.current?.querySelectorAll("[role=option]")[n]?.focus();
  };
  const onSearchKey = (e) => {
    if (e.key === "Enter" && results.length) {
      e.preventDefault();
      pick(iconName(query) ?? results[Math.min(at, results.length - 1)]);
    } else if (e.key === "ArrowDown" && results.length) {
      e.preventDefault();
      focus(0);
    } else if (e.key === "Escape" && query) {
      e.preventDefault();
      setQuery("");
    }
  };
  const onGridKey = (e, i) => {
    // Columns as drawn (the panel's width decides), else `columns`.
    const tops = [...(grid.current?.querySelectorAll("[role=option]") ?? [])].map((o) => o.offsetTop);
    const drawn = tops.filter((t) => t === tops[0]).length;
    const cols = drawn > 1 && drawn < tops.length ? drawn : columns;
    const step = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: cols, ArrowUp: -cols, Home: -i, End: results.length - 1 - i }[e.key];
    if (step !== undefined) {
      e.preventDefault();
      if (e.key === "ArrowUp" && i < cols) {
        e.currentTarget.closest(".icon-picker")?.querySelector("input")?.focus();
        return;
      }
      focus(i + step);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      pick(results[i]);
    }
  };
  const choice = (name, i, role) => (
    <button
      key={name}
      role={role}
      aria-label={iconLabel(name)}
      title={writtenIcon(name)}
      className="profile-icon-choice"
      data-name={name}
      {...(role === "radio" ? { "aria-checked": icon === name } : { "aria-selected": icon === name, tabIndex: i === at ? 0 : -1 })}
      onKeyDown={role === "option" ? (e) => onGridKey(e, i) : undefined}
      onMouseDown={(e) => e.preventDefault()}
      onClick={() => pick(name)}
    >
      <SymbolIcon name={name} />
    </button>
  );
  return (
    <div className="icon-picker" data-color={color}>
      <div className="profile-icons" role="radiogroup" aria-label="Suggested icons">
        {SUGGESTED_ICONS.map((name, i) => choice(name, i, "radio"))}
      </div>
      <div className="icon-search-row">
        <span className="icon-search-current" title={`Now ${writtenIcon(icon)}`}>
          <SymbolIcon name={icon} />
        </span>
        <input
          type="search"
          className="icon-search"
          aria-label="Search icons"
          placeholder="Search icons, or type :name:"
          value={query}
          spellCheck={false}
          autoComplete="off"
          aria-controls="icon-results"
          onChange={(e) => (setQuery(e.target.value), setAt(0))}
          onKeyDown={onSearchKey}
        />
      </div>
      {note && (
        <p className="icon-search-note" role="status">
          {note}
        </p>
      )}
      {results.length > 0 && (
        <div className="profile-icons icon-results" id="icon-results" role="listbox" aria-label="Icons found" ref={grid}>
          {results.map((name, i) => choice(name, i, "option"))}
        </div>
      )}
      <button type="button" className="icon-browse" onClick={() => open()}>
        <Icon name="search" />
        Browse all icons
      </button>
    </div>
  );
}
