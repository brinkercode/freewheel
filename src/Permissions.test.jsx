import { readFileSync } from "node:fs";
import { renderToString } from "react-dom/server";
import { expect, test, vi } from "vitest";
import Permissions, { BUTTON, PermissionsScreen, leadFor, watch } from "./Permissions.jsx";

test("the status is checked again when the window regains focus, until unmounted", () => {
  vi.useFakeTimers();
  const window = new EventTarget();
  const check = vi.fn();
  const stop = watch(check, window, 60_000);
  expect(check).toHaveBeenCalledTimes(1);
  window.dispatchEvent(new Event("focus"));
  expect(check).toHaveBeenCalledTimes(2);
  stop();
  window.dispatchEvent(new Event("focus"));
  vi.advanceTimersByTime(60_000);
  expect(check).toHaveBeenCalledTimes(2);
  vi.useRealTimers();
});

test("a missing permission shows Needs permission and a button to its pane", () => {
  const html = renderToString(
    <Permissions initial={{ Accessibility: "Missing", InputMonitoring: "Granted" }} />,
  );
  // "✓ Allowed" became the bundled check_circle icon with "Allowed" (card item 12)
  for (const text of ["Permissions", "Needs permission", "Accessibility", "Open System Settings", "Input Monitoring", "Allowed"]) {
    expect(html).toContain(text);
  }
  expect(html).toMatch(/data-icon="check_circle"[^>]*><svg[\s\S]*?<\/span> <span>Allowed<\/span>/);
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

test("Linux's one permission is the device rule, installed with one button", () => {
  const html = renderToString(<Permissions initial={{ DeviceAccess: "Missing" }} />);
  for (const text of ["Needs permission", "Allow freewheel to use your Speed Editor", "the device rule", "Install the device rule"]) expect(html).toContain(text);
  expect(html).not.toContain("Open System Settings");
  const screen = renderToString(<PermissionsScreen initial={{ DeviceAccess: "Missing" }} button="Done" />);
  expect(screen).toContain("Linux asks for your password once");
  expect(screen).toContain("Install the device rule");
  expect(BUTTON("Accessibility")).toBe("Open System Settings");
  expect(leadFor(["Accessibility", "InputMonitoring"])).toMatch(/^macOS asks you/);
});
