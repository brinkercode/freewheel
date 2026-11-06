import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { useEffect, useRef, useState } from "react";
import UpdateBanner from "./UpdateBanner.jsx";
import { download, restart, startupCheck } from "./updater.js";
import Simulator from "./Simulator.jsx";
import Backup from "./Backup.jsx";
import FirstRun from "./FirstRun.jsx";
import Permissions, { PermissionsScreen } from "./Permissions.jsx";
import Status from "./Status.jsx";
import KeysTab from "./KeysTab.jsx";
import ResolveTab from "./ResolveTab.jsx";
import WheelTab from "./WheelTab.jsx";
import { NoticeBar, Toast, Toolbar } from "./SettingsWindow.jsx";
import { getStatus, onFixPermissions, onStatus, setEnabled } from "./settingsApi.js";
import { TABS, finishFirstRun, firstRunDone, fromStatusEvent, showToast, switchOn } from "./shell.js";

export default function App({ autoUpdate = true, storage = globalThis.localStorage }) {
  // The Simulator window exists only in `freewheel --simulate` (main.rs).
  if (globalThis.location?.hash === "#simulator") return <Simulator />;
  return <Settings autoUpdate={autoUpdate} storage={storage} />;
}

function Settings({ autoUpdate, storage }) {
  const [update, setUpdate] = useState(null);
  const [tab, setTab] = useState(TABS[0]);
  const [status, setStatus] = useState(null);
  const [firstRun, setFirstRun] = useState(() => !firstRunDone(storage));
  const [fixing, setFixing] = useState(false);
  const pending = useRef(null);
  useEffect(() => {
    startupCheck({ check, autoUpdate, onState: setUpdate }).then((u) => (pending.current = u));
  }, [autoUpdate]);
  useEffect(() => {
    if (!globalThis.window?.__TAURI__) return undefined;
    getStatus().then(setStatus, () => {});
    const unlisten = [onStatus(setStatus), onFixPermissions(() => setFixing(true))];
    return () => unlisten.forEach((u) => u.then((stop) => stop(), () => {}));
  }, []);
  const state = fromStatusEvent(status ?? { connection: "Absent" });
  const [toast, setToast] = useState(null);
  // A failed switch is an error that needs action: it is logged, never toasted.
  const turn = (on) =>
    switchOn(setEnabled, on).then(
      (text) => showToast(setToast, text),
      (error) => console.error("freewheel: turning", on ? "on" : "off", "failed:", String(error)),
    );
  if (firstRun)
    return (
      <FirstRun
        state={state}
        permissions={status?.permissions}
        onFinish={() => {
          finishFirstRun(storage);
          console.info("freewheel: first run finished");
          setTab("Keys");
          setFirstRun(false);
        }}
      />
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
      {status && !fixing && <NoticeBar state={state} onFix={() => setFixing(true)} onTurnOn={() => turn(true)} />}
      {fixing ? (
        <PermissionsScreen button="Done" onDone={() => setFixing(false)} />
      ) : (
        <main>
          <h1>freewheel settings</h1>
          <Status />
          {tab === "Keys" && <KeysTab />}
          {tab === "Wheel" && <WheelTab />}
          {tab === "DaVinci Resolve" && <ResolveTab />}
          <Permissions />
          {tab === "Help & backup" && <Backup onToast={(text) => showToast(setToast, text)} />}
        </main>
      )}
      <Toast text={toast} />
    </>
  );
}
