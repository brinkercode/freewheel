// What's new after an update (src/whatsNew.js decides when): a notice under
// the toolbar with a 'What's new in vX.Y.Z' button, and the notes page it
// opens, which links to the version's GitHub release. The notes are the
// safe markdown subset of src/notesMarkdown.js, drawn as React elements
// (never as HTML); their links open in the browser through the opener.
import { parseNotes } from "./notesMarkdown.js";

/** Opens a notes link (https://github.com/... only, notesMarkdown.js safeHref) in the default browser. */
export const openNotesLink = (url) => globalThis.window.__TAURI__.core.invoke("plugin:opener|open_url", { url });

/** Inline nodes (notesMarkdown.js parseInline) as elements. */
function Inline({ nodes, openLink }) {
  return nodes.map((n, i) => {
    if (n.text !== undefined) return n.text;
    if (n.code !== undefined) return <code key={i} className="whats-new-code">{n.code}</code>;
    if (n.bold) return <strong key={i}><Inline nodes={n.bold} openLink={openLink} /></strong>;
    if (!n.href) return n.link;
    const click = (event) => {
      if (!openLink) return;
      event.preventDefault();
      openLink(n.href).catch((e) => console.error("freewheel: opening a notes link failed:", String(e)));
    };
    return (
      <a key={i} href={n.href} target="_blank" rel="noreferrer" onClick={click}>
        {n.link}
      </a>
    );
  });
}

/** Release notes markdown as the notes card's content. */
export function NotesBody({ markdown, openLink }) {
  const blocks = parseNotes(markdown);
  if (!blocks.length) return <div className="card-text">The notes for this version are on GitHub.</div>;
  return blocks.map((b, i) => {
    if (b.type === "h2")
      return (
        <h3 key={i} className="card-title whats-new-h2">
          <Inline nodes={b.inline} openLink={openLink} />
        </h3>
      );
    if (b.type === "h3")
      return (
        <h4 key={i} className="card-title">
          <Inline nodes={b.inline} openLink={openLink} />
        </h4>
      );
    if (b.type === "p")
      return (
        <p key={i} className="card-text">
          <Inline nodes={b.inline} openLink={openLink} />
        </p>
      );
    return (
      <ul key={i} className="card-text">
        {b.items.map((item, j) => (
          <li key={j}>
            <Inline nodes={item} openLink={openLink} />
          </li>
        ))}
      </ul>
    );
  });
}

/** The once-per-update notice; renders nothing without news. */
export function WhatsNewNotice({ news, onOpen, onDismiss }) {
  if (!news) return null;
  return (
    <div role="status" className="update-banner">
      <span className="logo-disc" aria-hidden="true" />
      <div className="update-text">
        <span className="update-title">freewheel is updated to {news.version}</span>
      </div>
      <button className="btn-sm btn-primary" onClick={onOpen}>
        What’s new in v{news.version}
      </button>
      <button className="btn-sm btn-ghost" onClick={onDismiss}>
        Dismiss
      </button>
    </div>
  );
}

/** The release notes of `news.version`, in place of the tab's page. */
export function WhatsNewPage({ news, onDone, openRelease, openLink = openNotesLink }) {
  const open = (event) => {
    if (!openRelease) return;
    event.preventDefault();
    openRelease(news.version).catch((e) => {
      console.error("freewheel: opening the release page failed:", String(e));
      globalThis.open?.(news.url, "_blank", "noopener");
    });
  };
  return (
    <div className="tab-page" data-screen-label="What's new">
      <div className="tab-column">
        <h2 className="tab-title">What’s new in v{news.version}</h2>
        <section className="card card-stack whats-new-notes" aria-label="Release notes">
          <NotesBody markdown={news.notes} openLink={globalThis.window?.__TAURI__ ? openLink : null} />
        </section>
        <div className="row">
          <a className="grow" href={news.url} target="_blank" rel="noreferrer" onClick={open}>
            See this release on GitHub
          </a>
          <button className="btn-primary" onClick={onDone}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
