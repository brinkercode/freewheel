/** The update banner for each updater state; renders nothing without an update. */
export default function UpdateBanner({ update, onInstall, onLater, onRestart }) {
  // Checking and up to date show only on the General tab.
  if (!update || update.status === "checking" || update.status === "current") return null;
  const { status, progress } = update;
  const busy = status === "downloading" || status === "restarting";
  return (
    <div role="status" className="update-banner">
      <span className={`logo-disc${busy ? " is-spinning" : ""}`} aria-hidden="true" />
      <div className="update-text">
        {status === "available" && <span className="update-title">A new version is available</span>}
        {status === "downloading" && (
          <>
            <span className="update-title">Downloading the new version…</span>
            <progress max="100" value={progress ?? undefined} />
          </>
        )}
        {status === "ready" && <span className="update-title">A new version is ready</span>}
        {status === "restarting" && <span className="update-title">Restarting freewheel…</span>}
      </div>
      {status === "available" && (
        <button className="btn-sm btn-primary" onClick={onInstall}>
          Download and install
        </button>
      )}
      {status === "ready" && (
        <button className="btn-sm btn-primary" onClick={onRestart}>
          Restart to update
        </button>
      )}
      {(status === "available" || status === "ready") && (
        <button className="btn-sm btn-ghost" onClick={onLater}>
          Later
        </button>
      )}
    </div>
  );
}
