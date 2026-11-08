// The settings window shell's derived state: the toolbar status, the one
// notice the notice bar shows and the toast timing.

export const TABS = ["Keys", "Wheel", "DaVinci Resolve", "General", "Help & backup"];
// The default low-battery threshold, for a status read before the config.
export const LOW_BATTERY = 15;
export const TOAST_MS = 2800;

const BUS = { Usb: "USB", Bluetooth: "Bluetooth" };

/**
 * The shell's view of a StatusEvent (bindings.ts). `enabled` is the app's
 * one On/Off (src-tauri settings.rs set_enabled): the tray's 'freewheel is
 * On' and this window's switch both set it, so each shows the other's
 * change. Stepped aside for Resolve the device loop is Off, yet freewheel is
 * On and only lent the device to Resolve, so it reads as On and connected
 * and the status says Paused.
 */
export function fromStatusEvent(event, lowBatteryAt = LOW_BATTERY) {
  const paused = event.paused ?? false;
  return {
    enabled: event.enabled ?? (paused || event.connection !== "Off"),
    permissionMissing: Object.values(event.permissions ?? {}).includes("Missing"),
    connected: paused || event.connection === "Connected",
    bus: event.bus ?? null,
    battery: event.battery?.level ?? null,
    lowBatteryAt,
    paused,
  };
}

// At or below settings.low_battery_threshold (SCHEMA.md low_battery_warning), as the tray.
const lowBattery = (s) => s.battery != null && s.battery <= (s.lowBatteryAt ?? LOW_BATTERY);

/** How the device is connected, in the prototype's words: "Connected by USB", "Connected by Bluetooth · 64%". */
export function connectedText(s) {
  const battery = s.battery == null ? "" : ` · ${s.battery}%`;
  const bus = BUS[s.bus];
  return bus ? `Connected by ${bus}${battery}` : `Connected${battery}`;
}

/** The header's short connected label: "Connected · USB", "Bluetooth · 64%" (the battery says it is wireless). */
export function connectedShort(s) {
  if (s.bus === "Bluetooth" && s.battery != null) return `Bluetooth · ${s.battery}%`;
  const bus = BUS[s.bus];
  if (bus) return `Connected · ${bus}`;
  return s.battery == null ? "Connected" : `Connected · ${s.battery}%`;
}

/**
 * The toolbar status, first match wins:
 * `text` is the full sentence (the tooltip, and the notice bar says it in
 * full too), `short` the label the header has room for at the default
 * 1000px width.
 */
export function deriveStatus(s) {
  if (!s.enabled) return { text: "Off: Resolve can use it", short: "Off", tone: "gray" };
  if (s.permissionMissing) return { text: "Needs permission (click to fix)", short: "Needs permission", tone: "orange" };
  if (!s.connected) return { text: "Looking for your Speed Editor…", short: "Looking for device", tone: "gray" };
  if (s.paused) return { text: "Paused: DaVinci Resolve is open", short: "Paused for Resolve", tone: "gray" };
  return { text: connectedText(s), short: connectedShort(s), tone: lowBattery(s) ? "orange" : "green" };
}

/** The one notice to show, or null; `action` names the button, if any. */
export function deriveNotice(s) {
  if (s.permissionMissing)
    return {
      id: "permission",
      title: "freewheel needs your permission.",
      text: "Without it, your Speed Editor can’t press keys for you.",
      action: "Fix this",
    };
  if (!s.enabled)
    return { id: "off", title: "freewheel is Off.", text: "Your Speed Editor is free for DaVinci Resolve.", action: "Turn on" };
  if (!s.connected)
    return {
      id: "looking",
      title: "Looking for your Speed Editor…",
      text: "Plug it in, or switch it on to connect by Bluetooth. You can still set up keys.",
    };
  if (s.paused)
    return { id: "paused", title: "Paused while DaVinci Resolve is open.", text: "Your shortcuts come back when you close Resolve." };
  if (lowBattery(s))
    return {
      id: "battery",
      title: `Battery is low (${s.battery}%).`,
      text: "Plug in the USB cable to charge. It keeps working while it charges.",
    };
  return null;
}

/**
 * Switches the running app On or Off through `setEnabled` (settingsApi.js,
 * the `set_enabled` command); resolves to the confirmation to toast.
 */
export async function switchOn(setEnabled, on) {
  await setEnabled(on);
  return on ? "freewheel is On. Your Speed Editor runs your shortcuts." : "freewheel is Off. DaVinci Resolve can use your Speed Editor.";
}

// How the found device is attached, for the first run's 'Found it!'.
export function connectionText(bus, battery = null) {
  return connectedText({ bus, battery });
}

// The first run shows until finished; the webview's storage keeps the flag.
export const FIRST_RUN_DONE = "freewheel.first-run-done";
export const firstRunDone = (storage) => storage?.getItem(FIRST_RUN_DONE) === "1";
export const finishFirstRun = (storage) => storage?.setItem(FIRST_RUN_DONE, "1");

/** Shows a confirmation toast, then clears it after `ms`. Never for errors that need action. */
export function showToast(setToast, text, timers = globalThis, ms = TOAST_MS) {
  setToast(text);
  return timers.setTimeout(() => setToast(null), ms);
}
