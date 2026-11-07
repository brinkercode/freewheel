# freewheel — design handoff

Open `prototype/freewheel.dc.html` in a browser (keep support.js next to it). The dark right column fakes the device and OS states.

## Contents
- **prototype/** — source. Single file: inline-styled markup (layout, spacing, colour) + one logic class (all states, copy, behaviour). The device layout lives in `KEYDEF` (key, x, y, w, h, colour in 52px units).
- **screens-and-components.md** — every screen, component and state.
- **tokens.css** — colours (light + dark), type sizes, spacing, radii, control heights, motion.
- **assets/speed-editor.svg** — device drawing, 666×419. Each key is `<g id="key-SLUG" data-key="PRINTED NAME">` with `.key-face`, `.key-led`, `.key-name`, `.key-label` (empty; set text at runtime). Wheel: `#wheel`, rotate `#wheel-dimple`, text `.wheel-button` / `.wheel-label`.
  - Selected: face → *-sel colour + 2px #1c1c1e gap + 2px --accent-device ring. Pressed: face → --accent-device for 2s. Lit: led → #ffb547 with glow.
- **icons/tray/macos/** — template PNGs (black on transparent), 18 px + 36 px (`-2x` — rename to `@2x` if your loader needs it): on, off, not-connected, attention.
- **icons/tray/windows/{dark,light}-taskbar/** — 16/24/32 px PNGs per state (white glyph for dark taskbar, near-black for light). Attention has an amber badge. No ICO; build one from these if needed.
- **icons/app/** — app-icon-1024.png (rounded square) and logo-mark-1024.png (bare mark).

## Copy
All final copy is in the prototype (template text + the logic class: choice titles/subtitles, notices, status lines, toasts). No separate file.

## Tray menu
The prototype draws a custom panel. For a native Tauri menu, map it as:
1. `freewheel is On` — check item (toggles enabled)
2. Status line — disabled item; when it says "Needs permission (click to fix)" make it enabled and open the permission screen
3. ——
4. Open settings… · Start at login (check) · Download logs for support… · Check for updates / Restart to update
5. ——
6. Quit freewheel
