import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { useEffect, useRef, useState } from "react";
import UpdateBanner from "./UpdateBanner.jsx";
import { download, restart, startupCheck } from "./updater.js";

export default function App({ autoUpdate = true }) {
  const [update, setUpdate] = useState(null);
  const pending = useRef(null);
  useEffect(() => {
    startupCheck({ check, autoUpdate, onState: setUpdate }).then((u) => (pending.current = u));
  }, [autoUpdate]);
  return (
    <>
      <UpdateBanner
        update={update}
        onInstall={() => download(pending.current, setUpdate)}
        onLater={() => setUpdate(null)}
        onRestart={() => restart(pending.current, relaunch, setUpdate)}
      />
      <h1>freewheel settings</h1>
    </>
  );
}
