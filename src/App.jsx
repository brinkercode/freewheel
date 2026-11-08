import { useEffect, useRef, useState } from "react";
import UpdateBanner from "./UpdateBanner.jsx";
import { WhatsNewNotice, WhatsNewPage } from "./WhatsNew.jsx";
import { currentNews, loadNotes, markSeen, openReleasePage, rememberNotes, whatsNew } from "./whatsNew.js";
import { checkNow, download, restart, startupCheck } from "./updater.js";
import { simRelaunch } from "./simUpdate.js";
import { trace } from "./trace.js";
import { awaitRestart, rustUpdates } from "./selfUpdate.js";
import General, { applyAppearance, changeSetting, switchLogin } from "./General.jsx";
import Simulator from "./Simulator.jsx";
import Backup from "./Backup.jsx";
import FirstRun from "./FirstRun.jsx";
import { PermissionsScreen } from "./Permissions.jsx";
import KeysTab from "./KeysTab.jsx";
import ResolveTab from "./ResolveTab.jsx";
import WheelTab from "./WheelTab.jsx";
import { NoticeBar, Toast, Toolbar } from "./SettingsWindow.jsx";
import { TitleBar, isLinux, isMac } from "./TitleBar.jsx";
import {
  getConfig,
  getDesktopHints,
  getStartAtLogin,
  getStatus,
  getVersion,
  onFixPermissions,
  onLogsSaved,
  onStatus,
  onTrayUpdate,
  reportUpdateState,
  saveConfig,
  settingsReady,
  setDetailedLogs,
  setEnabled,
  setStartAtLogin,
} from "./settingsApi.js";
import { listenSimulate, simulating } from "./simulateEvents.js";
import { TABS, finishFirstRun, firstRunDone, fromStatusEvent, showToast, switchOn } from "./shell.js";

