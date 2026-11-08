// @vitest-environment jsdom
// What's new's notes, best first: stored at update time, else the version's
// GitHub release body (src-tauri release_notes.rs, cleaned by
// src/releaseNotes.js appNotes, kept per version), else the link to GitHub.
import { cleanup, configure, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { afterEach, expect, test, vi } from "vitest";
import App from "./App.jsx";
import { sampleConfig, sampleStatus } from "./dev/mockTauri.js";
import { appNotes } from "./releaseNotes.js";
import { LAST_SEEN_VERSION, RELEASE_NOTES, UPDATE_NOTES, fetchedNotes, loadNotes } from "./whatsNew.js";

/** A GitHub release body (the release cut: scope labels, PR links, hashes), shared with release_notes.rs's tests. */
const BODY = readFileSync("scripts/fixtures/release-notes/github-release.md", "utf8");
/** ... and what What's new makes of it. */
const APP = readFileSync("scripts/fixtures/release-notes/github-release.app.md", "utf8");
const STORED = "## 1.1.0 (2026-11-08)\n\n### New\n\n* Stored at update time\n";

function memoryStorage(items = {}) {
  const map = new Map(Object.entries(items));
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, String(v)),
    removeItem: (k) => map.delete(k),
    map,
  };
}

let invoke;
/** A build running `version` whose release_notes answers `notes` (a function of the version). */
function fakeTauri(version, notes) {
  invoke = vi.fn(async (cmd, args) => {
    switch (cmd) {
      case "app_version":
        return version;
      case "get_config":
        return sampleConfig();
      case "get_status":
        return sampleStatus();
      case "release_notes":
        return notes(args.version);
      default:
        return null;
    }
  });
  window.__TAURI__ = { core: { invoke }, event: { listen: async () => () => {} } };
  window.__TAURI_INTERNALS__ = { invoke, transformCallback: () => 1, metadata: {} };
}

configure({ asyncUtilTimeout: 10000 });
afterEach(() => {
  cleanup();
  delete window.__TAURI__;
  delete window.__TAURI_INTERNALS__;
});

const notice = () => screen.queryByRole("button", { name: "What’s new in v1.1.0" });
const asked = () => invoke.mock.calls.filter(([c]) => c === "release_notes");
const shown = (container) => {
  const card = container.querySelector(".whats-new-notes");
  return { h4: [...card.querySelectorAll("h4")].map((h) => h.textContent), li: [...card.querySelectorAll("li")].map((li) => li.textContent), text: card.textContent };
};

