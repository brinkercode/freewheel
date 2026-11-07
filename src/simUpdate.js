// `freewheel --simulate` only: a simulated update source with the plugin's
// shape (check() resolving to an update with version, body, download(onEvent)
// and install()), so the Simulator's "New version" and the General tab's
// Check / Download / Restart go through the same states and screens as a real
// update (src/updater.js). Restarting "installs" the new version in place:
// the settings window then shows that version and What's new for it. Nothing
// here is used outside --simulate (src/App.jsx picks it only when
// simulating()).
import { compareVersions } from "./whatsNew.js";

/** How long the simulated check takes, like the prototype's 1.3 s. */
export const SIM_CHECK_MS = 1500;
/** How long the simulated download takes, like the prototype's. */
export const SIM_DOWNLOAD_MS = 3200;
/** How long the simulated restart takes, like the prototype's. */
export const SIM_RESTART_MS = 1600;
/** The simulated installer's size: progress is counted against it. */
export const SIM_SIZE = 8_400_000;
const CHUNKS = 32;

/** The release notes the simulated update carries, release-please style. */
export const SIM_NOTES = `## [{v}](https://github.com/brinkercode/freewheel/compare) (simulated)

### Features

* **wheel:** smoother scrolling when you turn the wheel fast
* **keys:** a shortcut can now open a folder

### Bug Fixes

* the Speed Editor reconnects by itself after your computer wakes up
`;

/** The next minor version: "0.1.0" -> "0.2.0". */
export function nextVersion(version) {
  const [major = 0, minor = 0] = String(version ?? "0.0.0")
    .replace(/^v/, "")
    .split("-")[0]
    .split(".")
    .map((n) => Number.parseInt(n, 10) || 0);
  return `${major}.${minor + 1}.0`;
}

function simulatedUpdate(version, current, timers, downloadMs) {
  return {
    version,
    currentVersion: current,
    body: SIM_NOTES.replace("{v}", version),
    download(onEvent) {
      return new Promise((resolve) => {
        onEvent({ event: "Started", data: { contentLength: SIM_SIZE } });
        let sent = 0;
        const timer = timers.setInterval(() => {
          sent++;
          onEvent({ event: "Progress", data: { chunkLength: SIM_SIZE / CHUNKS } });
          if (sent >= CHUNKS) {
            timers.clearInterval(timer);
            onEvent({ event: "Finished" });
            resolve();
          }
        }, downloadMs / CHUNKS);
      });
    },
    install: async () => {},
  };
}

/**
 * The simulated update server: nothing published until publish(); check(current)
 * finds the published version when it is newer than `current`.
 */
export function createSimUpdates({ timers = globalThis, downloadMs = SIM_DOWNLOAD_MS, checkMs = SIM_CHECK_MS } = {}) {
  let published = null;
  return {
    /** Publishes a version newer than `current` (and than the last one published); returns it. */
    publish(current) {
      const base = published && compareVersions(published, current) > 0 ? published : current;
      published = nextVersion(base);
      return published;
    },
    published: () => published,
    async check(current) {
      await new Promise((r) => timers.setTimeout(r, checkMs));
      if (!published || compareVersions(published, current) <= 0) return null;
      return simulatedUpdate(published, current, timers, downloadMs);
    },
  };
}

/** The simulated relaunch: after SIM_RESTART_MS, `onRestarted()` (the window then runs the new version). */
export function simRelaunch(onRestarted, { timers = globalThis, ms = SIM_RESTART_MS } = {}) {
  return new Promise((resolve) =>
    timers.setTimeout(() => {
      onRestarted();
      resolve();
    }, ms),
  );
}
