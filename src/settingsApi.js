// The settings window's bridge to src-tauri (settings.rs). Types: bindings.ts,
// generated from the Rust serde types.
const tauri = () => window.__TAURI__;
const invoke = (cmd, args) => tauri().core.invoke(cmd, args);

export const STATUS_EVENT = "status";
export const KEY_PRESSED_EVENT = "key-pressed";

/** @returns {Promise<import("./bindings").Config>} */
export const getConfig = () => invoke("get_config");

/**
 * Validates, saves and applies the config to the running engine.
 * Rejects with an import("./bindings").SaveError.
 */
export const saveConfig = (config) => invoke("save_config", { config });

/** @returns {Promise<import("./bindings").StatusEvent>} */
export const getStatus = () => invoke("get_status");

// Opens the OS Bluetooth settings. Rejects with a string when it cannot.
export const openBluetoothSettings = () => invoke("open_bluetooth_settings");

// The tray's 'Needs permission (click to fix)' line asks for the permission fix.
export const FIX_PERMISSIONS_EVENT = "fix-permissions";

// Calls `onFix` when the tray asks for the permission fix; resolves to the unlisten function.
export const onFixPermissions = (onFix) => tauri().event.listen(FIX_PERMISSIONS_EVENT, () => onFix());

/**
 * Switches freewheel On or Off in the running app, as the tray toggle does;
 * the next StatusEvent shows it. Rejects with a string when it cannot.
 */
export const setEnabled = (on) => invoke("set_enabled", { on });

/** Calls `onChange` with every StatusEvent; resolves to the unlisten function. */
export const onStatus = (onChange) =>
  tauri().event.listen(STATUS_EVENT, (event) => onChange(event.payload));

/** Calls `onPress` with every KeyPressed; resolves to the unlisten function. */
export const onKeyPressed = (onPress) =>
  tauri().event.listen(KEY_PRESSED_EVENT, (event) => onPress(event.payload));

/** What the UI says about a rejected save. */
export function saveErrorText(error) {
  switch (error?.kind) {
    case "VersionMismatch":
      return `Config version ${error.found} is not ${error.expected}.`;
    case "UnknownField":
      return `Unknown setting ${error.path}.`;
    case "UnknownKey":
      return `Layer ${error.layer} binds ${error.key}, which is not a Speed Editor key.`;
    case "BadLayerReference":
      return `${error.location} refers to layer ${error.target}, which does not exist.`;
    case "Malformed":
      return `The config is not valid: ${error.message}`;
    case "SaveFailed":
      return `Saving failed: ${error.message}`;
    default:
      return String(error);
  }
}
