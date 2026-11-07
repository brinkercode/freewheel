// The update source: freewheel's own portable updater in Rust
// (src-tauri/src/selfupdate.rs). It has the shape src/updater.js drives:
// check() resolves to null (up to date) or an update with version, body (the
// release notes), download(onEvent) (progress events; the package's
// signature is verified before it resolves) and install() (swaps the new
// version in and restarts; in `freewheel --simulate` the same Rust state
// machine runs a simulated source and install() resolves "simulated").

const tauri = () => globalThis.window.__TAURI__;

/** The Rust updater as an update source. */
export function rustUpdates(get = tauri) {
  const invoke = (command, args) => get().core.invoke(command, args);
  return {
    async check() {
      const found = await invoke("update_check");
      if (!found) return null;
      return {
        version: found.version,
        currentVersion: found.current_version,
        body: found.body,
        download(onEvent) {
          const channel = new (get().core.Channel)();
          channel.onmessage = onEvent;
          return invoke("update_download", { onEvent: channel });
        },
        install: () => invoke("update_install"),
      };
    },
  };
}

/** After a real install the app exits and the new version starts: nothing to do but wait. */
export const awaitRestart = () => new Promise(() => {});
