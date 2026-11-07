// The settings window's bridge to src-tauri (settings.rs). Types: bindings.ts,
// generated from the Rust serde types.
const tauri = () => window.__TAURI__;
const invoke = (cmd, args) => tauri().core.invoke(cmd, args);

export const STATUS_EVENT = "status";
export const KEY_PRESSED_EVENT = "key-pressed";
export const DEVICE_DIAGNOSTICS_EVENT = "device-diagnostics";

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

// The tray's 'Check for updates' / 'Restart to update' item asks the window's update flow (src/updater.js).
export const TRAY_UPDATE_EVENT = "tray-update";

/** Calls `onAction("check" | "download" | "restart")` when the tray's update item is clicked; resolves to the unlisten function. */
export const onTrayUpdate = (onAction) => tauri().event.listen(TRAY_UPDATE_EVENT, (event) => onAction(event.payload));

/** Tells the tray the update state (src/updater.js; null is idle), so its item says Check for updates, Restart to update… */
export const reportUpdateState = (state) => invoke("tray_update_state", { state: state ?? null });

// The tray's 'Download logs for support…' saved the bundle: { path } or { error }.
export const LOGS_SAVED_EVENT = "logs-saved";

/** Calls `onSaved({ path } | { error })` when the tray saved the logs; resolves to the unlisten function. */
export const onLogsSaved = (onSaved) => tauri().event.listen(LOGS_SAVED_EVENT, (event) => onSaved(event.payload));

/**
 * Switches freewheel On or Off in the running app, as the tray toggle does;
 * the next StatusEvent shows it. Rejects with a string when it cannot.
 */
export const setEnabled = (on) => invoke("set_enabled", { on });

/** @returns {Promise<boolean>} whether the OS login item is registered now. */
export const getStartAtLogin = () => invoke("get_start_at_login");

/** Sets the OS login item and the config; resolves to what the OS has now. */
export const setStartAtLogin = (on) => invoke("set_start_at_login", { on });

/** @returns {Promise<string>} the app's version. */
export const getVersion = () => invoke("app_version");

/**
 * Asks for a file in a save dialog and writes freewheel-mappings.json to it.
 * Resolves to false when the user cancelled; rejects with a SaveError.
 */
export const exportMappings = () => invoke("export_mappings");

/**
 * Asks for a mappings file, validates it, asks "Replace your keys?", backs up
 * config.json and replaces the mappings. Imported keys that open apps stay off
 * unless `allowLaunch`. Resolves to false when cancelled; rejects with a SaveError.
 */
export const importMappings = (allowLaunch) => invoke("import_mappings", { allowLaunch });

/**
 * Saves the support log bundle (every log file and a redacted config
 * summary, no shortcuts or paths) to the Downloads folder, as the tray's
 * 'Download logs for support...' does. Resolves to the file's absolute path;
 * rejects with a string when it cannot.
 * @returns {Promise<string>}
 */
export const exportLogs = () => invoke("export_logs");

/**
 * Help & backup's Detailed logs switch: saves settings.detailed_logs and
 * raises (DEBUG, auto-off after 24 hours) or lowers the log level now.
 * Rejects with a string when the setting cannot be saved.
 * @returns {Promise<void>}
 */
export const setDetailedLogs = (on) => invoke("set_detailed_logs", { on });

/**
 * Help & backup's 'Show file': the log bundle saved last, selected in its
 * folder (Explorer, the Finder). Rejects with a string when it is gone.
 * @returns {Promise<void>}
 */
export const showLogsFile = () => invoke("show_logs_file");

/** The Wheel tab's 'Back to normal': leaves the dial override a key turned on. */
export const exitWheelOverride = () => invoke("exit_wheel_override");

/**
 * The installed apps for the key panel (Start Menu shortcuts on Windows,
 * .app bundles on macOS), each as the LaunchApp to save.
 * @returns {Promise<{name: string, path: string, args: string[]}[]>}
 */
export const listApps = () => invoke("list_apps");

/**
 * 'Choose file…': the OS file dialog for a program; resolves to
 * { name, path, args } or null when cancelled.
 */
export const chooseApp = () => invoke("choose_app");

/**
 * The installed apps' real icons (src-tauri app_icons.rs): `paths` (each
 * app's .lnk / .exe / .app) → PNG data URL; apps without one are left out.
 * @returns {Promise<Record<string, string>>}
 */
export const appIcons = (paths) => invoke("app_icons", { paths });

/** What the UI says about a rejected import. */
export function importErrorText(error) {
  if (error?.kind === "VersionMismatch" && error.found > error.expected)
    return "This file is from a newer version of freewheel. Update freewheel, then try again.";
  if (error?.kind === "Malformed") return `This is not a freewheel mappings file: ${error.message}`;
  // A mistake in a hand-written file: where it is and what is wrong (SCHEMA.md).
  if (error?.kind === "InvalidFile") return `This file can't be restored: ${fileErrorText(error)}`;
  return saveErrorText(error);
}

/** "line 7, column 15, CAM1 on Main: unknown action …", as core SaveError's Display. */
export function fileErrorText({ line, column, at, message }) {
  return `line ${line}, column ${column}${at ? `, ${at}` : ""}: ${message}`;
}

/** Calls `onChange` with every StatusEvent; resolves to the unlisten function. */
export const onStatus = (onChange) =>
  tauri().event.listen(STATUS_EVENT, (event) => onChange(event.payload));

/** @returns {Promise<import("./bindings").DeviceDiagnostics>} the Device test view's data now. */
export const getDeviceDiagnostics = () => invoke("get_device_diagnostics");

/** Calls `onChange` with every DeviceDiagnostics; resolves to the unlisten function. */
export const onDeviceDiagnostics = (onChange) =>
  tauri().event.listen(DEVICE_DIAGNOSTICS_EVENT, (event) => onChange(event.payload));

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
      return `Page ${error.layer} uses ${error.key}, which is not a Speed Editor key.`;
    case "BadLayerReference":
      return `${error.location} refers to page ${error.target}, which does not exist.`;
    case "Malformed":
      return `The config is not valid: ${error.message}`;
    case "SaveFailed":
      return `Saving failed: ${error.message}`;
    default:
      return String(error);
  }
}
