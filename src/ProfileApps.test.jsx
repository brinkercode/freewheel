// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import KeysTab from "./KeysTab.jsx";
import ProfileApps, { NO_FRONT_APP, otherOsOnly, profileRule } from "./ProfileApps.jsx";
import { addProfile, profileApps, profileLook, sameApp, setProfileApps, setProfileLook, typedApp } from "./keys.js";

afterEach(cleanup);

const notepad = { name: "Notepad", windows: "notepad.exe" };
/** Default (pages Main and Music) and the profile Notes (pages Draft and Review). */
const config = (apps) => ({
  version: 1,
  base_layer: "base",
  layers: [
    { id: "base", name: "Main", bindings: { Cam1: { LayerPush: { target: "music" } } } },
    { id: "music", name: "Music", bindings: {} },
    { id: "draft", name: "Draft", bindings: { Cam2: { LayerPush: { target: "review" } } } },
    { id: "review", name: "Review", bindings: {} },
  ],
  profiles: [{ id: "notes", name: "Notes", pages: ["draft", "review"], ...(apps ? { apps } : {}) }],
});
const APPS = [
  { name: "Notepad", path: "explorer.exe", args: ["C:\\Start Menu\\Notepad.lnk"] },
  { name: "Paint", path: "explorer.exe", args: ["C:\\Start Menu\\Paint.lnk"] },
];
const click = (el) => act(async () => fireEvent.click(el));

const mount = (c, props = {}) => {
  const save = vi.fn(() => Promise.resolve());
  const onSaved = vi.fn();
  const toApp = vi.fn((choice) => Promise.resolve({ name: choice.name, windows: `${choice.name.toLowerCase()}.exe` }));
  render(<ProfileApps config={c} profileId="notes" save={save} onSaved={onSaved} toApp={toApp} initialApps={APPS} loadIcons={null} choose={() => Promise.resolve(null)} {...props} />);
  return { save, onSaved, toApp };
};

test("the panel states the rule: the profile's apps switch to it, any other app goes back to Default", () => {
  mount(config());
  expect(screen.getByRole("heading", { name: "When is Notes used?" })).toBeTruthy();
  expect(screen.getByText("Add the apps it is for")).toBeTruthy();
  expect(screen.getByText(profileRule("Notes"))).toBeTruthy();
  expect(profileRule("Notes")).toBe(
    "When one of these apps is in front, your Speed Editor uses Notes, starting on its first page. Any other app: back to Default, on the page it was on.",
  );
  expect(screen.queryByText(NO_FRONT_APP)).toBeNull();
});

test("picking an app from the same picker as Open an app saves it by the name its OS reports", async () => {
  const { save, onSaved, toApp } = mount(config());
  await click(screen.getByRole("option", { name: /Notepad/ }));
  expect(toApp).toHaveBeenCalledWith(APPS[0]);
  await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
  const saved = save.mock.calls[0][0];
  expect(saved.profiles[0].apps).toEqual([{ name: "Notepad", windows: "notepad.exe" }]);
  expect(onSaved).toHaveBeenCalledWith(saved);
});

test("a program name can be typed; the same app twice is refused; an app can be removed", async () => {
  const { save } = mount(config([notepad]));
  expect(screen.getByText("For 1 app")).toBeTruthy();
  const list = within(screen.getByRole("list", { name: "Apps for Notes" }));
  expect(list.getByText("Notepad")).toBeTruthy();
  expect(list.getByText("notepad.exe")).toBeTruthy();
  await click(screen.getByRole("button", { name: "Add an app" }));
  const input = screen.getByRole("textbox", { name: "Program name" });
  expect(input.placeholder).toBe("like notepad.exe");
  expect(input.title).toBe("The program file, like notepad.exe");
  fireEvent.change(input, { target: { value: "NOTEPAD" } });
  await click(screen.getByRole("button", { name: "Add" }));
  expect(screen.getByRole("alert").textContent).toBe("Notepad is already on Notes.");
  expect(save).not.toHaveBeenCalled();
  fireEvent.change(input, { target: { value: " code.exe " } });
  await click(screen.getByRole("button", { name: "Add" }));
  await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
  expect(save.mock.calls[0][0].profiles[0].apps).toEqual([notepad, { name: "code.exe", windows: "code.exe" }]);
  await click(list.getByRole("button", { name: "Remove Notepad" }));
  await waitFor(() => expect(save).toHaveBeenCalledTimes(2));
  // No apps left: config.json leaves `apps` out.
  expect("apps" in save.mock.calls[1][0].profiles[0]).toBe(false);
});

