/** The update banner for each updater state; renders nothing without an update. */
export default function UpdateBanner({ update, onInstall, onLater, onRestart }) {
  if (!update) return null;
  const { status, version, progress } = update;
  return (
    <div role="status" className="update-banner">
      {status === "available" && (
        <>
          <span>freewheel {version} is available.</span>
          <button onClick={onInstall}>Download and install</button>
          <button onClick={onLater}>Later</button>
        </>
      )}
      {status === "downloading" && (
        <>
          <span>Downloading freewheel {version}…</span>
          <progress max="100" value={progress ?? undefined} />
        </>
      )}
      {status === "ready" && (
        <>
          <span>freewheel {version} is ready.</span>
          <button onClick={onRestart}>Restart to update</button>
          <button onClick={onLater}>Later</button>
        </>
      )}
      {status === "restarting" && <span>Restarting…</span>}
    </div>
  );
}
