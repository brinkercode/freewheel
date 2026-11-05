import { readFileSync } from "node:fs";
import { renderToString } from "react-dom/server";
import { expect, test } from "vitest";
import Permissions from "./Permissions.jsx";

test("a missing permission shows Needs permission and a button to its pane", () => {
  const html = renderToString(
    <Permissions initial={{ Accessibility: "Missing", InputMonitoring: "Granted" }} />,
  );
  for (const text of ["Permissions", "Needs permission", "Accessibility", "Open System Settings", "Input Monitoring", "✓ Allowed"]) {
    expect(html).toContain(text);
  }
});

test("all granted: no Needs permission", () => {
  const html = renderToString(
    <Permissions initial={{ Accessibility: "Granted", InputMonitoring: "Granted" }} />,
  );
  expect(html).not.toContain("Needs permission");
  expect(html).not.toContain("Open System Settings");
});

test("outside macOS (no permissions) the section is hidden", () => {
  expect(renderToString(<Permissions initial={{}} />)).toBe("");
});

test("the buttons call open_permission_pane and the section polls get_permissions", () => {
  const src = readFileSync(new URL("./Permissions.jsx", import.meta.url), "utf8");
  expect(src).toContain('invoke("open_permission_pane", { permission: name })');
  expect(src).toContain('invoke("get_permissions")');
  expect(src).not.toMatch(/fonts\.googleapis|fonts\.gstatic|Material\sSymbols|https?:\/\//);
});
