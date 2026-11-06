import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { useEffect, useRef, useState } from "react";
import UpdateBanner from "./UpdateBanner.jsx";
import { download, restart, startupCheck } from "./updater.js";
import Simulator from "./Simulator.jsx";
import Backup from "./Backup.jsx";
import Permissions from "./Permissions.jsx";
import Status from "./Status.jsx";
import KeysTab from "./KeysTab.jsx";
import { NoticeBar, Toast, Toolbar } from "./SettingsWindow.jsx";
import { getStatus, onStatus, setEnabled } from "./settingsApi.js";
import { TABS, fromStatusEvent, showToast, switchOn } from "./shell.js";

export default function App({ autoUpdate = true }) {
  // The Simulator window exists only in `freewheel --simulate` (main.rs).
  if (globalThis.location?.hash === "#simulator") return <Simulator />;
  return <Settings autoUpdate={autoUpdate} />;
}

function Settings({ autoUpdate }) {
  const [update, setUpdate] = useState(null);
  const [tab, setTab] = useState(TABS[0]);
  const [status, setStatus] = useState(null);
  const pending = useRef(null);
  useEffect(() => {
    startupCheck({ check, autoUpdate, onState: setUpdate }).then((u) => (pending.current = u));
  }, [autoUpdate]);
  useEffect(() => {
    if (!globalThis.window?.__TAURI__) return undefined;
    getStatus().then(setStatus, () => {});
    const unlisten = onStatus(setStatus);
    return () => unlisten.then((stop) => stop(), () => {});
  }, []);
  const state = fromStatusEvent(status ?? { connection: "Absent" });
  const [toast, setToast] = useState(null);
  // A failed switch is an error that needs action: it is logged, never toasted.
  const turn = (on) =>
    switchOn(setEnabled, on).then(
      (text) => showToast(setToast, text),
      (error) => console.error("freewheel: turning", on ? "on" : "off", "failed:", String(error)),
    );
  return (
    <>
      <Toolbar tab={tab} onTab={setTab} state={state} onToggle={status ? () => turn(!state.enabled) : undefined} />
      <UpdateBanner
        update={update}
        onInstall={() => download(pending.current, setUpdate)}
        onLater={() => setUpdate(null)}
        onRestart={() => restart(pending.current, relaunch, setUpdate)}
      />
      {status && <NoticeBar state={state} onFix={() => document.getElementById("permissions")?.scrollIntoView()} onTurnOn={() => turn(true)} />}
      <main>
        <h1>freewheel settings</h1>
        <Status />
        {tab === "Keys" && <KeysTab />}
        <Permissions />
        {tab === "Help & backup" && <Backup onToast={(text) => showToast(setToast, text)} />}
      </main>
      <Toast text={toast} />
    </>
  );
}
