import { useEffect, useRef, useState } from "react";
import ActionEditor from "./ActionEditor.jsx";
import {
  COMBO_KINDS,
  DRAWING,
  KEYS,
  LABELS,
  PRESS_CHIP_MS,
  addPage,
  comboError,
  comboFromEvent,
  deletePage,
  describeAction,
  formatCombo,
  renamePage,
  setBinding,
} from "./keys.js";
import { getConfig, onKeyPressed, saveConfig, saveErrorText } from "./settingsApi.js";
import { showToast } from "./shell.js";

const KIND_TEXT = {
  Hotkey: ["Hotkey", "Sends the shortcut once."],
  Held: ["Held", "Holds the shortcut down while you hold this key."],
  Latched: ["Latched", "Press once to hold the shortcut down, press again to let go."],
};

/** A physical press: shows 'You pressed X' for PRESS_CHIP_MS and selects that key. */
export function handlePress(press, { select, setChip, timer, timers = globalThis }) {
  select(press.key);
  timers.clearTimeout(timer.current);
  timer.current = showToast(setChip, `You pressed ${press.label}`, timers, PRESS_CHIP_MS);
}

/**
 * Saves `kind` + `combo` on `key` of page `pageId` through `save` (save_config).
 * Resolves to { config } or { error } (the inline error); an invalid combo is never sent.
 */
export async function saveKey({ config, pageId, key, kind, combo, save = saveConfig }) {
  const error = comboError(combo);
  if (error) return { error };
  const next = setBinding(config, pageId, key, { [kind]: { combo } });
  try {
    await save(next);
    return { config: next };
  } catch (e) {
    console.error("freewheel: saving key", key, "failed:", saveErrorText(e));
    return { error: saveErrorText(e) };
  }
}

/** Pick, add, rename and delete pages. */
export function PageSelector({ config, page, onPage, onAdd, onRename, onDelete, error }) {
  const current = config.layers.find((l) => l.id === page);
  return (
    <div className="page-selector" style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
      <label>
        Page{" "}
        <select value={page} onChange={(e) => onPage(e.target.value)}>
          {config.layers.map((l) => (
            <option key={l.id} value={l.id}>
              {l.name}
            </option>
          ))}
        </select>
      </label>
      <input aria-label="Page name" value={current?.name ?? ""} onChange={(e) => onRename(e.target.value)} />
      <button onClick={onAdd}>Add page</button>
      <button onClick={onDelete}>Delete page</button>
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
          <tr key={key} aria-selected={key === selected} onClick={() => onSelect(key)} style={{ cursor: "pointer" }}>
            <td>{LABELS[key]}</td>
            <td>{describeAction(bindings[key], config)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** The Speed Editor drawn as it looks; click a key to change it. */
export function DeviceDrawing({ config, page, selected, onSelect }) {
  const bindings = config.layers.find((l) => l.id === page)?.bindings ?? {};
  return (
    <div className="device" style={{ display: "flex", gap: 24, padding: 16, borderRadius: 12, background: "#2c2c2e" }}>
      {DRAWING.map((block, b) => (
        <div key={b} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {block.map((row, r) => (
            <div key={r} style={{ display: "flex", gap: 6 }}>
              {row.map((key) => (
                <button
                  key={key}
                  aria-pressed={key === selected}
                  title={describeAction(bindings[key], config)}
                  onClick={() => onSelect(key)}
                  style={{ width: 72, minHeight: 40, fontSize: 10 }}
                >
                  {LABELS[key]}
                </button>
              ))}
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

const comboOf = (action) => (action && Object.values(action)[0]?.combo) || null;

/** What should this key do: Hotkey, Held or Latched with a recorded shortcut. */
export function KeyEditor({ keyName, action, onSave, onClose, initialError = null }) {
  const current = action && COMBO_KINDS.find((k) => k in action);
  const [kind, setKind] = useState(current ?? "Hotkey");
  const [combo, setCombo] = useState(comboOf(action));
  const [error, setError] = useState(initialError);
  const record = (e) => {
    if (e.key === "Tab") return;
    e.preventDefault();
    setCombo(comboFromEvent(e));
    setError(null);
  };
  const save = () => onSave(kind, combo).then((r) => setError(r.error ?? null));
  return (
    <section className="key-editor" aria-label={`${LABELS[keyName]} action`}>
      <h2>{`What should ${LABELS[keyName]} do?`}</h2>
      {COMBO_KINDS.map((k) => (
        <label key={k} style={{ display: "block" }}>
          <input type="radio" name="kind" checked={kind === k} onChange={() => setKind(k)} />
          <strong>{KIND_TEXT[k][0]}</strong> <span>{KIND_TEXT[k][1]}</span>
        </label>
      ))}
      <label>
        Shortcut{" "}
        <input
          className="combo-capture"
          readOnly
          placeholder="Click here, then press the shortcut"
          value={combo ? formatCombo(combo) : ""}
          onKeyDown={record}
          aria-invalid={Boolean(error)}
        />
      </label>
      {error && <p role="alert">{error}</p>}
      <button onClick={save}>Save</button>
      <button onClick={onClose}>Cancel</button>
    </section>
  );
}

/** The Keys tab: page selector, the drawing or list of keys, and the editor of the selected key. */
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
  const bindings = config.layers.find((l) => l.id === page)?.bindings ?? {};
  const Keys = view === "Drawing" ? DeviceDrawing : ButtonList;
  return (
    <section className="keys-tab">
      <PageSelector
        config={config}
        page={page}
        onPage={(p) => (setPage(p), setPageError(null))}
        onAdd={onAdd}
        onRename={(name) => commit(renamePage(config, page, name))}
        onDelete={onDelete}
        error={pageError}
      />
      <div role="group" aria-label="View">
        {["Drawing", "List"].map((v) => (
          <button key={v} aria-pressed={v === view} onClick={() => setView(v)}>
            {v}
          </button>
        ))}
      </div>
      {chip && (
        <span className="press-chip" role="status">
          {chip}
        </span>
      )}
      <Keys config={config} page={page} selected={selected} onSelect={setSelected} />
      {selected && (
        <KeyEditor
          key={`${page}-${selected}`}
          keyName={selected}
          action={bindings[selected]}
          onClose={() => setSelected(null)}
          onSave={(kind, combo) =>
            saveKey({ config, pageId: page, key: selected, kind, combo, save }).then((r) => {
              if (r.config) setConfig(r.config);
              return r;
            })
          }
        />
      )}
      <ActionEditor key={`${page}-${selected}`} keyName={selected} page={page} initialConfig={config} save={save} onSaved={setConfig} />
    </section>
  );
}
