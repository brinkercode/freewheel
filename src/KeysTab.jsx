import { useEffect, useRef, useState } from "react";
import ActionEditor, { KeyPanel } from "./ActionEditor.jsx";
import drawing from "./assets/speed-editor.svg?raw";
import { KEYS, LABELS, PRESS_CHIP_MS, addPage, deletePage, describeAction, renamePage } from "./keys.js";
import { getConfig, onKeyPressed, saveConfig, saveErrorText } from "./settingsApi.js";
import { showToast } from "./shell.js";

export { KIND_ROWS, KIND_TEXT, saveKey } from "./ActionEditor.jsx";

/** The key panel; `KeyEditor` is its old name. */
export const KeyEditor = KeyPanel;

/** A physical press: shows 'You pressed X' for PRESS_CHIP_MS and selects that key. */
export function handlePress(press, { select, setChip, timer, timers = globalThis }) {
  select(press.key);
  timers.clearTimeout(timer.current);
  timer.current = showToast(setChip, `You pressed ${press.label}`, timers, PRESS_CHIP_MS);
}

/** Page pills (shown once a 2nd page exists), rename, add and delete. */
export function PageSelector({ config, page, onPage, onAdd, onRename, onDelete, error }) {
  const current = config.layers.find((l) => l.id === page);
  const many = config.layers.length > 1;
  return (
    <div className="page-selector">
      {many && (
        <div className="pills" role="tablist" aria-label="Pages">
          {config.layers.map((l) => (
            <button key={l.id} role="tab" aria-selected={l.id === page} className={`pill${l.id === page ? " is-active" : ""}`} onClick={() => onPage(l.id)}>
              {l.name}
            </button>
          ))}
        </div>
      )}
      {many && <input className="input pill-rename" aria-label="Page name" value={current?.name ?? ""} onChange={(e) => onRename(e.target.value)} />}
      <button className="btn-ghost" onClick={onAdd}>
        Add page
      </button>
      {many && (
        <button className="btn-ghost" onClick={onDelete}>
          Delete page
        </button>
      )}
      {error && <p role="alert">{error}</p>}
    </div>
  );
}

/** One row per Speed Editor key with what it does on the page. */
export function ButtonList({ config, page, selected, onSelect }) {
  const bindings = config.layers.find((l) => l.id === page)?.bindings ?? {};
  return (
    <table className="button-list">
      <tbody>
        {KEYS.map((key) => (
          <tr key={key} aria-selected={key === selected} onClick={() => onSelect(key)}>
            <td>{LABELS[key]}</td>
            <td>{describeAction(bindings[key], config)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

const norm = (s) => s.replace(/[^A-Z0-9]/gi, "").toUpperCase();
const BY_PRINTED = Object.fromEntries(KEYS.map((k) => [norm(LABELS[k]), k]));
const xml = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const WHEEL_KEY = { Jog: "Jog", Shuttle: "Shtl", Scroll: "Scrl" };

/**
 * The handoff's speed-editor.svg with each key's short label and its state
 * classes, as the handoff README names them: is-selected, is-pressed, is-lit.
 */
export function drawingMarkup({ labels = {}, selected = null, pressed = null, lit = [] }) {
  let key = null;
  return drawing.replace(/<g id="(key-[A-Z0-9_]+)" data-key="([^"]+)"([^>]*) class="key">|<text class="key-label"([^>]*)><\/text>/g, (m, id, printed, rest, attrs) => {
    if (id === undefined) return `<text class="key-label"${attrs}>${xml(labels[key] ?? "Not set")}</text>`;
    key = BY_PRINTED[norm(printed)];
    const cls = ["key", key === selected && "is-selected", key === pressed && "is-pressed", lit.includes(key) && "is-lit", !labels[key] && "is-unset"];
    const title = xml(labels[key] ?? "Not set").replace(/"/g, "&quot;");
    return `<g id="${id}" data-key="${printed}"${rest} data-binding="${key}" title="${title}" class="${cls.filter(Boolean).join(" ")}">`;
  });
}

/** The Speed Editor drawn as it looks; click a key to change it. */
export function DeviceDrawing({ config, page, selected, pressed = null, onSelect }) {
  const bindings = config.layers.find((l) => l.id === page)?.bindings ?? {};
  const labels = Object.fromEntries(KEYS.filter((k) => bindings[k]).map((k) => [k, describeAction(bindings[k], config)]));
  const lit = [WHEEL_KEY[config.jog?.default_mode]].filter(Boolean);
  const click = (e) => {
    const key = e.target.closest?.("[data-binding]")?.dataset.binding;
    if (key) onSelect(key);
  };
  return <div className="device" onClick={click} dangerouslySetInnerHTML={{ __html: drawingMarkup({ labels, selected, pressed, lit }) }} />;
}

/** The Keys tab: page pills, the drawing or list of keys, and the key panel of the selected key. */
export default function KeysTab({ initial = null, initialSelected = null, initialView = "Drawing", initialChip = null, save = saveConfig }) {
  const [config, setConfig] = useState(initial);
  const [page, setPage] = useState(initial?.base_layer ?? null);
  const [selected, setSelected] = useState(initialSelected);
  const [view, setView] = useState(initialView);
  const [chip, setChip] = useState(initialChip);
  const [pageError, setPageError] = useState(null);
  const timer = useRef(null);
  useEffect(() => {
    if (initial || !globalThis.window?.__TAURI__) return undefined;
    getConfig().then(
      (c) => (setConfig(c), setPage(c.base_layer)),
      (e) => console.error("freewheel: loading the config failed:", String(e)),
    );
    const unlisten = onKeyPressed((p) => handlePress(p, { select: setSelected, setChip, timer }));
    return () => unlisten.then((stop) => stop(), () => {});
  }, [initial]);
  if (!config) return null;

  const commit = (next) =>
    save(next).then(
      () => (setConfig(next), setPageError(null)),
      (e) => setPageError(saveErrorText(e)),
    );
  const onDelete = () => {
    const r = deletePage(config, page);
    if (r.error) return setPageError(r.error);
    return commit(r.config).then(() => setPage(config.base_layer));
  };
  const onAdd = () => {
    const r = addPage(config);
    return commit(r.config).then(() => setPage(r.id));
  };
  return (
    <section className="keys-tab">
      <div className="keys-main">
        <div className="keys-bar">
          <PageSelector
            config={config}
            page={page}
            onPage={(p) => (setPage(p), setPageError(null))}
            onAdd={onAdd}
            onRename={(name) => commit(renamePage(config, page, name))}
            onDelete={onDelete}
            error={pageError}
          />
          <div className="segmented" role="group" aria-label="View">
            {["Drawing", "List"].map((v) => (
              <button key={v} aria-pressed={v === view} onClick={() => setView(v)}>
                {v}
              </button>
            ))}
          </div>
        </div>
        {chip && (
          <span className="press-chip" role="status">
            {chip}
          </span>
        )}
        {view === "Drawing" ? (
          <DeviceDrawing config={config} page={page} selected={selected} pressed={chip ? selected : null} onSelect={setSelected} />
        ) : (
          <ButtonList config={config} page={page} selected={selected} onSelect={setSelected} />
        )}
      </div>
      <aside className="side-panel">
        <ActionEditor
          key={`${page}-${selected}`}
          keyName={selected}
          page={page}
          initialConfig={config}
          save={save}
          onSaved={setConfig}
          onClose={() => setSelected(null)}
        />
      </aside>
    </section>
  );
}
