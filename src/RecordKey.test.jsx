// @vitest-environment jsdom
// The Simulator's Record a key from another tab or the permission fix
// mounts the Keys tab with the key to record: the device's page (already
// known from the status) must not clear that key's panel.
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import KeysTab from "./KeysTab.jsx";

afterEach(cleanup);

const config = { version: 1, base_layer: "base", layers: [{ id: "base", name: "Main", bindings: {} }, { id: "edit", name: "Edit", bindings: {} }] };

test("Record a key opens CUT recording in a Keys tab that mounts with it; a later page switch on the device closes it", async () => {
  const status = { connection: "Connected", layer: "base", lit: [] };
  const { rerender } = render(<KeysTab initial={config} status={status} record={{ key: "Cut", n: 1 }} />);
  await waitFor(() => expect(document.querySelector(".side-panel .panel-title")?.textContent).toContain("CUT"));
  expect(document.querySelector(".side-panel .record-box")).not.toBeNull();
  // The device's page key: the tab follows it, the panel closes.
  rerender(<KeysTab initial={config} status={{ ...status, layer: "edit" }} record={{ key: "Cut", n: 1 }} />);
  await waitFor(() => expect(document.querySelector(".side-panel .panel-empty")).not.toBeNull());
});
