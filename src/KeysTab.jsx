import { useEffect, useRef, useState } from "react";
import ActionEditor, { KeyPanel } from "./ActionEditor.jsx";
import Icon from "./Icon.jsx";
import drawing from "./assets/speed-editor.svg?raw";
import { DRAWING, KEYS, LABELS, PRESS_CHIP_MS, addPage, backTarget, deletePage, longLabel, renamePage, shortLabel } from "./keys.js";
import { buttonDoes, liveButton } from "./WheelTab.jsx";
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

/**
 * Page pills (shown once a 2nd page exists): the active pill renames inline
 * (double-click or its pencil) and deletes after an inline confirm.
 */
export function PageSelector({ config, page, livePage = null, onPage, onAdd, onRename, onDelete, error }) {
  const [renaming, setRenaming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const rename = useRef(null);
  useEffect(() => {
    if (renaming) rename.current?.select();
  }, [renaming]);
  const current = config.layers.find((l) => l.id === page);
  const many = config.layers.length > 1;
  if (deleting)
    return (
      <div className="page-selector">
        <div className="confirm-inline">
          <span>{`Delete “${current?.name}”? Its keys will be cleared.`}</span>
          <button className="btn-sm btn-danger" onClick={() => (setDeleting(false), onDelete())}>
            Delete
          </button>
          <button className="btn-sm" onClick={() => setDeleting(false)}>
            Keep it
          </button>
        </div>
      </div>
    );
  const live = (id) => id === livePage && <span className="live-dot" aria-label="Your Speed Editor is on this page" />;
  return (
    <div className="page-selector">
      {!many && <span className="keys-title">What each key does</span>}
      {many && (
        <div className="pills" role="tablist" aria-label="Pages">
          {config.layers.map((l, i) =>
            l.id === page ? (
              <div key={l.id} className={`pill is-active${renaming ? " is-renaming" : ""}`}>
                <button role="tab" aria-selected="true" title="Double-click to rename" onDoubleClick={() => setRenaming(true)}>
                  {live(l.id)}
                  {l.name}
                </button>
                <input
                  ref={rename}
                  className="pill-rename"
                  aria-label="Page name"
                  value={l.name}
                  onChange={(e) => onRename(e.target.value)}
                  onBlur={() => setRenaming(false)}
                  onKeyDown={(e) => (e.key === "Enter" || e.key === "Escape") && setRenaming(false)}
                />
                <button className="pill-icon" title="Rename" aria-label="Rename" onClick={() => setRenaming(true)}>
                  <Icon name="edit" />
                </button>
                {i > 0 && (
                  <button className="pill-icon" title="Delete page" onClick={() => setDeleting(true)}>
                    <Icon name="close" />
                    Delete page
                  </button>
                )}
              </div>
            ) : (
              <div key={l.id} className="pill">
                <button role="tab" aria-selected="false" title="Show this page" onClick={() => onPage(l.id)}>
                  {live(l.id)}
                  {l.name}
                </button>
              </div>
            ),
          )}
        </div>
      )}
      <button className="pill-add" onClick={onAdd}>
        <Icon name="add" />
        Add page
      </button>
      {many && livePage && (
        <span className="live-legend">
          <span className="live-legend-dot" />
          Your Speed Editor is here
        </span>
      )}
      {error && <p role="alert">{error}</p>}
    </div>
  );
}

/** The List view's three cards, as the prototype's listGroups: the drawing's three blocks of keys. */
export const LIST_GROUPS = [
  ["Left side", DRAWING[0].flat()],
  ["Middle", DRAWING[1].flat()],
  ["Right side, above the wheel", DRAWING[2].flat()],
];

/** One row per Speed Editor key with what it does on the page, in three titled cards. */
export function ButtonList({ config, page, selected, onSelect }) {
  const bindings = config.layers.find((l) => l.id === page)?.bindings ?? {};
  const does = (key) => {
    if (WHEEL_KEYS.includes(key)) return `Wheel button · ${buttonDoes(config, key)}`;
    const back = backTarget(config, page, key);
    return back ? `Back to ${back.name}` : longLabel(bindings[key], config);
  };
  const isSet = (key) => bindings[key] || WHEEL_KEYS.includes(key) || backTarget(config, page, key);
  return (
    <div className="list-groups">
      {LIST_GROUPS.map(([title, keys]) => (
        <div key={title} className="list-group">
          <div className="list-group-title">{title}</div>
          <table className="button-list">
            <tbody>
              {keys.map((key) => (
                <tr key={key} aria-selected={key === selected} className={isSet(key) ? undefined : "is-unset"} onClick={() => onSelect(key)}>
                  <td>{LABELS[key]}</td>
                  <td>{does(key)}</td>
                  <td className="row-chevron">
                    <Icon name="chevron_right" />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ))}
    </div>
  );
}

const norm = (s) => s.replace(/[^A-Z0-9]/gi, "").toUpperCase();
const BY_PRINTED = Object.fromEntries(KEYS.map((k) => [norm(LABELS[k]), k]));
const xml = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const WHEEL_KEYS = ["Shtl", "Jog", "Scrl"];

// Text widths for the drawing's key names and labels: the real font when a
// canvas is there, else an estimate (tests).
const FONT = `"Segoe UI Variable Text","Segoe UI",system-ui,sans-serif`;
let ctx;
function textWidth(text, size, weight, spacing = 0) {
  if (ctx === undefined) {
    try {
      // jsdom has no canvas: estimate there.
      ctx = /jsdom/.test(globalThis.navigator?.userAgent ?? "") ? null : (globalThis.document?.createElement("canvas").getContext("2d") ?? null);
    } catch {
      ctx = null;
    }
  }
  if (!ctx) return text.length * size * 0.6 + spacing * text.length;
  ctx.font = `${weight} ${size}px ${FONT}`;
  return ctx.measureText(text).width + spacing * text.length;
}

/** `text` in lines that fit `width` (CSS word wrap, long words broken), at most two; the 2nd ends in "…" when cut. */
export function wrapLines(text, width, measure) {
  const lines = [];
  let line = "";
  for (const word of text.split(" ")) {
    const next = line ? `${line} ${word}` : word;
    if (measure(next) <= width) {
      line = next;
      continue;
    }
    if (line) lines.push(line);
    line = "";
    for (const ch of word) {
      if (line && measure(line + ch) > width) {
        lines.push(line);
        line = "";
      }
      line += ch;
    }
  }
  if (line) lines.push(line);
  if (lines.length <= 2) return lines;
  let last = lines[1];
  while (last.length > 1 && measure(`${last}…`) > width) last = last.slice(0, -1);
  return [lines[0], `${last}…`];
}

/** A key's label in at most two lines that fit a key `width` wide (the prototype clamps to 2 lines). */
export const wrapLabel = (text, width, fontSize = 9.5) => wrapLines(text, width - 8, (t) => textWidth(t, fontSize, 600));

const tspans = (lines, x, top, size) =>
  lines
    .map((l, i) => `<tspan x="${x}" y="${(top + i * size * 1.15 + size * 1.15 * 0.5 + size * 0.36).toFixed(2)}">${xml(l)}</tspan>`)
    .join("");

/**
 * One key of the drawing laid out as the prototype's key button: the printed
 * name over the label, both wrapped to two lines, centred in the face below
 * the LED (padding 9px 4px 4px, a 2px gap).
 */
function layoutKey(block, label) {
  const [, fx, fy, fw, fh] = /<rect class="key-face" x="([\d.]+)" y="([\d.]+)" width="([\d.]+)" height="([\d.]+)"/.exec(block).map(Number);
  const cx = /<text class="key-label" x="([\d.]+)"/.exec(block)[1];
  const size = fw > 104 ? 11 : 9.5;
  const name = / data-key="([^"]+)"/.exec(block)[1];
  const nameLines = wrapLines(name, fw - 8, (t) => textWidth(t, 7, 700, 0.21));
  const labelLines = wrapLabel(label, fw, size);
  const nameH = nameLines.length * 7 * 1.15;
  const labelH = labelLines.length * size * 1.15;
  const top = fy + 9 + (fh - 13 - (nameH + 2 + labelH)) / 2;
  return block
    .replace(/(<text class="key-name"[^>]*?) y="[\d.]+"([^>]*>)[\s\S]*?<\/text>/, (m, open, rest) => `${open}${rest}${tspans(nameLines, cx, top, 7)}</text>`)
    .replace(
      /(<text class="key-label"[^>]*?) y="[\d.]+"([^>]*?) font-size="[\d.]+"([^>]*>)<\/text>/,
      (m, open, mid, rest) => `${open}${mid} font-size="${size}"${rest}${tspans(labelLines, cx, top + nameH + 2, size)}</text>`,
    );
}

/**
 * The handoff's speed-editor.svg with each key's short label and its state
 * classes, as the handoff README names them: is-selected, is-pressed, is-lit;
 * the wheel shows the lit wheel button and what it does.
 */
export function drawingMarkup({ labels = {}, selected = null, pressed = null, lit = [], wheel = null }) {
  const block = /<g id="(key-[A-Z0-9_]+)" data-key="([^"]+)"([^>]*) class="key">([\s\S]*?)<\/g>|<text class="wheel-(button|label)"([^>]*)>[^<]*<\/text>/g;
  return drawing.replace(block, (m, id, printed, rest, inner, wheelPart, wheelAttrs) => {
    if (wheelPart) {
      if (!wheel) return m;
      return `<text class="wheel-${wheelPart}"${wheelAttrs}>${xml(wheelPart === "button" ? wheel.button : wheel.does)}</text>`;
    }
    const key = BY_PRINTED[norm(printed)];
    const label = labels[key] ?? "Not set";
    let body = layoutKey(m, label).replace(/^<g[^>]*>/, "").replace(/<\/g>$/, "");
    // selected: a 2px accent ring around a 2px panel-coloured gap, drawn behind the face
    if (key === selected)
      body = body.replace(
        /<rect class="key-face" x="([\d.]+)" y="([\d.]+)" width="([\d.]+)" height="([\d.]+)"/,
        (f, x, y, w, h) => `<rect class="key-ring" x="${x - 3}" y="${y - 3}" width="${+w + 6}" height="${+h + 6}" rx="8"></rect>${f}`,
      );
    const cls = ["key", key === selected && "is-selected", key === pressed && "is-pressed", lit.includes(key) && "is-lit", !labels[key] && "is-unset"];
    const title = xml(label).replace(/"/g, "&quot;");
    return `<g id="${id}" data-key="${printed}"${rest} data-binding="${key}" title="${title}" class="${cls.filter(Boolean).join(" ")}">${body}</g>`;
  });
}

/** Each key's short label on `page`: its action, the way back, or what a wheel button does. */
export function drawingLabels(config, page) {
  const bindings = config.layers.find((l) => l.id === page)?.bindings ?? {};
  return Object.fromEntries(
    KEYS.map((k) => {
      if (WHEEL_KEYS.includes(k)) return [k, buttonDoes(config, k)];
      const back = backTarget(config, page, k);
      if (back) return [k, `← Back to ${back.name}`];
      return [k, bindings[k] ? shortLabel(bindings[k], config) : null];
    }).filter(([, v]) => v),
  );
}

/** The Speed Editor drawn as it looks; click a key to change it, the wheel to set it up. */
export function DeviceDrawing({ config, page, selected, pressed = null, dim = false, lit: deviceLit = null, onSelect, onWheel }) {
  const labels = drawingLabels(config, page);
  // While connected the drawing lights what the device lights (status.lit:
  // page key, latched keys, override key, wheel button); otherwise the wheel
  // button the config starts in.
  const lit = deviceLit ?? [liveButton(config)].filter(Boolean);
  const live = WHEEL_KEYS.find((k) => lit.includes(k)) ?? liveButton(config);
  const wheel = { button: LABELS[live] ?? "JOG", does: live ? buttonDoes(config, live) : "Scrolls" };
  const click = (e) => {
    const key = e.target.closest?.("[data-binding]")?.dataset.binding;
    if (key) onSelect(key);
    else if (e.target.closest?.("#wheel")) onWheel?.();
  };
  return (
    <div
      className={`device${dim ? " is-dim" : ""}`}
      onClick={click}
      dangerouslySetInnerHTML={{ __html: drawingMarkup({ labels, selected, pressed, lit, wheel }) }}
    />
  );
}

/** The Keys tab: page pills, the drawing or list of keys, and the key panel of the selected key. */
export default function KeysTab({
  initial = null,
  initialSelected = null,
  initialView = "Drawing",
  initialChip = null,
  save = saveConfig,
  status = null,
  onWheel,
  onToast,
  record = null,
}) {
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
  // The Simulator's "Record a key" (--simulate only): that key's panel, recording.
  const [rec, setRec] = useState(null);
  useEffect(() => {
    if (record) (setSelected(record.key), setRec(record));
  }, [record]);
  const pickKey = (k) => (setRec(null), setSelected(k));
  // A page key on the Speed Editor switches its page: the Keys tab follows.
  const deviceLayer = useRef(null);
  useEffect(() => {
    const layer = status?.layer;
    if (!config || !layer || layer === deviceLayer.current) return;
    deviceLayer.current = layer;
    if (config.layers.some((l) => l.id === layer)) (setPage(layer), setSelected(null), setPageError(null));
  }, [status?.layer, config]);
  if (!config) return null;

  // Shown at once (a page name follows each keystroke), undone if the save fails.
  const commit = (next) => {
    const before = config;
    setConfig(next);
    return save(next).then(
      () => setPageError(null),
      (e) => (setConfig(before), setPageError(saveErrorText(e))),
    );
  };
  const onDelete = () => {
    const r = deletePage(config, page);
    if (r.error) return setPageError(r.error);
    return commit(r.config).then(() => setPage(config.base_layer));
  };
  const onAdd = () => {
    const r = addPage(config);
    return commit(r.config).then(() => {
      setPage(r.id);
      setSelected(null);
      onToast?.("To reach this page, pick a key on another page and choose “Switch to another set of keys”.");
    });
  };
  // The drawing dims only while no Speed Editor is found (Off and Paused still have it).
  const found = status ? status.connection !== "Absent" || status.paused : true;
  const livePage = status && found ? (config.layers.some((l) => l.id === status.layer) ? status.layer : config.base_layer) : null;
  const back = selected ? backTarget(config, page, selected) : null;
  return (
    <section className="keys-tab">
      <div className="keys-main" data-screen-label="What each key does">
        <div className="keys-bar">
          <PageSelector
            config={config}
            page={page}
            livePage={livePage}
            onPage={(p) => (setPage(p), setSelected(null), setPageError(null))}
            onAdd={onAdd}
            onRename={(name) => commit(renamePage(config, page, name))}
            onDelete={onDelete}
            error={pageError}
          />
          <div className="segmented" role="group" aria-label="View">
            {["Drawing", "List"].map((v) => (
              <button key={v} data-view={v} aria-pressed={v === view} onClick={() => setView(v)}>
                {v}
              </button>
            ))}
          </div>
        </div>
        {view === "Drawing" ? (
          <div className="board">
            <div className="press-row">
              {chip ? (
                <span className="press-chip" role="status">
                  {chip}
                </span>
              ) : (
                <span className="keys-hint">Click a key to choose what it does — or just press it on your Speed Editor.</span>
              )}
            </div>
            <DeviceDrawing
              config={config}
              page={page}
              selected={selected}
              pressed={chip ? selected : null}
              dim={!found}
              lit={status?.connection === "Connected" ? (status.lit ?? null) : null}
              onSelect={pickKey}
              onWheel={onWheel}
            />
          </div>
        ) : (
          <div className="list-scroll">
            <ButtonList config={config} page={page} selected={selected} onSelect={pickKey} />
          </div>
        )}
      </div>
      <aside className="side-panel" data-screen-label="What should this key do">
        <ActionEditor
          key={`${page}-${selected}-${rec?.key === selected ? rec.n : 0}`}
          keyName={selected}
          recording={rec?.key === selected}
          page={page}
          initialConfig={config}
          save={save}
          back={back}
          onSaved={setConfig}
          onClose={() => setSelected(null)}
          onWheel={onWheel}
          onShowPage={(id) => (setPage(id), setSelected(null))}
        />
      </aside>
    </section>
  );
}
