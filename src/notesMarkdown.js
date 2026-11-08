// What's new's release notes: the safe markdown subset the notes use
// (scripts/release-notes.mjs writes them), parsed into plain data that
// src/WhatsNew.jsx draws with React elements only, never as HTML:
//   ## / ### headings, * or - bullets (wrapped lines joined), paragraphs,
//   **bold**, `code` and [links](https://github.com/...).
// Raw HTML stays literal text; HTML comments are dropped; a link to
// anywhere but GitHub keeps only its text. The release's version heading
// ("## [0.2.0](compare link) (date)") is left out: the page's title says it.

/** Links open in the browser only to GitHub (the opener's scope, capabilities/settings.json). */
export const LINK_PREFIX = "https://github.com/";

/** `url` when What's new may open it, else null. */
export function safeHref(url) {
  const u = String(url ?? "").trim();
  if (!u.startsWith(LINK_PREFIX) || /[\s"'<>\\]/.test(u)) return null;
  try {
    return new URL(u).origin === "https://github.com" ? u : null;
  } catch {
    return null;
  }
}

/**
 * Inline markdown -> nodes: { text } | { code } | { bold: nodes } |
 * { link: text, href } (href null: shown as text only).
 */
export function parseInline(text, { bold = true } = {}) {
  const nodes = [];
  const push = (t) => {
    if (!t) return;
    const last = nodes[nodes.length - 1];
    if (last && "text" in last) last.text += t;
    else nodes.push({ text: t });
  };
  let s = String(text ?? "");
  const pattern = /`([^`]+)`|\[([^\]]*)\]\(([^)\s]*)\)|\*\*(.+?)\*\*/;
  for (;;) {
    const m = s.match(pattern);
    if (!m) break;
    if (!bold && m[4] !== undefined) {
      // Inside bold: '**' is literal.
      push(s.slice(0, m.index + 2));
      s = s.slice(m.index + 2);
      continue;
    }
    push(s.slice(0, m.index));
    if (m[1] !== undefined) nodes.push({ code: m[1] });
    else if (m[2] !== undefined) nodes.push({ link: m[2], href: safeHref(m[3]) });
    else nodes.push({ bold: parseInline(m[4], { bold: false }) });
    s = s.slice(m.index + m[0].length);
  }
  push(s);
  return nodes;
}

/** The text a node list reads as (what the page shows). */
export function inlineText(nodes) {
  return nodes.map((n) => n.text ?? n.code ?? n.link ?? inlineText(n.bold)).join("");
}

/**
 * The text with every HTML comment cut out, scanning left to right, so no
 * "<!--" can survive (an unclosed one drops the rest, as a browser would).
 * The notes are drawn as text, never as HTML; this keeps comments out of view.
 */
export function withoutComments(text) {
  // Cutting can join pieces into a new "<!--" (as in "<!<!---->--"), so cut
  // again until nothing changes; each pass only shortens the text.
  for (;;) {
    const cut = cutCommentsOnce(text);
    if (cut === text) return cut;
    text = cut;
  }
}

function cutCommentsOnce(text) {
  let out = "";
  let at = 0;
  for (;;) {
    const open = text.indexOf("<!--", at);
    if (open < 0) return out + text.slice(at);
    out += text.slice(at, open);
    const close = text.indexOf("-->", open + 4);
    if (close < 0) return out;
    at = close + 3;
  }
}

const VERSION_HEADING = /^#{1,2}\s+\[?v?\d+\.\d+\.\d+/;

/**
 * The notes as blocks: { type: "h2" | "h3", inline } | { type: "list",
 * items: [inline] } | { type: "p", inline }.
 */
export function parseNotes(markdown) {
  const blocks = [];
  let list = null;
  let para = null;
  let item = null;
  const src = withoutComments(String(markdown ?? "").replace(/\r\n?/g, "\n"));
  const flush = () => {
    if (para) blocks.push({ type: "p", inline: parseInline(para.join(" ")) });
    para = null;
  };
  const closeList = () => {
    if (list) blocks.push({ type: "list", items: list.map((t) => parseInline(t)) });
    list = null;
    item = null;
  };
  for (const raw of src.split("\n")) {
    const line = raw.replace(/\s+$/, "");
    const trimmed = line.trim();
    if (!trimmed) {
      flush();
      item = null;
      continue;
    }
    const heading = trimmed.match(/^(#{1,6})\s+(.*)$/);
    if (heading) {
      flush();
      closeList();
      if (VERSION_HEADING.test(trimmed)) continue;
      blocks.push({ type: heading[1].length <= 2 ? "h2" : "h3", inline: parseInline(heading[2].replace(/\s+#+$/, "")) });
      continue;
    }
    const bullet = line.match(/^\s*[*-]\s+(.*)$/);
    if (bullet) {
      flush();
      list ??= [];
      list.push(bullet[1].trim());
      item = list.length - 1;
      continue;
    }
    if (item !== null && /^\s/.test(line)) {
      // A wrapped bullet.
      list[item] += ` ${trimmed}`;
      continue;
    }
    closeList();
    para ??= [];
    para.push(trimmed);
  }
  flush();
  closeList();
  return blocks;
}

/** Every line of text the page shows for `markdown`, in order (headings, items, paragraphs). */
export function notesText(markdown) {
  return parseNotes(markdown).flatMap((b) => (b.type === "list" ? b.items.map(inlineText) : [inlineText(b.inline)]));
}
