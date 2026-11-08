# The freewheel mappings file

**Help & backup → Back up…** saves your keys, pages, profiles, wheel and dial as one
file, `freewheel-mappings.json`. **Restore from a backup…** reads it back, on
this computer or another one. The file is short and meant to be written and
edited by hand:

```json
{
  "$schema": "https://raw.githubusercontent.com/brinkercode/freewheel/main/schema/freewheel-mappings.schema.json",
  "version": 2,
  "pages": {
    "Main": {
      "CUT": "Ctrl+Z",
      "CAM1": "open Spotify",
      "CAM2": "https://youtube.com",
      "TRANS": "hold Shift",
      "SNAP": "toggle Alt",
      "ESC": "page Music",
      "STOP/PLAY": "Play/Pause",
      "AUDIO LEVEL": "wheel volume"
    },
    "Music": { "ESC": "back" }
  },
  "wheel": { "mode": "scroll", "speed": 5 }
}
```

- Each page lists keys by the name printed on the Speed Editor, and what
  each key does as one line of text.
- Leave out anything you don't use: missing keys are not set, a missing
  `wheel` or `dial` takes the defaults below.
- The `$schema` line lets editors such as VS Code complete key names and
  actions and underline mistakes as you type. The schema is
  [`schema/freewheel-mappings.schema.json`](schema/freewheel-mappings.schema.json).
- Files from earlier freewheel versions (the long `"version": 1` form with
  `"layers"`) still restore. Back up always writes this short form.