/** The status event's permissions, when it has any (else the screen polls get_permissions). */
const asked = (permissions) => (permissions && Object.keys(permissions).length > 0 ? permissions : null);

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
  const [update, setUpdateState] = useState(null);
  // Every change of update state is traced (src/trace.js; progress only once per status).
  const lastTraced = useRef(null);
  const setUpdate = (s) => {
    const status = s?.status ?? null;
    if (status !== lastTraced.current) trace("update", { status, version: s?.version, during: s?.during, message: s?.message });
    lastTraced.current = status;
    setUpdateState(s);
  };
  const [tab, setTab] = useState(TABS[0]);
  const [status, setStatus] = useState(null);
  const [config, setConfig] = useState(null);
  const [login, setLogin] = useState(null);
  const [trayHint, setTrayHint] = useState(null);
  const [version, setVersion] = useState(null);
  const [firstRun, setFirstRun] = useState(() => !firstRunDone(storage));
  const [fixing, setFixing] = useState(false);
  const [toast, setToast] = useState(null);
  const [record, setRecord] = useState(null);
  const [news, setNews] = useState(null);
  const [newsOpen, setNewsOpen] = useState(false);
  const [trayLogs, setTrayLogs] = useState(null);
  // The tray's update item runs these (set each render, so it acts on the current state).
  const trayActions = useRef({});
  const pending = useRef(null);
  // Update checks running now, and whether one was asked for (the tray's or Check for updates).
  const checks = useRef(0);
  const manual = useRef(false);
  const toastTimer = useRef(null);
  // The Rust updater (in --simulate its simulated source) and, in --simulate
  // only, the version a simulated update "installed".
  const updates = useRef(null);
  updates.current ??= rustUpdates();
  const [simVersion, setSimVersion] = useState(null);
  const shownVersion = simVersion ?? version;
  const current = useRef(null);
  current.current = { version: shownVersion, update, config };
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
        // A check asked for meanwhile (the tray's, which may be what opened this window) answers instead.
        if (checks.current > 0) return null;
        checks.current += 1;
        const check = () => source().check().then((u) => (manual.current ? null : u));
        return startupCheck({ check, autoUpdate: c?.settings?.auto_update ?? autoUpdate, onState: setUpdate }).finally(() => (checks.current -= 1));
      })
      .then((u) => u && (pending.current = u));
  }, [autoUpdate]);
  useEffect(() => {
    if (!globalThis.window?.__TAURI__) return undefined;
    getStartAtLogin().then(setLogin, () => {});
    getDesktopHints().then((h) => setTrayHint(h?.tray ?? null), () => {});
    getVersion().then(setVersion, () => {});
    getStatus().then(setStatus, () => {});
    // --simulate only (src/simulateEvents.js): the Simulator's "Record a key" and "New version".
    const sim = listenSimulate({
      onRecordKey: (key) => {
        setFixing(false);
        setNewsOpen(false);
        setTab("Keys");
        setRecord((r) => ({ key, n: (r?.n ?? 0) + 1 }));
      },
      // "New version": the simulated source (Rust) published a newer version; freewheel finds it by itself.
      onUpdate: () => {
        const busy = ["checking", "downloading", "restarting"].includes(current.current.update?.status);
        if (busy || checks.current > 0) return;
        checks.current += 1;
        startupCheck({
          check: () => updates.current.check(),
          autoUpdate: current.current.config?.settings?.auto_update ?? autoUpdate,
          onState: setUpdate,
        })
          .then((u) => u && (pending.current = u))
          .finally(() => (checks.current -= 1));
      },
    });
    const unlisten = [
      onStatus(setStatus),
      onFixPermissions(() => setFixing(true)),
      // The tray's 'Check for updates' / 'Restart to update' item.
      onTrayUpdate((action) => {
        trace("tray", { action, pending: pending.current?.version ?? null, status: current.current.update?.status ?? null });
        trayActions.current[action]?.();
      }),
      // The tray's 'Download logs for support…' opens Help & backup, which says where they went.
      onLogsSaved((saved) => {
        setFixing(false);
        setNewsOpen(false);
        setTab("Help & backup");
        setTrayLogs({ ...saved });
      }),
    ];
    // Every listener is attached: what the tray sent while this window was
    // built or loading arrives now (app_windows.rs settings_ready). A status
    // change between get_status and its listener is read again.
    Promise.allSettled([...unlisten, sim.ready])
      .then(() => (trace("page ready"), settingsReady()))
      .then(() => getStatus().then(setStatus))
      .catch((e) => console.error("freewheel: the settings window could not say it is ready:", String(e)));
    return () => {
      sim();
      unlisten.forEach((u) => u.then((stop) => stop(), () => {}));
    };
  }, []);
  // The tray's update item follows the update flow (src-tauri tray.rs update_item).
  useEffect(() => {
    if (globalThis.window?.__TAURI__) reportUpdateState(update).catch(() => {});
  }, [update]);
  // What's new: once per update, after the restart into the newer version.
  useEffect(() => {
    if (shownVersion) setNews(whatsNew(storage, shownVersion));
  }, [shownVersion, storage]);
  // The notes page opened without notes stored from the update: the
  // version's GitHub release notes (src/whatsNew.js loadNotes), asked for
  // only now, never in the background.
  const newsVersion = newsOpen && news && !news.notes && !news.loaded ? news.version : null;
  useEffect(() => {
    if (!newsVersion) return;
    let live = true;
    setNews((n) => (n?.version === newsVersion ? { ...n, loading: true } : n));
    loadNotes({ version: newsVersion }, { storage }).then((notes) => {
      if (live) setNews((n) => (n?.version === newsVersion ? { ...n, notes, loading: false, loaded: true } : n));
    });
    return () => {
      live = false;
    };
  }, [newsVersion, storage]);
  const seeNews = (open) => {
    markSeen(storage, news.version);
    if (open) setNewsOpen(true);
    else setNews(null);
  };
  // Where updates come from: the Rust updater; in --simulate its simulated
  // source, and the restart is drawn here (nothing is installed).
  const source = () => {
    const check = () => updates.current.check();
    if (!simulating()) return { check, relaunch: awaitRestart };
    return {
      check,
      relaunch: (u) =>
        simRelaunch(() => {
          trace("restarted", { version: u.version });
          pending.current = null;
          setSimVersion(u.version);
          setUpdate(null);
          say(`Updated to freewheel ${u.version}`);
        }),
    };
  };
  const autoUpdateOn = () => current.current.config?.settings?.auto_update ?? autoUpdate;
  const runCheck = (auto) => {
    let last = null;
    const onState = (s) => {
      last = s;
      setUpdate(s);
    };
    manual.current = true;
    checks.current += 1;
    return checkNow({ check: source().check, autoUpdate: auto, onState })
      .then((u) => {
        pending.current = u;
        if (last?.status === "current") say("You’re up to date.");
      })
      .finally(() => (checks.current -= 1));
  };
  const onCheck = () => runCheck(autoUpdateOn());
  // A window built again for the tray's 'Download freewheel …' has not found the update yet: find it, then download.
  const onDownload = () => (pending.current ? download(pending.current, setUpdate) : runCheck(true));
  const onRestart = () => {
    const u = pending.current;
    if (!u) return trace("restart: nothing downloaded");
    const { relaunch: again } = source();
    const install = () => (trace("install", { version: u.version }), u.install().then((r) => (trace("installed", { result: r }), r)));
    restart({ ...u, install }, () => (trace("relaunch"), again(u)), setUpdate, (up) => rememberNotes(storage, up));
  };
  trayActions.current = { check: onCheck, download: onDownload, restart: onRestart };
  const state = fromStatusEvent(status ?? { connection: "Absent" }, config?.settings?.low_battery_threshold);
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
        onToast={say}
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
        <PermissionsScreen initial={asked(status?.permissions)} button="Done" onDone={() => setFixing(false)} onToast={say} />
      </div>
    );
  else
    content = (
      <>
        <Toolbar
          tab={tab}
          onTab={(t) => {
            if (newsOpen) {
              setNewsOpen(false);
              setNews(null);
            }
            setTab(t);
          }}
          state={state}
          onToggle={status ? () => turn(!state.enabled) : undefined}
          onFix={() => setFixing(true)}
        />
        <UpdateBanner
          update={update}
          onInstall={onDownload}
          onLater={() => setUpdate(null)}
          onRestart={onRestart}
        />
        {!newsOpen && <WhatsNewNotice news={news} onOpen={() => seeNews(true)} onDismiss={() => seeNews(false)} />}
        {status && <NoticeBar state={state} onFix={() => setFixing(true)} onTurnOn={() => turn(true)} />}
        <div className="app-body">
          {newsOpen && (
            <WhatsNewPage
              news={news}
              openRelease={globalThis.window?.__TAURI__ ? openReleasePage : undefined}
              onDone={() => {
                setNewsOpen(false);
                setNews(null);
              }}
            />
          )}
          {!newsOpen && tab === "Keys" && <KeysTab status={status} record={record} onWheel={() => setTab("Wheel")} onToast={say} />}
          {!newsOpen && tab === "Wheel" && <WheelTab status={status} />}
          {!newsOpen && tab === "DaVinci Resolve" && <ResolveTab settings={config?.settings} onSetting={onSetting} />}
          {!newsOpen && tab === "General" && (
            <General
              settings={config?.settings}
              onSetting={onSetting}
              startAtLogin={login}
              onStartAtLogin={(on) => switchLogin({ on, set: setStartAtLogin, get: getStartAtLogin, setLogin, setConfig })}
              version={shownVersion}
              update={update}
              onCheck={onCheck}
              onDownload={onDownload}
              onRestart={onRestart}
              onWhatsNew={
                shownVersion
                  ? () => {
                      setNews(currentNews(storage, shownVersion));
                      setNewsOpen(true);
                    }
                  : undefined
              }
              trayHint={trayHint}
            />
          )}
          {!newsOpen && tab === "Help & backup" && <Backup version={shownVersion} detailedLogs={config?.settings?.detailed_logs} onDetailedLogs={onDetailedLogs} trayLogs={trayLogs} />}
        </div>
      </>
    );
  return (
    <div className={`app${isMac() ? " is-mac" : ""}${isLinux() ? " is-linux" : ""}`}>
      {(firstRun || fixing) && <TitleBar />}
      {content}
      <Toast text={toast} />
    </div>
  );
}
