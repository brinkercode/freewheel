import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { useEffect, useRef, useState } from "react";
import UpdateBanner from "./UpdateBanner.jsx";
import { checkNow, download, restart, startupCheck } from "./updater.js";
import General, { applyAppearance, changeSetting, switchLogin } from "./General.jsx";
import Simulator from "./Simulator.jsx";
import Permissions from "./Permissions.jsx";
import Status from "./Status.jsx";
import { NoticeBar, Toast, Toolbar } from "./SettingsWindow.jsx";
import {
  getConfig,
  getStartAtLogin,
  getStatus,
  getVersion,
  onStatus,
  saveConfig,
  setEnabled,
  setStartAtLogin,
} from "./settingsApi.js";
import { TABS, fromStatusEvent, showToast, switchOn } from "./shell.js";

export default function App() {
  // The Simulator window exists only in `freewheel --simulate` (main.rs).
  if (globalThis.location?.hash === "#simulator") return <Simulator />;
  return <Settings />;
}

function Settings() {
  const [update, setUpdate] = useState(null);
  const [tab, setTab] = useState(TABS[0]);
  const [status, setStatus] = useState(null);
  const [config, setConfig] = useState(null);
  const [login, setLogin] = useState(null);
  const [version, setVersion] = useState(null);
  const pending = useRef(null);
  useEffect(() => {
    // 'Update automatically' comes from the config; on when it cannot be read.
    const loaded = globalThis.window?.__TAURI__ ? getConfig().catch(() => null) : Promise.resolve(null);
    loaded
      .then((c) => {
        setConfig(c);
        if (c) applyAppearance(c.settings.appearance);
        return startupCheck({ check, autoUpdate: c?.settings.auto_update ?? true, onState: setUpdate });
      })
      .then((u) => (pending.current = u));
  }, []);
  useEffect(() => {
    if (!globalThis.window?.__TAURI__) return undefined;
    getStartAtLogin().then(setLogin, () => {});
    getVersion().then(setVersion, () => {});
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
      {tab === "General" ? (
        <General
          settings={config?.settings}
          onSetting={config ? (key, value) => changeSetting({ config, key, value, setConfig, save: saveConfig }) : undefined}
          startAtLogin={login}
          onStartAtLogin={(on) => switchLogin({ on, set: setStartAtLogin, get: getStartAtLogin, setLogin, setConfig })}
          version={version}
          update={update}
          onCheck={() =>
            checkNow({ check, autoUpdate: config?.settings.auto_update ?? true, onState: setUpdate }).then(
              (u) => (pending.current = u),
            )
          }
        />
      ) : (
        <main>
          <h1>freewheel settings</h1>
          <Status />
          <Permissions />
        </main>
      )}
      <Toast text={toast} />
    </>
  );
}
