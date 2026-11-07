// @vitest-environment jsdom
import { cleanup, configure, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "./App.jsx";
import { sampleConfig, sampleStatus } from "./dev/mockTauri.js";
import { restart } from "./updater.js";
import { notesText, parseInline, parseNotes, safeHref } from "./notesMarkdown.js";
import {
  LAST_SEEN_VERSION,
  UPDATE_NOTES,
  compareVersions,
  markSeen,
  releasePage,
  rememberNotes,
  whatsNew,
} from "./whatsNew.js";

// What latest.json carries: release-please's notes after scripts/release-notes.mjs filter.
const NOTES = [
  "## [0.1.1](https://github.com/brinkercode/freewheel/compare/v0.1.0...v0.1.1) (2026-11-07)",
  "",
  "### Features",
  "",
  "* **keys:** a key can open a folder, like `C:\\Music` ([#131](https://github.com/brinkercode/freewheel/issues/131))",
  "",
  "### Bug Fixes",
  "",
  "* a held Shift is released when freewheel turns Off",
  "* the wheel scrolls again after sleep",
].join("\n");

/** The webview's localStorage, in memory. */
function memoryStorage(items = {}) {
  const map = new Map(Object.entries(items));
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, String(v)),
    removeItem: (k) => map.delete(k),
    map,
  };
}

const updatedFrom = (from, to, notes = NOTES) =>
  memoryStorage({
    "freewheel.first-run-done": "1",
    [LAST_SEEN_VERSION]: from,
    [UPDATE_NOTES]: JSON.stringify({ version: to, notes }),
  });

let invoke;
/** The Tauri bridge of a build running `version`. */
function fakeTauri(version) {
  invoke = vi.fn(async (cmd) => {
    switch (cmd) {
      case "app_version":
        return version;
      case "get_config":
        return sampleConfig();
      case "get_status":
        return sampleStatus();
      case "plugin:updater|check":
        return null;
      default:
        return null;
    }
  });
  window.__TAURI__ = { core: { invoke }, event: { listen: async () => () => {} } };
  window.__TAURI_INTERNALS__ = { invoke, transformCallback: () => 1, metadata: {} };
}

// The first render of the whole window is slow on cold CI runners.
configure({ asyncUtilTimeout: 10000 });

beforeEach(() => fakeTauri("0.1.1"));
afterEach(() => {
  cleanup();
  delete window.__TAURI__;
  delete window.__TAURI_INTERNALS__;
});

const button = () => screen.queryByRole("button", { name: "What’s new in v0.1.1" });

test("after an update into a newer version the window offers What's new in vX.Y.Z", async () => {
  render(<App storage={updatedFrom("0.1.0", "0.1.1")} />);
  await waitFor(() => expect(button()).not.toBeNull());
  expect(screen.getByText("freewheel is updated to 0.1.1")).toBeTruthy();
  expect(button().className).toBe("btn-sm btn-primary");
});

test("clicking it opens the notes as markdown (headings, bullets, bold, code, links), linked to the tag's release page", async () => {
  const storage = updatedFrom("0.1.0", "0.1.1");
  const { container } = render(<App storage={storage} />);
  await waitFor(() => expect(button()).not.toBeNull());
  fireEvent.click(button());
  expect(screen.getByRole("heading", { name: "What’s new in v0.1.1" })).toBeTruthy();
  // The version heading is the page's title; the sections are headings.
  expect(screen.getByRole("heading", { level: 4, name: "Features" })).toBeTruthy();
  expect(screen.getByRole("heading", { level: 4, name: "Bug Fixes" })).toBeTruthy();
  expect(container.textContent).not.toContain("2026-11-07");
  const notes = container.querySelector(".whats-new-notes");
  expect([...notes.querySelectorAll("li")].map((li) => li.textContent)).toEqual([
    "keys: a key can open a folder, like C:\\Music (#131)",
    "a held Shift is released when freewheel turns Off",
    "the wheel scrolls again after sleep",
  ]);
  expect(notes.querySelector("li strong").textContent).toBe("keys:");
  expect(notes.querySelector("li code.whats-new-code").textContent).toBe("C:\\Music");
  // A notes link opens in the browser through the opener, not in the window.
  const pr = screen.getByRole("link", { name: "#131" });
  expect(pr.getAttribute("href")).toBe("https://github.com/brinkercode/freewheel/issues/131");
  fireEvent.click(pr);
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("plugin:opener|open_url", { url: "https://github.com/brinkercode/freewheel/issues/131" }),
  );
  const link = screen.getByRole("link", { name: "See this release on GitHub" });
  expect(link.getAttribute("href")).toBe("https://github.com/brinkercode/freewheel/releases/tag/v0.1.1");
  fireEvent.click(link);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("open_release_page", { version: "0.1.1" }));
  // The notice is gone while the notes are open, and Done returns to the tabs.
  expect(button()).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Done" }));
  expect(screen.queryByRole("heading", { name: "What’s new in v0.1.1" })).toBeNull();
  expect(button()).toBeNull();
  expect(storage.getItem(LAST_SEEN_VERSION)).toBe("0.1.1");
});