test("the shared fixture (release_notes.rs's tests parse it too) cleans as expected", () => {
  expect(appNotes(BODY)).toBe(APP);
  expect(APP).not.toMatch(/\*\*\w+:\*\*|#18\d|\/commit\/|[0-9a-f]{7}\)/);
});

test("level 1: notes stored at update time are shown and GitHub is never asked", async () => {
  fakeTauri("1.1.0", () => BODY);
  const storage = memoryStorage({ "freewheel.first-run-done": "1", [LAST_SEEN_VERSION]: "1.0.0", [UPDATE_NOTES]: JSON.stringify({ version: "1.1.0", notes: STORED }) });
  const { container } = render(<App storage={storage} />);
  await waitFor(() => expect(notice()).not.toBeNull());
  fireEvent.click(notice());
  expect(shown(container).li).toEqual(["Stored at update time"]);
  await new Promise((r) => setTimeout(r, 50));
  expect(asked()).toEqual([]);
});

test("level 2: without stored notes (the bug: a placeholder) the GitHub release's notes are shown, formatted, and kept", async () => {
  fakeTauri("1.1.0", (v) => (v === "1.1.0" ? BODY : null));
  // Updated from outside the window (a new file downloaded by hand): no notes stored.
  const storage = memoryStorage({ "freewheel.first-run-done": "1", [LAST_SEEN_VERSION]: "1.0.0" });
  const { container } = render(<App storage={storage} />);
  await waitFor(() => expect(notice()).not.toBeNull());
  // Nothing is fetched until What's new opens.
  expect(asked()).toEqual([]);
  fireEvent.click(notice());
  await waitFor(() => expect(shown(container).li.length).toBeGreaterThan(0));
  expect(shown(container)).toEqual({
    h4: ["New", "Fixed"],
    li: ["Profiles that switch automatically for each app", "Keys can open a folder with Open", "What's new shows the release notes from GitHub", "freewheel starts faster after an update"],
    text: expect.not.stringContaining("The notes for this version are on GitHub."),
  });
  expect(container.querySelector(".whats-new-notes li strong").textContent).toBe("folder");
  expect(asked()).toEqual([["release_notes", { version: "1.1.0" }]]);
  expect(fetchedNotes(storage, "1.1.0")).toBe(APP);
});

test("level 3: when GitHub can't be asked or has no notes, the page links to the release", async () => {
  for (const answer of [() => Promise.reject("network: timed out"), () => null, () => "  "]) {
    fakeTauri("1.1.0", answer);
    const storage = memoryStorage({ "freewheel.first-run-done": "1", [LAST_SEEN_VERSION]: "1.0.0" });
    const { container, unmount } = render(<App storage={storage} />);
    await waitFor(() => expect(notice()).not.toBeNull());
    fireEvent.click(notice());
    await waitFor(() => expect(container.querySelector(".whats-new-notes").textContent).toBe("The notes for this version are on GitHub."));
    expect(screen.getByRole("link", { name: "See this release on GitHub" }).getAttribute("href")).toBe("https://github.com/brinkercode/freewheel/releases/tag/v1.1.0");
    expect(storage.getItem(RELEASE_NOTES)).toBeNull();
    unmount();
  }
});

test("General's What's new opens the running version's notes on a fresh install too", async () => {
  fakeTauri("1.1.0", () => BODY);
  const storage = memoryStorage({ "freewheel.first-run-done": "1" });
  const { container } = render(<App storage={storage} />);
  fireEvent.click(await screen.findByRole("button", { name: "General" }));
  // A fresh install: no notice, no request.
  expect(notice()).toBeNull();
  expect(asked()).toEqual([]);
  fireEvent.click(await screen.findByRole("link", { name: "What’s new in v1.1.0" }));
  expect(screen.getByRole("heading", { name: "What’s new in v1.1.0" })).toBeTruthy();
  await waitFor(() => expect(shown(container).h4).toEqual(["New", "Fixed"]));
  fireEvent.click(screen.getByRole("button", { name: "Done" }));
  // Opened again: the kept notes, no second request.
  fireEvent.click(await screen.findByRole("button", { name: "General" }));
  fireEvent.click(await screen.findByRole("link", { name: "What’s new in v1.1.0" }));
  await waitFor(() => expect(shown(container).h4).toEqual(["New", "Fixed"]));
  expect(asked()).toHaveLength(1);
});

test("loadNotes: stored first, then kept, then fetched; a few versions are kept", async () => {
  const storage = memoryStorage();
  const fetchBody = vi.fn(async (v) => `### New\n\n* **keys:** in ${v} ([#1](https://github.com/brinkercode/freewheel/issues/1))\n`);
  expect(await loadNotes({ version: "1.1.0", notes: STORED }, { storage, fetchBody })).toBe(STORED);
  expect(fetchBody).not.toHaveBeenCalled();
  expect(await loadNotes({ version: "1.1.0" }, { storage, fetchBody })).toBe("### New\n\n* In 1.1.0\n");
  expect(await loadNotes({ version: "1.1.0" }, { storage, fetchBody })).toBe("### New\n\n* In 1.1.0\n");
  expect(fetchBody).toHaveBeenCalledTimes(1);
  for (const v of ["1.2.0", "1.3.0", "1.4.0", "1.5.0", "1.6.0"]) await loadNotes({ version: v }, { storage, fetchBody });
  expect(Object.keys(JSON.parse(storage.getItem(RELEASE_NOTES)))).toEqual(["1.2.0", "1.3.0", "1.4.0", "1.5.0", "1.6.0"]);
  storage.setItem(RELEASE_NOTES, "not json");
  const log = vi.fn();
  expect(await loadNotes({ version: "2.0.0" }, { storage, fetchBody: () => Promise.reject("x"), log })).toBeNull();
  expect(log).toHaveBeenCalled();
});
