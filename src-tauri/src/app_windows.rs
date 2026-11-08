//! The settings window (and, in `--simulate`, the Simulator window) exists
//! only while it is on screen. Closing it destroys its webview, so freewheel
//! idling in the tray runs no WebView2 processes (measured by
//! scripts/footprint-windows.mjs); Open settings, a second launch or a
//! Simulator jump builds it again from tauri.conf.json.
//!
//! One exception: while an update is checking, downloading, ready or
//! restarting, the flow lives in the settings window's script
//! (src/updater.js), so closing then only hides the window; it is destroyed
//! once the update flow is idle again ([`update_state`]).
//!
//! Windows are only ever built off the main thread: on Windows, building a
//! webview from a sync command or an event handler deadlocks (WebView2
//! waits for the main thread, which waits for the build).
//!
//! What the tray (and the Simulator) sends the settings window goes through
//! [`emit_to_settings`] / [`eval_in_settings`]: it is delivered only once
//! the page has attached its event listeners and said so
//! ([`settings_ready`]). A window being built, loading or reloading has no
//! listeners yet, so an event emitted then is lost; it waits instead, in
//! order, until the page is ready.

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};

/// The settings window's label (tauri.conf.json's only window).
pub const SETTINGS_WINDOW: &str = "main";

/// The update states whose flow lives in the settings window's script.
pub fn update_busy(status: Option<&str>) -> bool {
    matches!(
        status,
        Some("checking" | "downloading" | "ready" | "restarting")
    )
}

/// What happens to a window the user closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnClose {
    /// Let it close: the webview and its WebView2 processes go.
    Destroy,
    /// Hide it to the tray (the settings window during an update).
    Hide,
}

pub fn on_close(label: &str, update_busy: bool) -> OnClose {
    if label == SETTINGS_WINDOW && update_busy {
        OnClose::Hide
    } else {
        OnClose::Destroy
    }
}

/// Something for the settings window that waits until its page is ready.
#[derive(Debug, Clone, PartialEq)]
enum Pending {
    Emit(String, serde_json::Value),
    Eval(String),
}

/// At most this many deliveries wait for a page (the oldest go first).
const MAX_PENDING: usize = 32;

/// The windows' life: whether the update flow is busy, whether the
/// settings window is hidden only for it, whether its page has its
/// listeners, and what waits for them.
#[derive(Default)]
pub struct Lifecycle {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    update_busy: bool,
    parked: bool,
    /// The settings page has attached its listeners ([`settings_ready`]);
    /// false while it is built, loads or reloads, and once it is closed.
    ready: bool,
    pending: Vec<Pending>,
}

impl State {
    fn wait(&mut self, p: Pending) {
        if self.pending.len() == MAX_PENDING {
            self.pending.remove(0);
        }
        self.pending.push(p);
    }
}

fn lifecycle<R: Runtime>(app: &AppHandle<R>) -> Option<tauri::State<'_, Lifecycle>> {
    app.try_state::<Lifecycle>()
}

fn lock(l: &Lifecycle) -> std::sync::MutexGuard<'_, State> {
    l.state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn deliver<R: Runtime>(app: &AppHandle<R>, p: Pending) {
    match p {
        Pending::Emit(event, payload) => {
            tracing::info!(event, %payload, "settings window: delivered");
            if let Err(e) = app.emit_to(SETTINGS_WINDOW, &event, payload) {
                tracing::warn!(error = %e, event, "the settings window did not hear an event");
            }
        }
        Pending::Eval(script) => {
            let done = app
                .get_webview_window(SETTINGS_WINDOW)
                .map(|w| w.eval(&script));
            if !matches!(done, Some(Ok(()))) {
                tracing::warn!("a script for the settings window did not run");
            }
        }
    }
}