test("notes are never HTML: tags stay text, scripts never run, only GitHub links are links", async () => {
  const evil = [
    "### <img src=x onerror=alert(1)>Fixes",
    "",
    "* <script>window.__pwned = 1</script> plain",
    "* [click](javascript:alert(1)) and [elsewhere](https://evil.example/x) and [ok](https://github.com/brinkercode/freewheel/pull/1)",
    "<!-- a comment -->",
    "<b>not bold</b>",
  ].join("\n");
  const { container } = render(<App storage={updatedFrom("0.1.0", "0.1.1", evil)} />);
  await waitFor(() => expect(button()).not.toBeNull());
  fireEvent.click(button());
  const notes = container.querySelector(".whats-new-notes");
  expect(notes.querySelector("img, script, b")).toBeNull();
  expect(window.__pwned).toBeUndefined();
  expect(notes.querySelector("h4").textContent).toBe("<img src=x onerror=alert(1)>Fixes");
  expect(notes.textContent).toContain("<script>window.__pwned = 1</script> plain");
  expect(notes.textContent).toContain("<b>not bold</b>");
  expect(notes.textContent).not.toContain("a comment");
  expect([...notes.querySelectorAll("a")].map((a) => a.getAttribute("href"))).toEqual(["https://github.com/brinkercode/freewheel/pull/1"]);
});

test("long notes scroll inside the notes card", async () => {
  const long = ["### Features", "", ...Array.from({ length: 80 }, (_, i) => `* change ${i + 1}`)].join("\n");
  const { container } = render(<App storage={updatedFrom("0.1.0", "0.1.1", long)} />);
  await waitFor(() => expect(button()).not.toBeNull());
  fireEvent.click(button());
  expect(container.querySelectorAll(".whats-new-notes li")).toHaveLength(80);
  const css = (await import("node:fs")).readFileSync(`${process.cwd()}/src/styles/base.css`, "utf8");
  expect(css).toMatch(/\.whats-new-notes \{[^}]*max-height:[^}]*overflow-y: auto;/);
});

test("a tab click leaves the notes for that tab", async () => {
  render(<App storage={updatedFrom("0.1.0", "0.1.1")} />);
  await waitFor(() => expect(button()).not.toBeNull());
  fireEvent.click(button());
  fireEvent.click(screen.getByRole("button", { name: "General" }));
  expect(screen.queryByRole("heading", { name: "What’s new in v0.1.1" })).toBeNull();
  expect(screen.getByRole("heading", { name: "General" })).toBeTruthy();
  expect(button()).toBeNull();
});

test("a first install shows nothing and records the version", async () => {
  const storage = memoryStorage({ "freewheel.first-run-done": "1" });
  render(<App storage={storage} />);
  await waitFor(() => expect(storage.getItem(LAST_SEEN_VERSION)).toBe("0.1.1"));
  expect(button()).toBeNull();
});

