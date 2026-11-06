// The settings window shell's derived state: the toolbar status, the one
// notice the notice bar shows and the toast timing.

export const TABS = ["Keys", "Wheel", "DaVinci Resolve", "General", "Help & backup"];
export const LOW_BATTERY = 15;
export const TOAST_MS = 2800;

const BUS = { Usb: "USB", Bluetooth: "Bluetooth" };

/**
 * The shell's view of a StatusEvent (bindings.ts). Stepped aside for Resolve
 * the device loop is Off, yet freewheel is On and only lent the device to
 * Resolve, so it reads as On and connected and the status says Paused.
 */
export function fromStatusEvent(event) {
  const paused = event.paused ?? false;
  return {
    enabled: paused || event.connection !== "Off",
    permissionMissing: Object.values(event.permissions ?? {}).includes("Missing"),
    connected: paused || event.connection === "Connected",
    bus: event.bus ?? null,
    battery: event.battery?.level ?? null,
    paused,
  };
}

/** The toolbar status, first match wins. */
export function deriveStatus(s) {
  if (!s.enabled) return { text: "Off", dot: "gray" };
  if (s.permissionMissing) return { text: "Needs permission", dot: "orange" };
  if (!s.connected) return { text: "Looking for device", dot: "orange" };
  if (s.paused) return { text: "Paused (Resolve)", dot: "gray" };
  const battery = s.battery == null ? "" : ` · ${s.battery}%`;
  const bus = BUS[s.bus];
  return { text: bus ? `Connected (${bus}${battery})` : `Connected${battery}`, dot: "green" };
}

/** The one notice to show, or null; `action` names the button, if any. */
export function deriveNotice(s) {
  if (s.permissionMissing)
    return {
      id: "permission",
      title: "freewheel needs permission.",
      text: "Until you allow it, your keys can't do anything.",
      action: "Fix this",
    };
  if (!s.enabled)
    return { id: "off", title: "freewheel is Off.", text: "Your keys do nothing until you turn it on.", action: "Turn on" };
  if (!s.connected)
    return {
      id: "looking",
      title: "Looking for your Speed Editor.",
      text: "Plug it in with a USB cable, or switch it on to use Bluetooth.",
    };
  if (s.paused)
    return { id: "paused", title: "Paused for DaVinci Resolve.", text: "Resolve uses the Speed Editor while it's open." };
  if (s.battery != null && s.battery < LOW_BATTERY)
    return { id: "battery", title: "Battery low.", text: `${s.battery}% left. Plug in your Speed Editor to charge it.` };
  return null;
}

/**
 * Switches the running app On or Off through `setEnabled` (settingsApi.js,
 * the `set_enabled` command); resolves to the confirmation to toast.
 */
export async function switchOn(setEnabled, on) {
  await setEnabled(on);
  return on ? "freewheel is On." : "freewheel is Off.";
}

/** Shows a confirmation toast, then clears it after `ms`. Never for errors that need action. */
export function showToast(setToast, text, timers = globalThis, ms = TOAST_MS) {
  setToast(text);
  return timers.setTimeout(() => setToast(null), ms);
}
