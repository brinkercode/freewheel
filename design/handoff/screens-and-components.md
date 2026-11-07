# freewheel — screens & components (React + Tauri)

Prototype: `freewheel.dc.html`. Use the dark "Prototype controls" column to fake the device, OS, and app states.

## Global state the UI reads
- `enabled` (On/Off), `connection` (none | usb | bluetooth + battery %), `resolveOpen`, `stepAsideForResolve`, `macPermissions {pressKeys, noticeDevice}`, `updateReady`
- Derived **status** (first match wins): Off → Needs permission → Looking for device → Paused (Resolve) → Connected (USB / Bluetooth · N%)
- Derived **tray icon**: On · Off/paused · Not connected · Needs attention (permission missing or battery < 15%)

## 1. Tray / menu-bar icon
Logo mark = a disc with an off-centre dimple (the jog wheel). States: On (filled disc), Off/paused (faded outline), Not connected (dashed outline), Needs attention (filled + amber badge). Click opens the Tray menu. Monochrome template image on macOS.

## 2. Tray menu
- On/Off switch row — title "freewheel is On/Off", one-line subtitle
- Status line — clickable; opens Permissions screen when permission is missing, otherwise Settings
- Open settings… · Start at login (check) · Download logs for support… (opens Help) · Check for updates / Restart to update · Quit freewheel

## 3. Settings window (1000×640 default, resizable, min 820×520)
Window chrome: native (Tauri decorations). Closing hides to tray — never quits.

**Toolbar**: tabs Keys · Wheel · DaVinci Resolve · General · Help & backup; status (dot + text); On/Off switch.
**Update banner** (animated): available (Download and install, Later) → downloading (spinning logo mark + progress bar) → ready (Restart to update, Later) → restarting. With auto-update on, it starts at downloading.
**Notice bar** (one at a time, same priority as status): permission missing (Fix this) · Off (Turn on) · Looking for device · Paused for Resolve · Battery low.

### 3a. Keys tab
- **Page pills** — hidden until a 2nd page exists. States: active, inactive, live (green dot = page the device is on now), renaming (inline input; Enter saves, Esc cancels), delete-confirm (inline, not modal). "Add page". First page can't be deleted.
- **Drawing / List** toggle.
- **"You pressed X"** chip — appears 2s on any physical press; also selects that key.
- **Device drawing** — 43 keys + wheel, scales to fit. Key states: not set (grey "Not set"), set (short label), selected (accent ring), just pressed (accent flash), lit (amber LED: toggle on, active wheel button, wheel-override key). Dimmed when device not connected. Wheel shows current wheel behaviour; click → Wheel tab.
- **List view** — same keys grouped Left / Middle / Right; 44px rows.

### 3b. Key panel — "When I press X…" (316px, right)
- Empty: "Pick a key" (+ "wheel already scrolls" tip on an empty setup)
- Choices list (7): Press a shortcut · Hold a key while I hold this one · Turn a key on and off · Open an app or website · Turn the wheel into… · Switch to another set of keys · Do nothing. Current choice is checked. One click each.
- **Record box** (shared component): idle (dashed, "Record") · listening (accent, pulsing, live modifiers as keycaps, Cancel) · done (keycaps, "click to record again"). Shortcut mode = modifiers + one key; Hold/Toggle mode = any single key. Starts listening automatically when the choice is opened. Saves instantly — no Save button.
- Presets: Copy, Paste, Undo, Redo, Save, Play/Pause, Mute (platform-correct keys). Hold/Toggle quick picks: Shift, Ctrl/⌘, Alt/⌥.
- Toggle mode shows the "key light stays lit" illustration.
- App picker: search, installed apps with icons, Choose file…, website field + Use.
- Wheel override: System volume / App volume / Brightness; "press again to go back".
- Pages: existing pages, new page name + suggestions; creating/linking auto-sets the same key to **Back** on the target page.
- Back key: locked, explains why ("you can never get stuck").
- Wheel button key (JOG/SHTL/SCRL): explainer + "Set up the wheel".
- Footer: "Try it: press X…" / "Saved. Press X…".

### 3c. Wheel tab
- Override notice (when a key turned the wheel into a knob) + Back to normal
- Wheel button selector SHTL / JOG / SCRL (live amber dot = lit on device)
- Radio list: Scroll up and down (default) · Press shortcuts (two Record boxes: left / right) · Volume · Brightness · Nothing
- Sensitivity slider Slow ↔ Fast (0–100 internally; no numbers shown)

### 3d. DaVinci Resolve tab
- Toggle "Step aside automatically…" (default on) + one-line explanation
- Tip: unplug and plug back in
- More → Resolve's program name (+ Reset)

### 3e. General tab
- Appearance: Match my computer / Light / Dark (preview cards)
- Start freewheel when I log in
- Version + Check for updates (idle / checking / up to date / downloading / ready)
- Update automatically (default on)

### 3f. Help & backup tab
- Download logs for support: Download logs → downloaded confirmation + Show file; privacy line
- Back up… / Restore from a backup… → **single** confirm dialog "Replace your keys?" (Keep my keys / Replace my keys)
- More → detailed logs toggle (auto-off after 24h)

## 4. First run (replaces window content)
1. Welcome → Get started
2. Plug in or pair — Looking… (spinner) → Found it! (connection type) · Open Bluetooth settings · Continue (disabled until found)
3. macOS only: Permissions — two rows (Accessibility, Input Monitoring), each: button → waiting → ✓ Allowed (poll). "Start using freewheel" enabled when both ticked.
Then Keys tab, every key "Not set", wheel already scrolling.
Permission fix (from status/notice) reuses screen 3 with a "Done" button.

## 5. Toast
Bottom-centre, 2.8s. Used for confirmations only, never for errors needing action.

## Shared components
Switch · Segmented control · Choice row (icon, title, sub, check) · Radio row · Record box · Keycap · Pill · Notice bar · Card · Toast · Confirm dialog · Device drawing / Key · Tray icon.

## Copy rules
Plain words only. Never show key codes, file paths, JSON, "layer", "HID", "mode", "binding". Keys are named as printed on the device.