test("it shows once: after Dismiss (or opening it) the next start shows nothing", async () => {
  const storage = updatedFrom("0.1.0", "0.1.1");
  render(<App storage={storage} />);
  await waitFor(() => expect(button()).not.toBeNull());
  fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
  expect(button()).toBeNull();
  cleanup();
  render(<App storage={storage} />);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("app_version", undefined));
  await new Promise((r) => setTimeout(r, 20));
  expect(button()).toBeNull();
  expect(storage.getItem(UPDATE_NOTES)).toBeNull();
});

test("the same version and a downgrade show nothing", () => {
  expect(whatsNew(updatedFrom("0.1.1", "0.1.1"), "0.1.1")).toBeNull();
  const down = updatedFrom("0.2.0", "0.1.1");
  expect(whatsNew(down, "0.1.1")).toBeNull();
  expect(down.getItem(LAST_SEEN_VERSION)).toBe("0.1.1");
});

test("without kept notes (an update from outside the window) it still links to GitHub", () => {
  const storage = memoryStorage({ [LAST_SEEN_VERSION]: "0.1.0" });
  expect(whatsNew(storage, "0.1.1")).toEqual({
    version: "0.1.1",
    notes: null,
    url: "https://github.com/brinkercode/freewheel/releases/tag/v0.1.1",
  });
  // Notes kept for another version are not shown for this one.
  expect(whatsNew(updatedFrom("0.1.0", "0.1.2"), "0.1.1").notes).toBeNull();
  markSeen(storage, "0.1.1");
  expect(whatsNew(storage, "0.1.1")).toBeNull();
});

test("Restart to update keeps the update's notes before installing it", async () => {
  const storage = memoryStorage();
  const order = [];
  const update = {
    version: "0.1.1",
    body: NOTES,
    install: vi.fn(async () => order.push(["install", storage.getItem(UPDATE_NOTES)])),
  };
  const relaunch = vi.fn(async () => order.push(["relaunch"]));
  await restart(update, relaunch, () => {}, (u) => rememberNotes(storage, u));
  expect(order).toEqual([["install", JSON.stringify({ version: "0.1.1", notes: NOTES })], ["relaunch"]]);
});

test("the markdown subset: headings, wrapped bullets, paragraphs, inline bold, code and links", () => {
  expect(parseNotes(NOTES).map((b) => b.type)).toEqual(["h3", "list", "h3", "list"]);
  expect(notesText(NOTES)).toEqual([
    "Features",
    "keys: a key can open a folder, like C:\\Music (#131)",
    "Bug Fixes",
    "a held Shift is released when freewheel turns Off",
    "the wheel scrolls again after sleep",
  ]);
  expect(notesText("## What's Changed\n\n* one\n  two\n- three\n\nA paragraph\nwrapped.")).toEqual(["What's Changed", "one two", "three", "A paragraph wrapped."]);
  expect(parseNotes("## 0.1.0 (2026-11-07)\n\n### Bug Fixes")).toEqual([{ type: "h3", inline: [{ text: "Bug Fixes" }] }]);
  expect(parseInline("**a `b` [c](https://github.com/x)** d")).toEqual([
    { bold: [{ text: "a " }, { code: "b" }, { text: " " }, { link: "c", href: "https://github.com/x" }] },
    { text: " d" },
  ]);
  expect(safeHref("https://github.com.evil.example/")).toBeNull();
  expect(safeHref("http://github.com/x")).toBeNull();
  expect(notesText("")).toEqual([]);
  expect(notesText(null)).toEqual([]);
});

test("versions compare as semver and the release page is the v-tag's", () => {
  expect(compareVersions("0.1.1", "0.1.0")).toBeGreaterThan(0);
  expect(compareVersions("0.10.0", "0.9.9")).toBeGreaterThan(0);
  expect(compareVersions("1.0.0", "1.0.0-beta.1")).toBeGreaterThan(0);
  expect(compareVersions("0.1.0", "0.1.0")).toBe(0);
  expect(compareVersions("0.1.0", "0.2.0")).toBeLessThan(0);
  expect(releasePage("0.1.1")).toBe("https://github.com/brinkercode/freewheel/releases/tag/v0.1.1");
  expect(releasePage("v0.1.1")).toBe("https://github.com/brinkercode/freewheel/releases/tag/v0.1.1");
});