- `"profiles"` is optional: app profiles, each with its own pages, used
  while one of its apps is in front (see [App profiles](#app-profiles)).

Contents: [Fields](#fields) · [App profiles](#app-profiles) · [Key names](#key-names) · [Actions](#actions) ·
[Shortcuts](#shortcuts) · [How open finds the app](#how-open-finds-the-app) ·
[Safety: Let restored keys open apps](#safety-let-restored-keys-open-apps) ·
[Errors](#errors) · [A full example](#a-full-example)

## Fields

Every field the file can have. Names of fields are exact (lower case);
values like `"Dark"` or `"JOG"` may be in any case.

<!-- generated: fields -->
| Field | Type | Default | Allowed | What it is |
|---|---|---|---|---|
| `$schema` | text | (none) | any URL | Where editors find the JSON Schema; freewheel ignores it. |
| `version` | number | 2 | 1 or 2 | The file format. 1 is the long form earlier versions wrote; it still reads. |
| `pages` | object | one empty page "Base" | 1 or more pages | Page name to its keys. The first page is where the Speed Editor starts. |
| `pages.<page>` | object | {} | key name to action | Key name (see Key names) to what the key does (see Actions). Keys left out are not set. |
| `profiles` | list | [] (none) | profiles, each with a name, apps and pages | App profiles: each has its own pages and is used while one of its apps is in front. The pages above are the Default profile, used while no profile's app is. |
| `profiles[].name` | text | (required) | unique, not Default | The profile's name. |
| `profiles[].color` | text | (none: the app's accent) | rose, orange, amber, green, teal, blue, indigo, violet | The profile's colour in the Keys tab. |
| `profiles[].icon` | text | (none: a plain profile icon) | a Material Symbols Rounded icon name, like :edit: or :sports_esports: (see Profile icons) | The profile's icon in the Keys tab, by its name in Google's icon catalogue (https://fonts.google.com/icons?icon.style=Rounded). Written :name: or name. |
| `profiles[].apps` | list | [] (switched to by no app) | app names, or { "name", "windows", "macos", "linux" } objects | The apps the profile is for (see App profiles). |
| `profiles[].pages` | object | (required) | 1 or more pages | The profile's pages, written as pages is. The first page is where the profile starts. A page key goes to a page of the same profile. |
| `wheel` | object | scroll mode, speed 10 | — | What turning the wheel does. |
| `wheel.mode` | text | "scroll" | jog, scroll, shuttle, volume, brightness | The wheel button lit when freewheel starts. volume and brightness turn the wheel into that knob. |
| `wheel.speed` | number | 10 | 1 to 10 | How far the wheel turns per step, for every mode: 10 steps on every tick, 1 needs ten ticks. |
| `wheel.<mode>` | object | left "scroll up", right "scroll down" | — | What turning the wheel does while &lt;mode&gt;'s button is lit. |
| `wheel.<mode>.left` | action | "scroll up" | any action except back | What one step to the left does. |
| `wheel.<mode>.right` | action | "scroll down" | any action except back | What one step to the right does. |
| `wheel.<mode>.speed` | number | wheel.speed | 1 to 10 | This mode's speed, instead of wheel.speed. |
| `wheel.<mode>.max_rate` | number or null | null (no limit) | 1 to 1000 | At most this many steps a second (useful for shuttle). |
| `dial` | object | 2, 2, 5 | — | How much one wheel step changes while the wheel is a knob. |
| `dial.volume_step` | number | 2 | 0.1 to 100 | Percent of system volume per step. |
| `dial.app_volume_step` | number | 2 | 0.1 to 100 | Percent of the front app's volume per step. |
| `dial.brightness_step` | number | 5 | 1 to 100 | Percent of display brightness per step. |
| `settings` | object | (your current settings) | — | Optional app settings. Restore changes only the ones written; export never writes them. |
| `settings.appearance` | text | "system" | system, light, dark | The settings window's colours. |
| `settings.update_automatically` | true/false | true | — | Download new versions by themselves. |
| `settings.step_aside_for_resolve` | true/false | true | — | Turn freewheel off while DaVinci Resolve is open. |
| `settings.resolve_program` | text | "Resolve.exe" (Windows), "Resolve" (macOS) | not empty | The program name freewheel watches for. |
| `settings.low_battery_warning` | number | 15 | 0 to 100 | Warn when the Speed Editor's battery is at or below this percent. |
| `settings.detailed_logs` | true/false | false | — | Detailed logs for troubleshooting (they turn off after a day). |
<!-- end generated: fields -->

`<page>` is any page name: pages are named by you, must be different from
each other (ignoring case), and the first one is where the Speed Editor
starts. `<mode>` is one of these wheel modes:

<!-- generated: wheel-modes -->
`jog`, `scroll`, `shuttle`, `volume`, `brightness`
<!-- end generated: wheel-modes -->

The `wheel.<mode>` blocks say what turning does while that mode's button
(SHTL, JOG or SCRL) is lit:

```json
"wheel": {
  "mode": "jog",
  "speed": 8,
  "jog": { "left": "Left", "right": "Right", "speed": 6 },
  "shuttle": { "left": "J", "right": "L", "max_rate": 8 }
}
```

`"settings"` is optional and never written by Back up: put it in a file you
share to also set, for example, `"appearance": "dark"`. Settings you leave
out stay as they are. Whether freewheel is on, and starting at login, are not
in the file.

## App profiles

`"pages"` are the Default profile. `"profiles"` adds app profiles: each has
a `name`, the `apps` it is for, its own `pages` (written as `"pages"` is),
and optionally a `color` and an `icon`:

```json
"profiles": [
  {
    "name": "Writing",
    "color": "teal",
    "icon": ":edit:",
    "apps": [
      "notepad.exe",
      { "name": "Word", "windows": "WINWORD.EXE", "macos": "com.microsoft.Word" },
      { "linux": "org.gnome.TextEditor" }
    ],
    "pages": {
      "Draft": { "CUT": "Ctrl+Shift+X", "ESC": "page Review" },
      "Review": { "ESC": "back" }
    }
  }
]
```

- When an app a profile lists comes to the front, freewheel uses that
  profile, starting on its first page. When any other app comes to the
  front, it goes back to the Default profile, on the page it was on.
  freewheel's own window never switches.
- `page <name>` keys switch between the pages of their own profile.
- An app as text is matched on every OS. An object names it per OS:
  `"windows"` is the program file (`notepad.exe`; `.exe` may be left out),
  `"macos"` the bundle id (`com.apple.TextEdit`), `"linux"` the program
  (`gedit`) or its window class (`org.gnome.gedit`). `"name"` is what the
  Keys tab shows; it defaults to the first OS's entry. Names ignore case.
- When two profiles list the same app, the first one wins.
- On Linux, apps are seen in an X11 session. Wayland does not tell other
  apps which window is in front, so there profiles never switch.
- DaVinci Resolve stepping aside wins: while freewheel is Off for Resolve,
  no profile turns it back On.
- The wheel and dial settings are shared by every profile.
- `color` and `icon` are optional: how the profile looks in the Keys tab.
  Colours: `rose`, `orange`, `amber`, `green`, `teal`, `blue`, `indigo`,
  `violet`. For the icon, see Profile icons.

## Profile icons

A profile's `icon` is any icon of [Material Symbols Rounded][catalogue],
by its name there: `":edit:"`, `":sports_esports:"`, `":brush:"`. freewheel
bundles the whole set (over 4,000 icons), so every name in the catalogue
works offline.

- Find an icon in Google's catalogue, [fonts.google.com/icons][catalogue],
  and copy its name: the words under it in lower case, with `_` for spaces
  (`Sports Esports` is `sports_esports`). The profile editor in the Keys tab
  searches the same names as you type.
- Write it between colons, `":edit:"`, or bare, `"edit"`. Both read the
  same; Back up writes `":edit:"`. Names ignore case.
- A name freewheel does not know is an error at its line and column, with
  the closest name when one is near: `line 4, column 15, the profile Games
  icon: there is no icon called ":sports_esport:" (did you mean
  ":sports_esports:"?)`. Nothing is restored until it is fixed. A config
  with an unknown icon (a hand edit) shows the profile's plain icon.
- Files from freewheel 1.0 keep their icons: `brush`, `code`,
  `music_note`, `sports_esports`, `movie`, `edit`, `photo_camera` and
  `terminal` are Material Symbols names.

[catalogue]: https://fonts.google.com/icons?icon.style=Rounded

## Key names

The keys in device order: the left block row by row, then the middle block,
then the right block. Write the name printed on the key; any letter case,
spacing or punctuation reads (`"cam 1"`, `"Cam1"`, `"stop play"`), and so do
the names earlier versions used (the last column). A key may appear once per
page.

<!-- generated: keys -->
| # | Key name | Also reads |
|---|---|---|
| 1 | `SMART INSRT` | `SmartInsrt` |
| 2 | `APPND` | `Appnd` |
| 3 | `RIPL O/WR` | `RiplOwr` |
| 4 | `CLOSE UP` | `CloseUp` |
| 5 | `PLACE ON TOP` | `PlaceOnTop` |
| 6 | `SRC O/WR` | `SrcOwr` |
| 7 | `IN` | `In` |
| 8 | `OUT` | `Out` |
| 9 | `TRIM IN` | `TrimIn` |
| 10 | `TRIM OUT` | `TrimOut` |
| 11 | `ROLL` | `Roll` |
| 12 | `SLIP SRC` | `SlipSrc` |
| 13 | `SLIP DEST` | `SlipDest` |
| 14 | `TRANS DUR` | `TransDur` |
| 15 | `CUT` | `Cut` |
| 16 | `DIS` | `Dis` |
| 17 | `SMTH CUT` | `SmthCut` |
| 18 | `ESC` | `Esc` |
| 19 | `SYNC BIN` | `SyncBin` |
| 20 | `AUDIO LEVEL` | `AudioLevel` |
| 21 | `FULL VIEW` | `FullView` |
| 22 | `TRANS` | `Trans` |
| 23 | `SPLIT` | `Split` |
| 24 | `SNAP` | `Snap` |
| 25 | `RIPL DEL` | `RiplDel` |
| 26 | `CAM7` | `CAM 7`, `Cam7` |
| 27 | `CAM8` | `CAM 8`, `Cam8` |
| 28 | `CAM9` | `CAM 9`, `Cam9` |
| 29 | `LIVE O/WR` | `LiveOwr` |
| 30 | `CAM4` | `CAM 4`, `Cam4` |
| 31 | `CAM5` | `CAM 5`, `Cam5` |
| 32 | `CAM6` | `CAM 6`, `Cam6` |
| 33 | `VIDEO ONLY` | `VideoOnly` |
| 34 | `CAM1` | `CAM 1`, `Cam1` |
| 35 | `CAM2` | `CAM 2`, `Cam2` |
| 36 | `CAM3` | `CAM 3`, `Cam3` |
| 37 | `AUDIO ONLY` | `AudioOnly` |
| 38 | `STOP/PLAY` | `StopPlay` |
| 39 | `SOURCE` | `Source` |
| 40 | `TIMELINE` | `Timeline` |
| 41 | `SHTL` | `Shtl` |
| 42 | `JOG` | `Jog` |
| 43 | `SCRL` | `Scrl` |
<!-- end generated: keys -->

`SHTL`, `JOG` and `SCRL` are the wheel buttons: left unset, each switches the
wheel to its own mode.

## Actions

What a key (or one wheel step) does. The first word is the kind of action,
in any case; a bare shortcut needs no word.

<!-- generated: actions -->
| Action | Example | What it does |
|---|---|---|
| `<shortcut>` | `Ctrl+Shift+Z` | Presses the shortcut: modifiers and one key joined by +, or a media key like Play/Pause. |
| `hold <shortcut>` | `hold Shift` | Holds the shortcut down while the key is held. A single modifier alone is allowed. |
| `toggle <shortcut>` | `toggle Alt` | The first press holds the shortcut down, the next press lets it go (the key's light shows it). |
| `open <app or path>` | `open Spotify` | Opens an app by its name, a program, a file or a folder (see How open finds the app). |
| `<web address>` | `https://youtube.com` | Opens the page in your web browser. It must start with https:// or http://. |
| `page <name>` | `page Music` | Goes to another page. On that page, the same key goes back. |
| `back` | `back` | Says the key goes back: allowed only on the key that opened this page from another page. It is the same as leaving the key out. |
| `scroll <n>` | `scroll -3` | Scrolls n lines (negative: up). scroll up and scroll down are -1 and 1. n is -100 to 100, not 0. |
| `wheel volume` | `wheel volume` | Turns the wheel into a volume knob until you press the key again. |
| `wheel app volume` | `wheel app volume` | Turns the wheel into a volume knob for the app in front. |
| `wheel brightness` | `wheel brightness` | Turns the wheel into a brightness knob. |
| `wheel mode <mode>` | `wheel mode jog` | Switches the wheel to a mode, as pressing SHTL, JOG or SCRL does: jog, scroll, shuttle, volume or brightness. |
| `nothing` | `nothing` | The key does nothing (the same as leaving it out). SHTL, JOG and SCRL left unset switch the wheel to their mode. |
| `{ "open": ..., "args": [...], "cwd": ... }` | `{ "open": "C:\\Tools\\run.cmd", "args": ["--fast", "file.txt"], "cwd": "%USERPROFILE%" }` | open with arguments and a start folder. open may also be { "windows": ..., "macos": ..., "linux": ... } for one file shared by several computers; a backup writes every OS back. |
<!-- end generated: actions -->

Notes:

- `null` works like `nothing`.
- `page <name>` names a page in the same file (any case). The key that
  opens a page always takes you back from it, whatever that page binds it
  to; `back` just says so explicitly, so it is only allowed on that key.
- `wheel volume`, `wheel app volume` and `wheel brightness` are toggles: the
  next press of the same key (or the Wheel tab's Back to normal) gives the
  wheel back. `wheel mode volume` instead makes volume the wheel's mode,
  like a fourth wheel button.
- `scroll up`/`scroll down` are `scroll -1`/`scroll 1`.
- The object form is for what one line can't say:

  ```json
  "CAM3": { "open": "C:\\Tools\\export.cmd", "args": ["--fast", "file.txt"], "cwd": "%USERPROFILE%\\Videos" },
  "CAM4": { "open": { "windows": "Spotify", "macos": "com.spotify.client", "linux": "spotify.desktop" } }
  ```

  `args` are passed to the program one by one, exactly as written (no shell
  reads them, so quotes and `$(...)` are just text). `cwd` is the folder the
  program starts in. `open` may be one target, or one per OS (`"windows"`,
  `"macos"`, `"linux"`, any of them) so a single shared file works on every
  computer: each computer uses its own line, and a computer whose line is
  missing gets the key turned off, with a note. freewheel keeps every line:
  a Back up writes all of them again, with this computer's line updated if
  you picked another app for the key here.

## Shortcuts

A shortcut is modifiers and one key, joined by `+`: `Ctrl+Shift+Z`,
`Win+Left`, `Cmd+Option+Esc`, `F13`. Spaces around `+` are fine and names
read in any case (`ctrl + z`). One character keys are just the character:
letters (`Ctrl+K`), digits, punctuation (`Ctrl+/`, or `Ctrl+Slash`), and
`Ctrl++` (or `Ctrl+Plus`) for the plus key. A shortcut needs a key besides
its modifiers; only `hold` and `toggle` take a single modifier alone
(`hold Shift`). Each modifier may appear once.

Modifiers (Back up writes the first name, and `Cmd` on a Mac):

<!-- generated: modifiers -->
| Modifier | Names that read |
|---|---|
| Ctrl | `Ctrl`, `Control`, `Ctl` |
| Shift | `Shift` |
| Alt | `Alt`, `Option`, `Opt` |
| Meta | `Win`, `Cmd`, `Command`, `Meta`, `Super`, `Windows` |
<!-- end generated: modifiers -->

Keys by name (Back up writes the second column; every name in the third
column reads too, as does each name without its spaces):

<!-- generated: shortcut-keys -->
| Group | Key (as written) | Also reads |
|---|---|---|
| Editing and navigation | `Enter` | `Return` |
| Editing and navigation | `Tab` |  |
| Editing and navigation | `Space` | `Spacebar` |
| Editing and navigation | `Escape` | `Esc` |
| Editing and navigation | `Backspace` |  |
| Editing and navigation | `Delete` | `Del` |
| Editing and navigation | `Insert` | `Ins` |
| Editing and navigation | `Home` |  |
| Editing and navigation | `End` |  |
| Editing and navigation | `PageUp` | `PgUp`, `Page Up` |
| Editing and navigation | `PageDown` | `PgDn`, `Page Down` |
| Editing and navigation | `CapsLock` | `Caps Lock` |
| Editing and navigation | `PrintScreen` | `PrtSc`, `Print Screen` |
| Editing and navigation | `Pause` | `Break` |
| Arrows | `Up` | `ArrowUp`, `Up Arrow` |
| Arrows | `Down` | `ArrowDown`, `Down Arrow` |
| Arrows | `Left` | `ArrowLeft`, `Left Arrow` |
| Arrows | `Right` | `ArrowRight`, `Right Arrow` |
| Function keys | `F1` |  |
| Function keys | `F2` |  |
| Function keys | `F3` |  |
| Function keys | `F4` |  |
| Function keys | `F5` |  |
| Function keys | `F6` |  |
| Function keys | `F7` |  |
| Function keys | `F8` |  |
| Function keys | `F9` |  |
| Function keys | `F10` |  |
| Function keys | `F11` |  |
| Function keys | `F12` |  |
| Function keys | `F13` |  |
| Function keys | `F14` |  |
| Function keys | `F15` |  |
| Function keys | `F16` |  |
| Function keys | `F17` |  |
| Function keys | `F18` |  |
| Function keys | `F19` |  |
| Function keys | `F20` |  |
| Function keys | `F21` |  |
| Function keys | `F22` |  |
| Function keys | `F23` |  |
| Function keys | `F24` |  |
| Number pad | `Num0` | `Numpad0` |
| Number pad | `Num1` | `Numpad1` |
| Number pad | `Num2` | `Numpad2` |
| Number pad | `Num3` | `Numpad3` |
| Number pad | `Num4` | `Numpad4` |
| Number pad | `Num5` | `Numpad5` |
| Number pad | `Num6` | `Numpad6` |
| Number pad | `Num7` | `Numpad7` |
| Number pad | `Num8` | `Numpad8` |
| Number pad | `Num9` | `Numpad9` |
| Number pad | `NumAdd` | `NumPlus`, `NumpadAdd` |
| Number pad | `NumSubtract` | `NumMinus`, `NumpadSubtract` |
| Number pad | `NumMultiply` | `NumpadMultiply` |
| Number pad | `NumDivide` | `NumpadDivide` |
| Number pad | `NumDecimal` | `NumpadDecimal` |
| Number pad | `NumEnter` | `NumpadEnter` |
| Number pad | `NumLock` |  |
| Number pad | `ScrollLock` | `Scroll Lock` |
| Media keys | `Play/Pause` | `PlayPause`, `Play Pause`, `Play` |
| Media keys | `Mute` | `Volume Mute` |
| Media keys | `Volume up` | `VolumeUp`, `Vol Up` |
| Media keys | `Volume down` | `VolumeDown`, `Vol Down` |
| Media keys | `Next track` | `NextTrack`, `Next` |
| Media keys | `Previous track` | `PrevTrack`, `Prev track`, `Previous`, `Prev` |
| Punctuation | `Plus` | `+` |
| Punctuation | `Minus` | `-`, `Dash`, `Hyphen` |
| Punctuation | `Equals` | `=`, `Equal` |
| Punctuation | `Comma` | `,` |
| Punctuation | `Period` | `.`, `Dot`, `Full stop` |
| Punctuation | `Slash` | `/` |
| Punctuation | `Backslash` | `\` |
| Punctuation | `Semicolon` | `;` |
| Punctuation | `Quote` | `'`, `Apostrophe` |
| Punctuation | `Backquote` | `` ` ``, `Grave`, `Backtick` |
| Punctuation | `BracketLeft` | `[`, `Left bracket` |
| Punctuation | `BracketRight` | `]`, `Right bracket` |
<!-- end generated: shortcut-keys -->

macOS has no `F21`–`F24`, `Insert`, `PrintScreen`, `Pause`, `ScrollLock`
or `NumLock` keys: a key bound to one of them does nothing there (the log
says so). The media keys press the OS media keys everywhere.

## How open finds the app

`open <target>` (and the object form's `"open"`) is resolved when the file
is restored, on the computer restoring it, in this order:

1. **A web address** (`https://...` or `http://...`): opens in the default
   browser.
2. **A macOS bundle id** such as `com.spotify.client` (macOS only): opens
   that app wherever it is installed.
3. **The exact path**, if it exists:
   `"open C:\\Program Files\\Spotify\\Spotify.exe"`,
   `"open /Applications/Spotify.app"`.
4. **The path with environment variables expanded**, if that exists:
   `%LOCALAPPDATA%`, `%ProgramFiles%`, `%USERPROFILE%` (Windows style),
   `$HOME` or `${HOME}` (Unix style), and a leading `~` for your home folder:
   `"open %LOCALAPPDATA%\\Programs\\Notion\\Notion.exe"`,
   `"open ~/Applications/Spotify.app"`.
5. **A Linux .desktop id** such as `spotify.desktop` (Linux only), looked up
   in the usual applications folders.
6. **An installed app's name** (no slashes): `"open Spotify"` matches the
   app picker's list (the Start Menu on Windows, the Applications folders on
   macOS), ignoring case and a trailing `.exe`, `.app` or `.lnk`. Then a
   program on PATH, like `"open notepad"`.
7. **Otherwise the key is kept, turned off**, with a note in the restore
   message saying which key and why; it shows "(off)" in the drawing. Pick
   the app again in the key panel to turn it on. Nothing is dropped, and a
   later Back up writes the key as it was.

What runs, once found:

| OS | Target | Runs |
|---|---|---|
| Windows | `.exe`, `.bat`, `.cmd`, `.com` | the program itself, with `args`, in `cwd` |
| Windows | `.lnk`, a document, a folder | opened through Explorer, like a double-click (no `args`) |
| macOS | a `.app` bundle or bundle id | `open`, with `args` passed to the app |
| macOS | an executable file | the program itself, with `args` |
| Linux | a `.desktop` file or id | `gio launch`, with `args` as files |
| Linux | an executable file | the program itself, with `args` |
| any | a document or folder | the OS opener (Explorer, `open`, `xdg-open`), no `args` |

`args` on a target that cannot take them (a Start Menu shortcut, a document)
turn the key off with a note: point `open` at the program itself instead.
Back up writes an app by its installed name when that name opens the same
thing again on this computer, else by its full path.

## Safety: Let restored keys open apps

A key that opens an app can run any program. So every `open` and every web
address in a restored file comes in **off** unless **Help & backup → More →
Let restored keys open apps** is on when you restore; the restore message
says how many keys came in off. Leave the switch off for a file someone sent
you, look at what its keys open in the Keys tab, and turn on the ones you
trust by picking the app again. Files from earlier versions follow the same
rule.

## Errors

A file with a mistake is not restored and your keys stay as they were. The
message says where the mistake is and, when it can guess, how to fix it:

- `line 7, column 15, CAM1 on Main: unknown action "opn Spotify" (did you mean "open Spotify"?)`
- `line 3, column 7, page Main: "CUTT" is not a Speed Editor key (did you mean "CUT"?)`
- `line 4, column 14, CUT on Main: "Entr" in "Ctrl+Entr" is not a key freewheel can press (did you mean "Ctrl+Enter"?)`
- `line 5, column 14, TRANS on Main: "Shift" has no key besides the modifiers (to keep it down while the key is held, write "hold Shift")`
- `line 9, column 14, ESC on Main: there is no page "Musc" in the file (did you mean "page music"?)`
- `line 12, column 14, ESC on Music: "back" works only on the key that opens this page: write "ESC": "page Music" on another page, or pick another action`
- `line 2, column 14, wheel.speed: wheel.speed must be a whole number from 1 to 10, not 11`
- `line 2, column 3, wheel: unknown field "sped" (did you mean "speed"?)`
- `line 6, column 5, page Main: CUT is on this page twice (as "CUT" and "cut")`
- A JSON mistake: `This is not a freewheel mappings file: line 3, column 25: a backslash in text must be doubled (write C:\\Tools, not C:\Tools)`
- A file from a newer freewheel: `This file is from a newer version of freewheel. Update freewheel, then try again.`

In JSON text a backslash is written twice: `"open C:\\Tools\\run.cmd"`.

Restoring can also succeed with notes, for keys it kept but turned off:

- `CAM5 on Main: there is no app or file called Photoshop on this computer. The key is off until you pick the app again.`
- `2 keys that open apps or web pages came in off: turn on “Let restored keys open apps” and restore again to keep them on.`

## A full example

```json
{
  "$schema": "https://raw.githubusercontent.com/brinkercode/freewheel/main/schema/freewheel-mappings.schema.json",
  "version": 2,
  "pages": {
    "Main": {
      "SMART INSRT": "Ctrl+C",
      "APPND": "Ctrl+V",
      "IN": "Ctrl+Shift+S",
      "OUT": "F13",
      "CUT": "Ctrl+Z",
      "DIS": "Ctrl+Y",
      "ESC": "page Music",
      "AUDIO LEVEL": "wheel volume",
      "FULL VIEW": "wheel brightness",
      "TRANS": "hold Shift",
      "SNAP": "toggle Alt",
      "CAM1": "open Spotify",
      "CAM2": "https://youtube.com",
      "CAM3": { "open": "%USERPROFILE%\\Tools\\export.cmd", "args": ["--fast", "file.txt"], "cwd": "%USERPROFILE%\\Videos" },
      "CAM4": { "open": { "windows": "Spotify", "macos": "com.spotify.client", "linux": "spotify.desktop" } },
      "CAM5": "open ~/Applications/Spotify.app",
      "STOP/PLAY": "Play/Pause",
      "SCRL": "wheel mode scroll"
    },
    "Music": {
      "ESC": "back",
      "CAM1": "Previous track",
      "CAM2": "Play/Pause",
      "CAM3": "Next track",
      "AUDIO ONLY": "Mute",
      "SMTH CUT": "scroll -3"
    }
  },
  "wheel": {
    "mode": "jog",
    "speed": 8,
    "jog": { "left": "Left", "right": "Right" },
    "shuttle": { "left": "J", "right": "L", "max_rate": 8 }
  },
  "dial": { "volume_step": 2, "app_volume_step": 5, "brightness_step": 10 },
  "settings": { "appearance": "dark" }
}
```
