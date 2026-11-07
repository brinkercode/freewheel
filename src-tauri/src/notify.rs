//! OS notifications, and when to show them.
//!
//! The design calls for three, each at most once per occasion:
//! - **low battery**: the Speed Editor's battery drops below the threshold
//!   (`settings.low_battery_threshold`, 15 % by default), again only after it
//!   charged back up ([`freewheel_core::battery::LowBatteryAlert`]);
//! - **stepped aside**: freewheel switched itself Off because DaVinci Resolve
//!   opened (once per time Resolve opens);
//! - **update ready**: a new version is downloaded and waits for a restart
//!   (once per version, and not while the settings window is in front: its
//!   banner already says so).
//!
//! The design has no setting for notifications, so there is none. Device
//! connected / lost are not notified (the design does not ask for it: the tray
//! icon and the status line show it). In `--simulate` every notification, and
//! every one skipped, is also a line in the Simulator's log.

use freewheel_core::resolve::REPLUG_NOTIFICATION;
use tauri::{AppHandle, Runtime};
use tauri_plugin_notification::NotificationExt;

/// One OS notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
}

impl Notice {
    fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Notice {
            title: title.into(),
            body: body.into(),
        }
    }
}

/// The low-battery notification at `level` %.
pub fn low_battery(level: u8) -> Notice {
    Notice::new(
        "Speed Editor battery low",
        format!("{level}% left. Charge it soon."),
    )
}

/// The notification when freewheel stepped aside for DaVinci Resolve.
pub fn stepped_aside() -> Notice {
    Notice::new(freewheel_core::APP_NAME, REPLUG_NOTIFICATION)
}

/// The notification when `version` is downloaded and ready.
pub fn update_ready(version: &str) -> Notice {
    Notice::new(
        format!("freewheel {version} is ready"),
        "Restart to update: choose Restart to update in the freewheel menu.",
    )
}

/// What to notify for app-wide changes: the step-aside and the update, each
/// once per occasion. (The battery has its own alert per device loop.)
#[derive(Debug, Default)]
pub struct Notifier {
    paused: bool,
    update_notified: Option<String>,
}

impl Notifier {
    /// Whether freewheel stepped aside for Resolve now; notifies on the
    /// change from not paused to paused.
    pub fn paused(&mut self, paused: bool) -> Option<Notice> {
        let started = paused && !self.paused;
        self.paused = paused;
        started.then(stepped_aside)
    }

    /// The settings window's update state: notifies once per version when it
    /// becomes ready. `window_in_front`: the banner already shows it, so the
    /// version counts as told and no notification is shown.
    pub fn update(
        &mut self,
        status: Option<&str>,
        version: Option<&str>,
        window_in_front: bool,
    ) -> Option<Notify> {
        if status == Some("restarting") {
            // A real restart is a new process with nothing notified yet; the
            // simulated one (--simulate) restarts in place, so forget here.
            self.update_notified = None;
            return None;
        }
        let (Some("ready"), Some(version)) = (status, version) else {
            return None;
        };
        if self.update_notified.as_deref() == Some(version) {
            return None;
        }
        self.update_notified = Some(version.to_string());
        let notice = update_ready(version);
        Some(if window_in_front {
            Notify::Skipped(notice)
        } else {
            Notify::Show(notice)
        })
    }
}

/// A notification decision that is worth a log line either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notify {
    Show(Notice),
    /// Not shown: the settings window is in front and says the same.
    Skipped(Notice),
}

/// The Simulator log line for a notification (shown or skipped).
pub fn log_line(notice: &Notice, shown: bool) -> String {
    if shown {
        format!("Notification: {} — {}", notice.title, notice.body)
    } else {
        format!(
            "Notification skipped (the settings window shows it): {} — {}",
            notice.title, notice.body
        )
    }
}

/// Shows `notice` as an OS notification (and logs it in `--simulate`).
pub fn show<R: Runtime>(app: &AppHandle<R>, notice: &Notice) {
    crate::simulate::note(app, log_line(notice, true));
    tracing::info!(title = %notice.title, "notification");
    let shown = app
        .notification()
        .builder()
        .title(&notice.title)
        .body(&notice.body)
        .show();
    if let Err(e) = shown {
        tracing::warn!(error = %e, "notification failed");
    }
}

/// Shows or, when skipped, only logs the decision.
pub fn apply<R: Runtime>(app: &AppHandle<R>, decision: Notify) {
    match decision {
        Notify::Show(notice) => show(app, &notice),
        Notify::Skipped(notice) => crate::simulate::note(app, log_line(&notice, false)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freewheel_core::battery::{Battery, LowBatteryAlert};

    #[test]
    fn low_battery_names_the_level_once_per_drop() {
        let mut alert = LowBatteryAlert::default();
        let at = |level| Battery {
            level,
            charging: false,
        };
        let mut shown = Vec::new();
        for level in [64, 20, 14, 12, 9, 50, 13] {
            if alert.update(at(level), 15) {
                shown.push(low_battery(level).body);
            }
        }
        assert_eq!(
            shown,
            ["14% left. Charge it soon.", "13% left. Charge it soon."]
        );
        assert_eq!(low_battery(14).title, "Speed Editor battery low");
    }

    #[test]
    fn stepping_aside_notifies_once_each_time_resolve_opens() {
        let mut n = Notifier::default();
        assert_eq!(n.paused(false), None);
        assert_eq!(n.paused(true), Some(stepped_aside()));
        assert_eq!(n.paused(true), None);
        assert_eq!(n.paused(false), None);
        assert_eq!(n.paused(true), Some(stepped_aside()));
        assert!(stepped_aside().body.contains("DaVinci Resolve started"));
    }

    #[test]
    fn update_ready_notifies_once_per_version() {
        let mut n = Notifier::default();
        for status in [None, Some("checking"), Some("downloading"), Some("current")] {
            assert_eq!(n.update(status, Some("0.2.0"), false), None, "{status:?}");
        }
        assert_eq!(
            n.update(Some("ready"), Some("0.2.0"), false),
            Some(Notify::Show(update_ready("0.2.0")))
        );
        assert_eq!(n.update(Some("ready"), Some("0.2.0"), false), None);
        assert_eq!(n.update(Some("downloading"), Some("0.2.0"), false), None);
        assert_eq!(n.update(Some("ready"), Some("0.2.0"), false), None);
        assert_eq!(
            n.update(Some("ready"), Some("0.3.0"), true),
            Some(Notify::Skipped(update_ready("0.3.0")))
        );
        assert_eq!(n.update(Some("ready"), Some("0.3.0"), false), None);
        // After a restart (a new process for real, in place in --simulate)
        // the same version is news again.
        assert_eq!(n.update(Some("restarting"), Some("0.3.0"), false), None);
        assert_eq!(
            n.update(Some("ready"), Some("0.3.0"), false),
            Some(Notify::Show(update_ready("0.3.0")))
        );
        assert_eq!(n.update(Some("ready"), None, false), None);
        assert_eq!(update_ready("0.2.0").title, "freewheel 0.2.0 is ready");
    }

    #[test]
    fn the_log_line_says_whether_it_was_shown() {
        let n = low_battery(12);
        assert_eq!(
            log_line(&n, true),
            "Notification: Speed Editor battery low — 12% left. Charge it soon."
        );
        assert!(log_line(&n, false).starts_with("Notification skipped"));
    }
}
