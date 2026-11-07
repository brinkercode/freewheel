// What's new after an update (src/whatsNew.js decides when): a notice under
// the toolbar with a 'What's new in vX.Y.Z' button, and the notes page it
// opens, which links to the version's GitHub release.
import { noteSections } from "./whatsNew.js";

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
export function WhatsNewPage({ news, onDone, openRelease }) {
  const sections = noteSections(news.notes);
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
        <section className="card card-stack whats-new-notes">
          {sections.length === 0 && <div className="card-text">The notes for this version are on GitHub.</div>}
          {sections.map((section, i) => (
            <div key={i}>
              {section.title && <div className="card-title">{section.title}</div>}
              <ul className="card-text">
                {section.items.map((item, j) => (
                  <li key={j}>{item}</li>
                ))}
              </ul>
            </div>
          ))}
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
