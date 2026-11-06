import { useEffect, useState } from "react";
import { getStatus, onStatus } from "./settingsApi.js";

export const CONNECTION = {
  Connected: "Connected",
  Absent: "Not connected",
  Off: "Off",
  AuthFailed: "Could not authenticate",
};

const MODE = { SystemVolume: "System volume", AppVolume: "App volume", Brightness: "Brightness" };

/**
 * The lines of one override error: brightness gives one line per display
 * (step_displays in overrides.rs joins them with "; ", A16), and app volume
 * reads 'permission needed' or 'not available on macOS yet'.
 */
export function overrideErrorLines({ mode, error }) {
  if (mode === "Brightness") return error.split("; ").map((e) => `Brightness: ${e}`);
  if (mode === "AppVolume" && /permission/i.test(error)) return ["App volume: permission needed"];
  if (mode === "AppVolume" && /not available/i.test(error)) return ["App volume: not available on macOS yet"];
  return [`${MODE[mode] ?? mode}: ${error}`];
}

// The device status; the Keys tab shows the last key pressed.
export default function Status({ initial = null }) {
  const [status, setStatus] = useState(initial);
  useEffect(() => {
    if (initial) return undefined;
    getStatus().then(setStatus, () => {});
    const unlisten = onStatus(setStatus);
    return () => unlisten.then((stop) => stop(), () => {});
  }, [initial]);

  if (!status) return null;
  const { battery } = status;
  const connection = CONNECTION[status.connection] ?? status.connection;
  const charge = battery
    ? ` · battery ${battery.level}%${battery.charging ? " (charging)" : ""}`
    : "";
  return (
    <section id="status">
      <p>{`Speed Editor: ${connection}${charge}`}</p>
      <p>{`Page: ${status.layer}`}</p>
      {status.override_errors.flatMap(overrideErrorLines).map((line) => (
        <p key={line} role="alert">
          {line}
        </p>
      ))}
    </section>
  );
}