test("a failed save says why", async () => {
  mount(config(), { save: () => Promise.reject({ kind: "SaveFailed", message: "disk full" }) });
  fireEvent.change(screen.getByRole("textbox", { name: "Program name" }), { target: { value: "x.exe" } });
  await click(screen.getByRole("button", { name: "Add" }));
  await waitFor(() => expect(screen.getByRole("alert").textContent).toBe("Saving failed: disk full"));
});

test("on Wayland the panel says profiles don't switch there", () => {
  mount(config(), { supported: false });
  expect(screen.getByRole("note").textContent).toBe(NO_FRONT_APP);
});

test("an app set for another OS only says so", () => {
  expect(otherOsOnly({ name: "TextEdit", macos: "com.apple.TextEdit" }, "windows")).toBe("Set for macOS only.");
  expect(otherOsOnly(notepad, "windows")).toBeNull();
  mount(config([{ name: "TextEdit", macos: "com.apple.TextEdit", linux: "gedit" }]));
  expect(screen.getByText("Set for macOS and Linux only.")).toBeTruthy();
});

test("the profile apps model: per OS, names ignore case and .exe", () => {
  expect(typedApp(" gedit ", "linux")).toEqual({ name: "gedit", linux: "gedit" });
  expect(sameApp(notepad, { name: "x", windows: "NOTEPAD" }, "windows")).toBe(true);
  expect(sameApp(notepad, { name: "x", linux: "notepad.exe" }, "windows")).toBe(false);
  const c = setProfileApps(config(), "notes", [notepad]);
  expect(profileApps(c, "notes")).toEqual([notepad]);
  expect(profileApps(c, "nope")).toEqual([]);
});

test("the profile picker: Default and each profile above the page pills; a profile shows its own pages and its apps", async () => {
  const save = vi.fn(() => Promise.resolve());
  render(<KeysTab initial={config([notepad])} save={save} initialFrontApps={false} />);
  const profiles = () => within(screen.getByRole("tablist", { name: "Profiles" }));
  const pages = () => within(screen.getByRole("tablist", { name: "Pages" }));
  expect(profiles().getAllByRole("tab").map((t) => t.textContent)).toEqual(["Default", "Notes"]);
  expect(pages().getAllByRole("tab").map((t) => t.textContent)).toEqual(["Main", "Music"]);
  expect(screen.getByText("Pick a key")).toBeTruthy();
  await click(profiles().getByRole("tab", { name: "Notes" }));
  expect(profiles().getByRole("tab", { name: "Notes" }).title).toBe("For Notepad");
  expect(pages().getAllByRole("tab").map((t) => t.textContent)).toEqual(["Draft", "Review"]);
  expect(document.querySelector(".pill.is-active").textContent).toContain("Draft");
  // Its apps, from the Apps button; a key's panel replaces them.
  await click(screen.getByRole("button", { name: "Apps for this profile" }));
  expect(screen.getByRole("heading", { name: "When is Notes used?" })).toBeTruthy();
  expect(screen.getByRole("note").textContent).toBe(NO_FRONT_APP);
  await click(document.querySelector('[data-binding="Cut"]'));
  expect(screen.queryByRole("heading", { name: "When is Notes used?" })).toBeNull();
  // "Go to a page" offers this profile's pages only.
  await click(document.querySelector('[data-binding="Cam2"]'));
  expect(document.querySelector('.device [data-binding="Cam2"]').getAttribute("title")).toBe("→ Review");
});

