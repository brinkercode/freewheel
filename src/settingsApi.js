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
