# freewheel spec

Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all

# freewheel v1 spec

## Problem
A Blackmagic DaVinci Resolve Speed Editor is useless while Resolve is closed. freewheel is a tray app that turns it into a general-purpose macro pad, and hands it back to Resolve when Resolve starts.

## Users
The owner and other Speed Editor owners on Windows 11 or macOS 15+ who want its buttons and jog wheel to drive any app.

## Goals
1. **Platform and stack**
   - Windows 11 x64 (r2q10) and macOS 15+, shipped as one universal binary (r3q4, r4q4). The macOS minimum rises with the oldest hosted CI image (r4q6).
   - Windows 10 is not supported. owner-reply:1 named 'Windows 10/11' for CI; r2q10 and r3q1 settle Windows 11 plus hosted windows-2025.
   - Tauri 2 tray app with two parts:
     - freewheel-core: a Rust library with no OS calls in its tested logic.
     - src-tauri: the app crate.
   - React + Vite settings window, tested with vitest.
   - Rust pinned to 1.99.1. Cargo.lock and package-lock.json are committed.
   - MIT LICENSE. NOTICE credits smunaut/blackmagic-misc (Apache-2.0). No code from the Unbound repo.
2. **Device**
   - hidapi opens VID 0x1edb / PID 0xda0e over USB and Bluetooth. The Bluetooth identifiers come from the S1 spike. Both buses are checked by hand on both OSes (P3).
   - Auth: the feature report 6 handshake. Re-auth is due at timeout minus 10 s.
   - Input reports: 0x04 keys (diffed into press and release), 0x03 jog, 0x07 battery.
   - Output reports: 0x02 LED mask, 0x04 jog LEDs, 0x03 set jog mode.
   - While the device is absent, the connection manager retries every 3 s.
   - Off closes the handle; On re-opens it and re-authenticates.
3. **Mapping engine**
   - Each layer maps buttons to actions:
     - Hotkey.
     - Held: down until the physical button is released, even across layers.
     - Latched: persists across layers.
     - Scroll{amount}: scrolls by the given amount through KeyInjector::scroll (P1).
     - LaunchApp: no shell is used, and imported entries need opt-in.
     - Layer push: the same button is 'back' on the target layer.
     - Set jog mode.
     - Override toggle.
   - Jog modes:
     - Jog and Scroll: left and right actions fire at a tick threshold.
     - Shuttle: repeats at a throttled rate while off-centre.
   - Dial overrides (engine modes in M3, OS controls in M4, P2):
     - System volume.
     - Foreground-app volume:
       - Windows: ISimpleAudioVolume.
       - macOS: a process tap if the C24 spike is a go, otherwise 'not available on macOS yet'.
     - Brightness: external DDC monitors and the built-in panel, best effort. Each display that fails shows its own error. Calls are throttled to one per 100 ms.
   - LEDs show the jog mode, the active layer's push button and the active override.
   - Keys and scrolling are injected through enigo behind a KeyInjector trait (key_down, key_up, tap_combo, scroll).
   - Everything held is released on Off, on disconnect and on quit.
4. **Tray**
   - On/Off toggle with a state icon.
   - Open settings and Quit.
   - Single instance.
   - 'Start at login' checkbox, off by default.
   - Battery tooltip and a notification below 15%.
   - macOS: 'Needs permission' state, checked by hand with permissions revoked (P5).
5. **Resolve hand-off**
   - sysinfo polls for Resolve.exe on Windows and Resolve on macOS. The name is editable.
   - When Resolve starts: switch Off and show the replug notification.
   - When Resolve quits: switch back On only if the auto-off did the switching.
   - Checked by hand: Resolve controls the timeline after the hand-off (P4).
6. **Config**
   - config.json with {"version":1}, in the ProjectDirs config dir, written atomically.
   - A newer version is rejected.
   - A malformed file gives an error and the app runs with defaults. The file is renamed to config.json.bad-<timestamp> before the first save, so it is never lost.
   - Default: no bindings, and the jog wheel scrolls: Scroll mode with left Scroll(-1) and right Scroll(+1) (P1).
   - Import/export uses freewheel-mappings.json. Import validates the version, asks for confirmation and backs up the current config first.
7. **Settings window**
   - Layer selector and a button table per layer.
   - An action editor for every action type, with key-combo capture.
   - Jog, override and Resolve settings.
   - Device and battery status, and per-display errors.
   - macOS Permissions section.
8. **Updates and release**
   - tauri-plugin-updater with a signed latest.json from GitHub Releases over https.
   - Releases are tag-triggered draft builds with tauri-action:
     - They run on windows-2025 and macos-15 only after the full test matrix passes.
     - Windows: unsigned NSIS per-user installer.
     - macOS: signed and notarized universal dmg with minimumSystemVersion 15.0, with any process-tap entitlement recorded by C24 (P6).
   - CI runs on windows-2025, macos-15 and macos-15-intel, using fakes only and no secrets.
   - The manual checklist per OS is the release gate.

## Non-goals
- OBS actions, cloud sync and accounts.
- Windows code signing in v1 (r3q3).
- MSI installer.
- Self-hosted runners (r3q1).
- Windows 10 and macOS older than 15.

## Constraints
- Hardware behaviour is unconfirmed until the S1 spike (Bluetooth, Input Monitoring, Resolve re-acquire) and the S2 spike (private brightness APIs).
- Signing secrets exist only in the protected 'release' environment.
- Every action in workflows is pinned to a commit SHA.
- Tapped audio is never stored or sent anywhere.

## Interfaces
- Core traits:
  - HidTransport
  - KeyInjector (key_down, key_up, tap_combo, scroll)
  - Launcher
  - VolumeControl, AppVolumeControl, BrightnessControl
  - ProcessLister
  - Clock
- Tauri commands:
  - get_config, save_config
  - import_mappings, export_mappings
  - open_permission_pane
  - set_enabled
- Status event: connection, battery, layer, override errors, permissions.
- Files: config.json and freewheel-mappings.json.
- Releases: GitHub Releases with latest.json.
