import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { useEffect, useRef, useState } from "react";
import UpdateBanner from "./UpdateBanner.jsx";
import { checkNow, download, restart, startupCheck } from "./updater.js";
import General, { applyAppearance, changeSetting, switchLogin } from "./General.jsx";
import Simulator from "./Simulator.jsx";
import Backup from "./Backup.jsx";
import FirstRun from "./FirstRun.jsx";
import { PermissionsScreen } from "./Permissions.jsx";
import KeysTab from "./KeysTab.jsx";
import ResolveTab from "./ResolveTab.jsx";
import WheelTab from "./WheelTab.jsx";
import { NoticeBar, Toast, Toolbar } from "./SettingsWindow.jsx";
import {
  getConfig,
  getStartAtLogin,
  getStatus,
  getVersion,
  onFixPermissions,
  onStatus,
  saveConfig,
  setDetailedLogs,
  setEnabled,
  setStartAtLogin,
} from "./settingsApi.js";
import { TABS, finishFirstRun, firstRunDone, fromStatusEvent, showToast, switchOn } from "./shell.js";

export default function App({ autoUpdate = true, storage = globalThis.localStorage }) {
  // The Simulator window exists only in `freewheel --simulate` (main.rs).
  if (globalThis.location?.hash === "#simulator") return <Simulator />;
  return <Settings autoUpdate={autoUpdate} storage={storage} />;
}

/**
 * The settings window, as the design handoff's prototype draws it: the
 * toolbar, the update banner, the one notice, then the tab's page (or the
 * first run / permission fix in place of all of it) and the toast.
 */
function Settings({ autoUpdate, storage }) {
  const [update, setUpdate] = useState(null);
  const [tab, setTab] = useState(TABS[0]);
  const [status, setStatus] = useState(null);
  const [config, setConfig] = useState(null);
  const [login, setLogin] = useState(null);
  const [version, setVersion] = useState(null);
  const [firstRun, setFirstRun] = useState(() => !firstRunDone(storage));
  const [fixing, setFixing] = useState(false);
  const [toast, setToast] = useState(null);
  const pending = useRef(null);
  const toastTimer = useRef(null);
  const say = (text) => {
    globalThis.clearTimeout(toastTimer.current);
    toastTimer.current = showToast(setToast, text);
  };
  useEffect(() => {
    // 'Update automatically' comes from the config; on when it cannot be read.
    const loaded = globalThis.window?.__TAURI__ ? getConfig().catch(() => null) : Promise.resolve(null);
    loaded
      .then((c) => {
        setConfig(c);
        if (c?.settings) applyAppearance(c.settings.appearance);
        return startupCheck({ check, autoUpdate: c?.settings?.auto_update ?? autoUpdate, onState: setUpdate });
      })
      .then((u) => (pending.current = u));
  }, [autoUpdate]);
  useEffect(() => {
    if (!globalThis.window?.__TAURI__) return undefined;
    getStartAtLogin().then(setLogin, () => {});
    getVersion().then(setVersion, () => {});
    getStatus().then(setStatus, () => {});
    const unlisten = [onStatus(setStatus), onFixPermissions(() => setFixing(true))];
    return () => unlisten.forEach((u) => u.then((stop) => stop(), () => {}));
  }, []);
  const state = fromStatusEvent(status ?? { connection: "Absent" });
  // A failed switch is an error that needs action: it is logged, never toasted.
  const turn = (on) =>
    switchOn(setEnabled, on).then(say, (error) => console.error("freewheel: turning", on ? "on" : "off", "failed:", String(error)));
  const onSetting = config
    ? (key, value) => changeSetting({ config, key, value, setConfig, save: saveConfig, load: getConfig })
    : undefined;
  const onDetailedLogs = config
    ? (on) =>
        setDetailedLogs(on).then(
          () => setConfig((c) => c && { ...c, settings: { ...c.settings, detailed_logs: on } }),
          (error) => console.error("freewheel: detailed logs failed:", String(error)),
        )
    : undefined;
  let content;
  if (firstRun)
    content = (
      <FirstRun
        state={state}
        permissions={status?.permissions}
        onFinish={() => {
          finishFirstRun(storage);
          console.info("freewheel: first run finished");
          setTab("Keys");
          setFirstRun(false);
          say("You’re all set. Your wheel already scrolls — give it a spin.");
        }}
      />
    );
  else if (fixing)
    content = (
      <div className="setup">
        <PermissionsScreen button="Done" onDone={() => setFixing(false)} />
      </div>
    );
  else
    content = (
      <>
        <Toolbar
          tab={tab}
          onTab={setTab}
          state={state}
          onToggle={status ? () => turn(!state.enabled) : undefined}
          onFix={() => setFixing(true)}
        />
        <UpdateBanner
          update={update}
          onInstall={() => download(pending.current, setUpdate)}
          onLater={() => setUpdate(null)}
          onRestart={() => restart(pending.current, relaunch, setUpdate)}
        />
        {status && <NoticeBar state={state} onFix={() => setFixing(true)} onTurnOn={() => turn(true)} />}
        <div className="app-body">
          {tab === "Keys" && <KeysTab status={status} onWheel={() => setTab("Wheel")} onToast={say} />}
          {tab === "Wheel" && <WheelTab status={status} />}
          {tab === "DaVinci Resolve" && <ResolveTab settings={config?.settings} onSetting={onSetting} />}
          {tab === "General" && (
            <General
              settings={config?.settings}
              onSetting={onSetting}
              startAtLogin={login}
              onStartAtLogin={(on) => switchLogin({ on, set: setStartAtLogin, get: getStartAtLogin, setLogin, setConfig })}
              version={version}
              update={update}
              onCheck={() =>
                checkNow({ check, autoUpdate: config?.settings?.auto_update ?? autoUpdate, onState: setUpdate }).then(
                  (u) => (pending.current = u),
                )
              }
            />
          )}
          {tab === "Help & backup" && <Backup detailedLogs={config?.settings?.detailed_logs} onDetailedLogs={onDetailedLogs} />}
        </div>
      </>
    );
  return (
    <div className="app">
      {content}
      <Toast text={toast} />
    </div>
  );
}
