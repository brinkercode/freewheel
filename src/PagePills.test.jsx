// @vitest-environment jsdom
// Page and profile pills: renaming a page (#199, #201), Add page names (#203)
// and long names (#206).
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import KeysTab from "./KeysTab.jsx";
import { addPage, deletePage } from "./keys.js";

afterEach(cleanup);

const hotkey = (key) => ({ Hotkey: { combo: { modifiers: ["Ctrl"], key } } });
const config = () => ({
  version: 1,
  base_layer: "main",
  layers: [
    { id: "main", name: "Main", bindings: { Cut: hotkey("S"), Esc: { LayerPush: { target: "music" } } } },
    { id: "music", name: "Music", bindings: {} },
  ],
});
const showMusic = () => fireEvent.click(screen.getByRole("tab", { name: "Music" }));
const pageName = () => screen.getByLabelText("Page name");
const savedNames = (save) => save.mock.calls.map(([c]) => c.layers.find((l) => l.id === "music").name);
const startRename = (save) => {
  render(<KeysTab initial={config()} save={save} />);
  showMusic();
  fireEvent.click(screen.getByRole("button", { name: "Rename" }));
};

test("a page can't be renamed to nothing: it keeps its old name", async () => {
  const save = vi.fn(() => Promise.resolve());
  startRename(save);
  await act(async () => fireEvent.change(pageName(), { target: { value: "" } }));
  await act(async () => fireEvent.keyDown(pageName(), { key: "Enter" }));
  expect(savedNames(save)).not.toContain("");
  expect(screen.getByRole("tab", { name: "Music" })).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "Rename" }));
  await act(async () => fireEvent.change(pageName(), { target: { value: "   " } }));
  await act(async () => fireEvent.blur(pageName()));
  expect(save).not.toHaveBeenCalled();
  expect(screen.getByRole("tab", { name: "Music" })).toBeTruthy();
});

test("Escape while renaming a page puts the old name back and saves nothing", async () => {
  const save = vi.fn(() => Promise.resolve());
  startRename(save);
  await act(async () => fireEvent.change(pageName(), { target: { value: "Typo" } }));
  await act(async () => fireEvent.keyDown(pageName(), { key: "Escape" }));
  await act(async () => fireEvent.blur(pageName()));
  expect(screen.getByRole("tab", { name: "Music" })).toBeTruthy();
  expect(save).not.toHaveBeenCalled();
});

test("Enter or clicking away saves the new page name once, trimmed", async () => {
  const save = vi.fn(() => Promise.resolve());
  startRename(save);
  await act(async () => fireEvent.change(pageName(), { target: { value: "M" } }));
  await act(async () => fireEvent.change(pageName(), { target: { value: " Mixer " } }));
  expect(save).not.toHaveBeenCalled();
  await act(async () => fireEvent.keyDown(pageName(), { key: "Enter" }));
  await act(async () => fireEvent.blur(pageName()));
  expect(savedNames(save)).toEqual(["Mixer"]);
  expect(screen.getByRole("tab", { name: "Mixer" })).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "Rename" }));
  await act(async () => fireEvent.change(pageName(), { target: { value: "Studio" } }));
  await act(async () => fireEvent.blur(pageName()));
  expect(savedNames(save)).toEqual(["Mixer", "Studio"]);
});

test("Add page never gives a new page the name of one already there", () => {
  let c = config();
  c = addPage(c).config; // Page 3
  c = addPage(c).config; // Page 4
  c = deletePage(c, "page-3").config;
  c = addPage(c).config;
  const names = c.layers.map((l) => l.name);
  expect(new Set(names).size).toBe(names.length);
  expect(names).toEqual(["Main", "Music", "Page 4", "Page 5"]);
  // A page renamed by hand to the next "Page N" is skipped too.
  const renamed = { ...config(), layers: [...config().layers, { id: "x", name: "page 4", bindings: {} }] };
  expect(addPage(renamed).config.layers.at(-1).name).toBe("Page 5");
});

test("a profile's new page skips the names of that profile's pages", () => {
  const c = {
    ...config(),
    layers: [...config().layers, { id: "w", name: "Main", bindings: {} }, { id: "w2", name: "Page 3", bindings: {} }],
    profiles: [{ id: "writing", name: "Writing", apps: [], pages: ["w", "w2"] }],
  };
  expect(addPage(c, "writing").config.layers.at(-1).name).toBe("Page 4");
  expect(addPage(c).config.layers.at(-1).name).toBe("Page 3");
});

const LONG_PAGE = "Music and podcasts while editing the long documentary project";
const LONG_PROFILE = "My extremely long profile name for video editing and music production work in the studio";

test("long page and profile names are cut with an ellipsis, the whole name in their title", () => {
  const c = {
    ...config(),
    layers: [...config().layers.map((l) => (l.id === "music" ? { ...l, name: LONG_PAGE } : l)), { id: "s", name: "Main", bindings: {} }, { id: "s2", name: LONG_PAGE, bindings: {} }],
    profiles: [{ id: "spotify", name: LONG_PROFILE, apps: [], pages: ["s", "s2"] }],
  };
  render(<KeysTab initial={c} />);
  const pages = within(screen.getByRole("tablist", { name: "Pages" }));
  const page = pages.getByRole("tab", { name: LONG_PAGE }).querySelector(".pill-name");
  expect(page.title).toBe(LONG_PAGE);
  const profiles = within(screen.getByRole("tablist", { name: "Profiles" }));
  const profile = profiles.getByRole("tab", { name: LONG_PROFILE }).querySelector(".pill-name");
  expect(profile.title).toBe(LONG_PROFILE);
  // The active pill's name and buttons share the pill: the name shrinks, the buttons don't.
  fireEvent.click(profiles.getByRole("tab", { name: LONG_PROFILE }));
  expect(screen.getByRole("button", { name: "Delete profile" }).closest(".profile-pill")).toBe(profile.closest(".profile-pill"));

  const css = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "styles/base.css"), "utf8");
  expect(css).toMatch(/\.pill-name \{[^}]*min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;/);
  expect(css).toMatch(/\.pill, \.profile-pill \{[^}]*min-width: 0; max-width: 100%;/);
  expect(css).toMatch(/\.pills, \.profile-pills \{[^}]*min-width: 0; max-width: 100%;/);
  expect(css).toMatch(/\.pill > button\[role="tab"\], \.profile-pill > button\[role="tab"\] \{[^}]*flex: 0 1 auto; min-width: 0;/);
  expect(css).toMatch(/\.pill \.pill-icon, \.profile-pill \.profile-icon \{[^}]*flex: none;/);
});
