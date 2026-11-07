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

/// An event for the settings window that waits for it to load when it is
/// being built again.
type Pending = (String, serde_json::Value);

/// The windows' life: whether the update flow is busy, whether the
/// settings window is hidden only for it, and events waiting for a new
/// settings window.
#[derive(Default)]
pub struct Lifecycle {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    update_busy: bool,
    parked: bool,
    pending: Vec<Pending>,
}

fn lifecycle<R: Runtime>(app: &AppHandle<R>) -> Option<tauri::State<'_, Lifecycle>> {
    app.try_state::<Lifecycle>()
}

fn lock(l: &Lifecycle) -> std::sync::MutexGuard<'_, State> {
    l.state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
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
        OnClose::Destroy => tracing::info!(window = window.label(), "window closed"),
    }
}

/// The settings window's update state (tray_update_state): a window hidden
/// only for the update flow is destroyed once the flow is idle; a new
/// window's first report delivers the events that waited for it.
pub fn update_state<R: Runtime>(app: &AppHandle<R>, status: Option<&str>) {
    let Some(l) = lifecycle(app) else { return };
    let (park_over, pending) = {
        let mut s = lock(&l);
        s.update_busy = update_busy(status);
        let over = s.parked && !s.update_busy;
        if over {
            s.parked = false;
        }
        (over, std::mem::take(&mut s.pending))
    };
    for (event, payload) in pending {
        if let Err(e) = app.emit_to(SETTINGS_WINDOW, &event, payload) {
            tracing::warn!(error = %e, event, "a waiting event did not reach the settings window");
        }
    }
    if park_over && let Some(w) = app.get_webview_window(SETTINGS_WINDOW) {
        let hidden = !w.is_visible().unwrap_or(true);
        if hidden {
            tracing::info!("update flow idle: closing the hidden settings window");
            let _ = w.destroy();
        }
    }
}

/// Emits `event` to the settings window, or, when it is being built,
/// once it has loaded (its first update report).
pub fn emit_to_settings<R: Runtime, S: serde::Serialize + Clone>(
    app: &AppHandle<R>,
    event: &str,
    payload: S,
    window_existed: bool,
) {
    if window_existed {
        if let Err(e) = app.emit_to(SETTINGS_WINDOW, event, payload) {
            tracing::warn!(error = %e, event, "the settings window did not hear an event");
        }
        return;
    }
    if let (Some(l), Ok(value)) = (lifecycle(app), serde_json::to_value(payload)) {
        lock(&l).pending.push((event.to_string(), value));
    }
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
    let window = tauri::WebviewWindowBuilder::from_config(app, &config)?.build()?;
    if crate::simulate::simulating(app) {
        window.set_title(&freewheel_core::simulator::title(
            "freewheel settings",
            true,
        ))?;
    }
    Ok(window)
}

/// Shows and focuses the settings window, building it (and in --simulate
/// the Simulator window) when it was closed, then runs `then` on it. Never
/// blocks the caller: the build runs on a thread of its own. Returns
/// whether the settings window was already there.
pub fn show_settings<R: Runtime>(
    app: &AppHandle<R>,
    then: impl FnOnce(&WebviewWindow<R>) + Send + 'static,
) -> bool {
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
        then(&window);
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