/// Delivers `p` now when the settings page is ready, else once it is.
fn to_settings<R: Runtime>(app: &AppHandle<R>, p: Pending) {
    let Some(l) = lifecycle(app) else {
        return deliver(app, p);
    };
    let mut s = lock(&l);
    if s.ready && app.get_webview_window(SETTINGS_WINDOW).is_some() {
        // Under the lock: never ahead of what was waiting.
        deliver(app, p);
    } else {
        tracing::info!(what = ?p, waiting = s.pending.len() + 1, "settings window not listening yet: waits");
        s.wait(p);
    }
}

/// Emits `event` to the settings window once its page listens: at once
/// when it is ready, else when it says so ([`settings_ready`]; a window
/// that is closed must be opened by the caller).
pub fn emit_to_settings<R: Runtime, S: serde::Serialize + Clone>(
    app: &AppHandle<R>,
    event: &str,
    payload: S,
) {
    match serde_json::to_value(payload) {
        Ok(value) => to_settings(app, Pending::Emit(event.to_string(), value)),
        Err(e) => tracing::error!(error = %e, event, "an event for the settings window"),
    }
}

/// Runs `script` in the settings page once it is ready (as
/// [`emit_to_settings`]).
pub fn eval_in_settings<R: Runtime>(app: &AppHandle<R>, script: String) {
    to_settings(app, Pending::Eval(script));
}

/// The settings page is gone or not listening yet: what is sent to it
/// waits. Called when its page starts loading and when it closes.
pub fn not_ready<R: Runtime>(app: &AppHandle<R>) {
    if let Some(l) = lifecycle(app) {
        let mut s = lock(&l);
        if s.ready {
            tracing::info!("settings window not ready (loading or closed)");
        }
        s.ready = false;
    }
}

/// The settings page has attached its event listeners (src/App.jsx): what
/// waited for it is delivered, in order. In `--simulate` the page learns
/// that first, so the Simulator's events that waited are not ignored.
#[tauri::command]
pub async fn settings_ready(app: AppHandle, webview: tauri::Webview) -> Result<(), String> {
    if webview.label() != SETTINGS_WINDOW {
        return Err(format!("{} is not the settings window", webview.label()));
    }
    if crate::simulate::simulating(&app) {
        webview
            .eval(crate::simulate::SIMULATE_FLAG_SCRIPT)
            .map_err(|e| e.to_string())?;
    }
    ready(&app);
    Ok(())
}

/// The settings page listens: delivers what waited, in order; returns how
/// many.
fn ready<R: Runtime>(app: &AppHandle<R>) -> usize {
    let Some(l) = lifecycle(app) else { return 0 };
    let mut s = lock(&l);
    s.ready = true;
    let pending = std::mem::take(&mut s.pending);
    let n = pending.len();
    tracing::info!(waiting = n, "settings window ready");
    for p in pending {
        deliver(app, p);
    }
    n
}

/// `CloseRequested` for `window`: hides it while the update flow needs it,
/// else lets it close.
pub fn close_requested<R: Runtime>(window: &tauri::Window<R>, api: &tauri::CloseRequestApi) {
    let app = window.app_handle();
    let busy = lifecycle(app).is_some_and(|l| lock(&l).update_busy);
    match on_close(window.label(), busy) {
        OnClose::Hide => {
            api.prevent_close();
            if let Some(l) = lifecycle(app) {
                lock(&l).parked = true;
            }
            let _ = window.hide();
            tracing::info!("settings window hidden while an update runs");
        }
        OnClose::Destroy => {
            if window.label() == SETTINGS_WINDOW {
                not_ready(app);
            }
            tracing::info!(window = window.label(), "window closed");
        }
    }
}

/// The settings window's update state (tray_update_state): a window hidden
/// only for the update flow is destroyed once the flow is idle.
pub fn update_state<R: Runtime>(app: &AppHandle<R>, status: Option<&str>) {
    let Some(l) = lifecycle(app) else { return };
    let park_over = {
        let mut s = lock(&l);
        s.update_busy = update_busy(status);
        let over = s.parked && !s.update_busy;
        if over {
            s.parked = false;
        }
        over
    };
    if park_over && let Some(w) = app.get_webview_window(SETTINGS_WINDOW) {
        let hidden = !w.is_visible().unwrap_or(true);
        if hidden {
            tracing::info!("update flow idle: closing the hidden settings window");
            not_ready(app);
            let _ = w.destroy();
        }
    }
}

