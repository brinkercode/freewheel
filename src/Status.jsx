import { useEffect, useState } from "react";
import { getStatus, onKeyPressed, onStatus } from "./settingsApi.js";

export const CONNECTION = {
  Connected: "Connected",
  Absent: "Not connected",
  Off: "Off",
  AuthFailed: "Could not authenticate",
};

// The device status and the last key pressed; `onPress` gets the pressed
// key's binding name so the Keys tab can select it.
export default function Status({ initial = null, initialPress = null, onPress }) {
  const [status, setStatus] = useState(initial);
  const [press, setPress] = useState(initialPress);
  useEffect(() => {
    if (initial) return undefined;
    getStatus().then(setStatus, () => {});
    const unlisten = [
      onStatus(setStatus),
      onKeyPressed((p) => {
        setPress(p);
        onPress?.(p.key);
      }),
    ];
    return () => unlisten.forEach((u) => u.then((stop) => stop(), () => {}));
  }, [initial, onPress]);

  if (!status) return null;
  const { battery } = status;
  const connection = CONNECTION[status.connection] ?? status.connection;
  const charge = battery
    ? ` · battery ${battery.level}%${battery.charging ? " (charging)" : ""}`
    : "";
  return (
    <section id="status">
      <p>{`Speed Editor: ${connection}${charge}`}</p>
      <p>{`Layer: ${status.layer}`}</p>
      {status.override_errors.map((e) => (
        <p key={`${e.mode}-${e.error}`} role="alert">
          {`${e.mode}: ${e.error}`}
        </p>
      ))}
      {press && <p>{`You pressed ${press.label}`}</p>}
    </section>
  );
}