test("New profile adds a profile with a Main page and opens its apps; rename and delete it", async () => {
  const save = vi.fn(() => Promise.resolve());
  render(<KeysTab initial={config()} save={save} initialFrontApps />);
  await click(screen.getByRole("button", { name: "New profile" }));
  await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
  const added = save.mock.calls[0][0];
  expect(added.profiles.map((p) => [p.name, p.pages])).toEqual([
    ["Notes", ["draft", "review"]],
    ["Profile 2", ["profile-2-main"]],
  ]);
  expect(added.layers.at(-1)).toEqual({ id: "profile-2-main", name: "Main", bindings: {} });
  expect(screen.getByRole("heading", { name: "When is Profile 2 used?" })).toBeTruthy();
  await click(screen.getByRole("button", { name: "Rename profile" }));
  const input = screen.getByRole("textbox", { name: "Profile name" });
  fireEvent.change(input, { target: { value: "default" } });
  fireEvent.keyDown(input, { key: "Enter" });
  expect(screen.getByRole("alert").textContent).toContain("Default is the profile for every other app");
  await click(screen.getByRole("button", { name: "Rename profile" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Profile name" }), { target: { value: "Browser" } });
  fireEvent.keyDown(screen.getByRole("textbox", { name: "Profile name" }), { key: "Enter" });
  await waitFor(() => expect(save.mock.calls.at(-1)[0].profiles[1].name).toBe("Browser"));
  await click(screen.getByRole("button", { name: "Delete profile" }));
  await click(screen.getByRole("button", { name: "Delete" }));
  await waitFor(() => expect(save.mock.calls.at(-1)[0].profiles.map((p) => p.name)).toEqual(["Notes"]));
  expect(save.mock.calls.at(-1)[0].layers.some((l) => l.id === "profile-2-main")).toBe(false);
  expect(screen.getByRole("tab", { name: "Default" }).getAttribute("aria-selected")).toBe("true");
});

test("the profile and page in use are filled in the profile's colour; no dots", async () => {
  const c = config([notepad]);
  c.profiles[0] = { ...c.profiles[0], color: "teal", icon: "edit" };
  const status = { connection: "Connected", layer: "music", lit: ["Cam1"] };
  const { rerender } = render(<KeysTab initial={c} status={status} initialFrontApps />);
  const using = (sel) => [...document.querySelectorAll(`${sel}.is-using [role=tab]`)].map((e) => e.textContent);
  expect(document.querySelector(".live-dot, .live-legend")).toBeNull();
  expect(using(".profile-pill")).toEqual(["Default"]);
  expect(document.querySelector(".profile-pill.is-using").dataset.color).toBe("accent");
  expect(document.querySelector('.profile-pill.is-using [data-icon="keyboard"]')).toBeTruthy();
  expect(using(".pill")).toEqual(["Music"]);
  // Notepad in front: Notes, on its first page, in teal with its icon.
  rerender(<KeysTab initial={c} status={{ ...status, layer: "draft", lit: [] }} initialFrontApps />);
  await waitFor(() => expect(using(".profile-pill")).toEqual(["Notes"]));
  const notes = document.querySelector(".profile-pill.is-using");
  expect(notes.dataset.color).toBe("teal");
  expect(notes.querySelector('[data-icon="edit"]')).toBeTruthy();
  expect(using(".pill")).toEqual(["Draft"]);
  expect(document.querySelector(".pill.is-using").dataset.color).toBe("teal");
  expect(document.querySelector(".live-dot")).toBeNull();
});

test("the profile's editor picks its colour and icon, saved in the config", async () => {
  const { save } = mount(config([notepad]));
  const colours = within(screen.getByRole("radiogroup", { name: "Colour" }));
  expect(colours.getAllByRole("radio").map((r) => r.getAttribute("aria-label"))).toEqual(["rose", "orange", "amber", "green", "teal", "blue", "indigo", "violet"]);
  await click(colours.getByRole("radio", { name: "indigo" }));
  await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
  expect(save.mock.calls[0][0].profiles[0].color).toBe("indigo");
  const icons = within(screen.getByRole("radiogroup", { name: "Icon" }));
  expect(icons.getAllByRole("radio")).toHaveLength(8);
  for (const r of icons.getAllByRole("radio")) expect(r.querySelector("svg"), r.getAttribute("aria-label")).toBeTruthy();
  await click(icons.getByRole("radio", { name: "music note" }));
  await waitFor(() => expect(save).toHaveBeenCalledTimes(2));
  expect(save.mock.calls[1][0].profiles[0].icon).toBe("music_note");
});

test("a new profile gets the next colour and icon; Default keeps the accent and the keyboard", () => {
  const a = addProfile(config());
  expect(a.config.profiles.at(-1)).toMatchObject({ color: "orange", icon: "code" });
  expect(profileLook(a.config, null)).toEqual({ color: "accent", icon: "keyboard" });
  expect(profileLook(config(), "notes")).toEqual({ color: "accent", icon: "tab" });
  expect(profileLook(setProfileLook(config(), "notes", { color: "pink", icon: "star" }), "notes")).toEqual({ color: "accent", icon: "tab" });
});