/// The settings window's builder from its tauri.conf.json entry; in
/// `--simulate` with the simulator's own webview storage (#202).
pub fn settings_builder<'a, R: Runtime, M: Manager<R>>(
    app: &'a M,
    config: &tauri::utils::config::WindowConfig,
    simulate: bool,
) -> tauri::Result<tauri::WebviewWindowBuilder<'a, R, M>> {
    let builder = tauri::WebviewWindowBuilder::from_config(app, config)?;
    Ok(if simulate {
        crate::simulate::own_storage(app, builder)
    } else {
        builder
    })
}

/// Builds the settings window from tauri.conf.json (plus, in --simulate,
/// the Simulator window), unless it is there. Call off the main thread.
fn build_settings<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<WebviewWindow<R>> {
    // Two quick opens build one window.
    static BUILDING: Mutex<()> = Mutex::new(());
    let _one = BUILDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(w) = app.get_webview_window(SETTINGS_WINDOW) {
        return Ok(w);
    }
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == SETTINGS_WINDOW)
        .cloned()
        .ok_or(tauri::Error::WindowNotFound)?;
    tracing::info!("building the settings window");
    not_ready(app);
    let simulating = crate::simulate::simulating(app);
    let window = settings_builder(app, &config, simulating)?.build()?;
    if simulating {
        window.set_title(&freewheel_core::simulator::title(
            "freewheel settings",
            true,
        ))?;
    }
    Ok(window)
}

