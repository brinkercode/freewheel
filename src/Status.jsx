import { useEffect, useState } from "react";
import { getStatus, onStatus } from "./settingsApi.js";

export const CONNECTION = {
  Connected: "Connected",
  Absent: "Not connected",
  Off: "Off",
  AuthFailed: "Could not authenticate",
};

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
      {status.override_errors.map((e) => (
        <p key={`${e.mode}-${e.error}`} role="alert">
          {`${e.mode}: ${e.error}`}
        </p>
      ))}
    </section>
  );
}
