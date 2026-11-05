import { renderToString } from "react-dom/server";
import { expect, test } from "vitest";
import Status from "./Status.jsx";

const status = {
  connection: "Connected",
  battery: { level: 64, charging: true },
  layer: "edit",
  override_errors: [{ mode: "Brightness", error: "DELL U2720Q: no DDC reply" }],
  permissions: { Accessibility: "Granted" },
};

test("the status shows connection, battery, layer and override errors", () => {
  const html = renderToString(<Status initial={status} />);
  for (const text of [
    "Speed Editor: Connected",
    "battery 64% (charging)",
    "Layer: edit",
    "Brightness: DELL U2720Q: no DDC reply",
  ]) {
    expect(html).toContain(text);
  }
});

test("a key press shows its printed label", () => {
  const html = renderToString(
    <Status initial={status} initialPress={{ key: "SmartInsrt", label: "SMART INSRT" }} />,
  );
  expect(html).toContain("You pressed SMART INSRT");
});

test("a lost connection reads Not connected without a battery", () => {
  const html = renderToString(
    <Status initial={{ ...status, connection: "Absent", battery: null, override_errors: [] }} />,
  );
  expect(html).toContain("Speed Editor: Not connected");
  expect(html).not.toContain("battery");
});