/// Shows and focuses the settings window, building it (and in --simulate
/// the Simulator window) when it was closed. Never blocks the caller: the
/// build runs on a thread of its own. Returns whether the settings window
/// was already there. (Send it things with [`emit_to_settings`]: they wait
/// for a new window's page.)
pub fn show_settings<R: Runtime>(app: &AppHandle<R>) -> bool {
    let existed = app.get_webview_window(SETTINGS_WINDOW).is_some();
    if let Some(l) = lifecycle(app) {
        lock(&l).parked = false;
    }
    let simulator_missing = crate::simulate::simulating(app)
        && app
            .get_webview_window(crate::simulate::SIMULATOR_WINDOW)
            .is_none();
    let app = app.clone();
    let show = move || {
        let window = match build_settings(&app) {
            Ok(w) => w,
            Err(e) => return tracing::error!(error = %e, "cannot open the settings window"),
        };
        if crate::simulate::simulating(&app)
            && app
                .get_webview_window(crate::simulate::SIMULATOR_WINDOW)
                .is_none()
            && let Err(e) = crate::simulate::open_window(&app)
        {
            tracing::error!(error = %e, "cannot open the Simulator window");
        }
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    };
    if existed && !simulator_missing {
        show();
    } else {
        std::thread::spawn(show);
    }
    existed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_destroys_every_window_but_a_settings_window_mid_update() {
        assert_eq!(on_close(SETTINGS_WINDOW, false), OnClose::Destroy);
        assert_eq!(
            on_close(crate::simulate::SIMULATOR_WINDOW, false),
            OnClose::Destroy
        );
        assert_eq!(on_close(SETTINGS_WINDOW, true), OnClose::Hide);
        assert_eq!(
            on_close(crate::simulate::SIMULATOR_WINDOW, true),
            OnClose::Destroy
        );
    }

    /// src/updater.js's states: the flow is in the window's script from
    /// the check until the restart.
    #[test]
    fn the_update_flow_needs_the_window_until_it_is_idle() {
        for busy in ["checking", "downloading", "ready", "restarting"] {
            assert!(update_busy(Some(busy)), "{busy}");
            assert!(include_str!("../../src/updater.js").contains(&format!("status: \"{busy}\"")));
        }
        for idle in [None, Some("current"), Some("available"), Some("error")] {
            assert!(!update_busy(idle), "{idle:?}");
        }
    }

    fn waiting<R: Runtime>(app: &AppHandle<R>) -> Vec<Pending> {
        lock(&app.state::<Lifecycle>()).pending.clone()
    }

    /// A settings page that has not said it listens (being built, loading,
    /// reloading, closed) gets nothing: it all waits, in order, and arrives
    /// once it is ready; then events go straight through.
    #[test]
    fn what_the_tray_sends_waits_until_the_settings_page_listens() {
        use tauri::Listener;
        let app = tauri::test::mock_app();
        app.manage(Lifecycle::default());
        let h = app.handle().clone();
        // Closed: nothing to deliver to, it waits.
        emit_to_settings(&h, "tray-update", "check");
        eval_in_settings(&h, "1".into());
        emit_to_settings(&h, "logs-saved", serde_json::json!({ "path": "a" }));
        assert_eq!(
            waiting(&h),
            vec![
                Pending::Emit("tray-update".into(), "check".into()),
                Pending::Eval("1".into()),
                Pending::Emit("logs-saved".into(), serde_json::json!({ "path": "a" })),
            ]
        );
        // Built, still loading: it still waits.
        let _w = tauri::WebviewWindowBuilder::new(&h, SETTINGS_WINDOW, Default::default())
            .build()
            .unwrap();
        not_ready(&h);
        emit_to_settings(&h, "fix-permissions", ());
        assert_eq!(waiting(&h).len(), 4);
        // Ready: all of it, in order (Rust listeners stand in for the page's).
        let heard = std::sync::Arc::new(Mutex::new(Vec::new()));
        for event in ["tray-update", "logs-saved", "fix-permissions"] {
            let heard = heard.clone();
            h.listen_any(event, move |e| {
                heard.lock().unwrap().push(e.payload().to_string())
            });
        }
        assert_eq!(ready(&h), 4);
        assert!(waiting(&h).is_empty());
        assert_eq!(
            *heard.lock().unwrap(),
            vec!["\"check\"", r#"{"path":"a"}"#, "null"]
        );
        // Ready: straight through.
        emit_to_settings(&h, "tray-update", "restart");
        assert!(waiting(&h).is_empty());
        assert_eq!(heard.lock().unwrap().last().unwrap(), "\"restart\"");
        // Reloading (page load started) or closed: waits again.
        not_ready(&h);
        emit_to_settings(&h, "tray-update", "check");
        assert_eq!(waiting(&h).len(), 1);
    }

    #[test]
    fn at_most_a_few_deliveries_wait_and_the_oldest_go_first() {
        let mut s = State::default();
        for i in 0..MAX_PENDING + 3 {
            s.wait(Pending::Eval(i.to_string()));
        }
        assert_eq!(s.pending.len(), MAX_PENDING);
        assert_eq!(s.pending[0], Pending::Eval("3".into()));
    }

    /// Every way the settings page starts loading or goes away marks it
    /// not ready (main.rs on_page_load, close, the parked window's end, a
    /// new build); the page says it is ready only after its listeners.
    #[test]
    fn the_page_load_and_the_window_close_mark_the_settings_page_not_ready() {
        let main = include_str!("main.rs");
        assert!(main.contains("PageLoadEvent::Started"));
        assert!(main.contains("app_windows::not_ready(webview.app_handle())"));
        assert_eq!(main.matches("app_windows::settings_ready,").count(), 2);
        let me = include_str!("app_windows.rs");
        assert!(me.matches("not_ready(app);").count() >= 3);
        let js = include_str!("../../src/App.jsx");
        let ready = js
            .find(".then(() => (trace(\"page ready\"), settingsReady()))")
            .unwrap();
        assert!(js[..ready].contains("Promise.allSettled([...unlisten, sim.ready])"));
    }

    #[test]
    fn the_settings_window_is_the_configured_one() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let windows = conf["app"]["windows"].as_array().unwrap();
        assert_eq!(windows.len(), 1);
        // No label: Tauri's default, "main".
        assert!(
            windows[0]
                .get("label")
                .is_none_or(|l| *l == SETTINGS_WINDOW)
        );
    }
}
