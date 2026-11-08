/** The download bar, drawn as the prototype's: a 4px track, the accent fill; no percentage known runs indeterminate. */
export function UpdateProgress({ progress }) {
  const known = progress != null;
  return (
    <span
      className={`update-progress${known ? "" : " is-indeterminate"}`}
      role="progressbar"
      aria-label="Download progress"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuenow={known ? progress : undefined}
    >
      <span className="update-progress-fill" style={known ? { width: `${progress}%` } : undefined} />
    </span>
  );
}

const ERROR_TITLE = {
  download: "The new version didn’t download",
  install: "The new version didn’t install",
};

/** The failed step's title: why it failed, when the updater said. */
export function errorTitle(update) {
  if (update.notWritable) return "freewheel can’t update itself in this folder";
  if (update.badSignature) return "The new version couldn’t be verified";
  if (update.cantInstall) return "freewheel can’t update itself here";
  return ERROR_TITLE[update.during];
}

/** What to do about it. */
export function errorSub(update) {
  if (update.notWritable) return "Move the freewheel folder somewhere you can change, like Documents, or download the new version from GitHub.";
  if (update.badSignature) return "Nothing was changed. Try again.";
  if (update.cantInstall) return `${update.reason || "This copy of freewheel can’t replace itself"}: download the new version from GitHub.`;
  return "Check your internet connection and try again.";
}

/** The update banner for each updater state; renders nothing without an update. */
export default function UpdateBanner({ update, onInstall, onLater, onRestart }) {
  // Checking, up to date and a failed check show only on the General tab.
  if (!update || update.status === "checking" || update.status === "current") return null;
  if (update.status === "error" && !ERROR_TITLE[update.during]) return null;
  const { status, progress } = update;
  const busy = status === "downloading" || status === "restarting";
  return (
    <div role="status" className={`update-banner${status === "error" ? " is-error" : ""}`} data-update={status}>
      <span className={`logo-disc${busy ? " is-spinning" : ""}`} aria-hidden="true" />
      <div className="update-text">
        {status === "available" && <span className="update-title">A new version is available</span>}
        {status === "downloading" && (
          <>
            <span className="update-title">
              Downloading the new version…
              {progress != null && <span className="update-pct"> {progress}%</span>}
            </span>
            <UpdateProgress progress={progress} />
          </>
        )}
        {status === "ready" && <span className="update-title">A new version is ready</span>}
        {status === "restarting" && <span className="update-title">Restarting freewheel…</span>}
        {status === "error" && (
          <>
            <span className="update-title">{errorTitle(update)}</span>
            <span className="update-sub">{errorSub(update)}</span>
          </>
        )}
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
      {status === "error" && (
        <button className="btn-sm btn-primary" onClick={update.during === "install" ? onRestart : onInstall}>
          Try again
        </button>
      )}
      {(status === "available" || status === "ready" || status === "error") && (
        <button className="btn-sm btn-ghost" onClick={onLater}>
          Later
        </button>
      )}
    </div>
  );
}
