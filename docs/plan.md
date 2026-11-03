# freewheel milestone plan

Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all

PDM total: 1880 agent minutes over 44 cards; architect estimate: 1845 minutes; PDM/architect rounds: 2.

## M1 M1 Foundation
Repo skeleton, CI matrix, versioned config, and the hardware facts confirmed before any protocol work

- Scaffold Tauri 2 + React workspace with pinned toolchain and licenses (50 min)
- CI matrix on hosted Windows and macOS runners (40 min)
- Versioned config model with defaults and persistence (50 min)
- Hardware spike: USB and Bluetooth open, auth and read on Windows 11 and macOS 15, Input Monitoring, Resolve re-acquire (60 min)

## M2 M2 Device protocol
Authenticated, reconnecting Speed Editor I/O over USB and Bluetooth that can be tested without hardware

- HID transport trait and fake device (30 min)
- Port the Speed Editor auth algorithm (45 min)
- Auth handshake and re-auth scheduling (45 min)
- Parse input reports into device events (40 min)
- Encode output reports for LEDs and jog mode (30 min)
- hidapi transport over USB and Bluetooth (35 min)
- Connection manager: reconnect, On/Off and status events (40 min)

## M3 M3 Mapping engine
All mapping types, layers, jog, dial override modes, LEDs, and no stuck keys

- KeyInjector trait with enigo and mock implementations (45 min)
- Engine: hotkey, held and latched actions (40 min)
- Engine: layer stack with automatic back (45 min)
- Engine: jog modes, thresholds and shuttle throttling (50 min)
- App launch action (30 min)
- Dial override modes in the engine (45 min)
- Release everything held on Off and disconnect (30 min)
- LED state derivation (30 min)

## M4 M4 Dial overrides
System volume, foreground-app volume and brightness controls on both OSes

- Windows system volume control (30 min)
- Windows foreground-app volume control (50 min)
- Windows brightness: DDC/CI and the built-in panel (55 min)
- macOS system volume control (35 min)
- Spike: macOS private brightness APIs on Apple Silicon and Intel (45 min)
- macOS brightness for external DDC displays (55 min)
- macOS built-in display brightness through private APIs (50 min)
- Spike: can a macOS process tap mute an app's original audio? (60 min)
- macOS per-app volume: process tap and gain for one PID (60 min)
- macOS per-app volume: follow the foreground app and the audio-capture permission (45 min)

## M5 M5 Tray and hand-off
Tray app with Resolve auto-off, battery, autostart and permissions

- Tray icon, On/Off toggle, quit and single instance (45 min)
- Resolve watcher with auto-off and auto-on (50 min)
- Battery tooltip and low-battery notification (25 min)
- Start at login (20 min)
- macOS permission detection and the 'Needs permission' tray state (45 min)

## M6 M6 Settings window
Edit every mapping and setting in the React UI

- Tauri commands and status events for the settings window (50 min)
- Settings UI: layer selector and button list per layer (45 min)
- Settings UI: key action editor with key-combo capture (45 min)
- Settings UI: launch, layer, jog-mode and override action editors (45 min)
- Settings UI: jog, overrides, Resolve and device status (50 min)
- Settings UI: macOS Permissions section (25 min)
- Mapping import and export (45 min)

## M7 M7 Release
Updatable draft releases built from the tested matrix and gated by the manual checklist

- Tauri updater integration (35 min)
- Release workflow: unsigned NSIS and signed universal dmg (60 min)
- Manual hardware test checklist per OS (30 min)

## The owner's decisions on the open items
- P1 (pushback, fix accepted): P1 (api_contract, C3/C10/C13): spec §6 and C3 item 2 promise a default where 'the jog wheel scrolls', and C13 item 4 tests it. No Action variant and no KeyInjector method can scroll: JogBinding.left/right is Option<Action>, and Action has only key, launch, layer, jog-mode and override variants. Fix: add Action::Scroll{amount: i32} to the data model, add KeyInjector::scroll(amount) to C10 (MockInjector records it, EnigoInjector maps it to enigo's scroll), and make C3's default use Scroll(-1)/Scroll(+1).
- P2 (pushback, fix accepted): P2 (ordering, C15/C16 vs C17): C15 item 3 ('Active dial overrides ... stop on release_all') and C16 item 3 ('An active dial override lights its toggle button') are in M3, but override modes first exist in C17 in M4. Fix: move both items into C17's acceptance, or move C17 into M3 ahead of C15 and C16.
- P3 (pushback, fix accepted): P3 (foundation F2 real check, C9): Bluetooth is a v1 requirement, but no card checks it end to end on hardware. C9 item 5 covers USB only, and C9 is already 55 minutes with transport, manager, retry, On/Off and status events. Fix: split C9 into C9a 'hidapi transport' with a '[manual] open, auth and Press/Jog over USB and Bluetooth on Windows 11 and macOS 15' item, and C9b 'connection manager' with the retry, On/Off and status-event items.
- P4 (pushback, fix accepted): P4 (foundation F3 real check, C27): requested in round 1 and not added. C27 needs '[manual] start Resolve while freewheel is On: freewheel goes Off, notifies, and Resolve controls the timeline (after a replug only if C39 recorded that one is needed)'.
- P5 (pushback, fix accepted): P5 (foundation F4 real check, C30): requested in round 1 and not added. C30 needs '[manual] on macOS 15 with Accessibility (and Input Monitoring if C39 found it needed) revoked, the tray shows Needs permission and open_permission_pane lands on the correct System Settings pane'.
- P6 (pushback, fix accepted): P6 (ADR-10 open question, C24): C24 does not record whether the hardened runtime needs an entitlement for process taps, which C25 and C37 depend on. Add to C24 item 2: 'records whether a hardened, notarized build needs an extra entitlement beyond NSAudioCaptureUsageDescription'.

## Risks, highest first
- high: The Speed Editor's Bluetooth PID, report IDs and auth are unknown (bmd.py is USB-only). If Bluetooth differs, C7 and C9 need rework, and today that would be found only at the C38 release gate. (architect)
- high: We don't know if DaVinci Resolve picks the device up again after freewheel releases it without a replug. If it doesn't, the main hand-off promise (C27) becomes 'replug every time'. (architect)
- high: macOS built-in display brightness relies on undocumented, architecture-specific private APIs (C23), and external DDC on Apple Silicon also uses private IOAVService calls (C22). Either can break on any macOS 15.x update. (architect)
- high: The macOS per-app volume process tap (C24/C25) may not be able to mute the original stream. The spike gates this, but C25 at 60 minutes is overfull even if the spike passes. (architect)
- high: C39, C40 and C24 need a person with a Speed Editor (USB and Bluetooth), Resolve, Apple Silicon and Intel Macs and a DDC monitor. An agent cannot run them, so M1, M2 and M4 wait on the owner's hardware time. (architect)
- high: the default config's 'jog wheel scrolls' has nothing to execute it (no Scroll action, no KeyInjector scroll). Until it is fixed, C3, C10 and C13 cannot meet their acceptance. (architect)
- high: Bluetooth PID, report IDs and auth over BT are undocumented, and BT is a v1 release gate (r1q1). If S1 cannot open the device over BT on both OSes, v1 cannot ship as specified and the owner must decide again. (pdm)
- high: Resolve may not re-acquire the device after freewheel closes the HID handle. The fallback the owner chose (r2q2) is a replug notification; S1 records what actually happens. (pdm)
- high: per-app volume on macOS through a process tap is unproven. If the C24 spike is a no-go, C25 and its follow-on card are dropped and macOS shows 'not available on macOS yet' (r4q2). (pdm)
- medium: Whether macOS needs Input Monitoring for hidapi to read the device is unconfirmed. If it does, first run shows no input until the user grants it, and C30/C34 depend on knowing this. (architect)
- medium: Updates to the unsigned Windows NSIS build may trigger a SmartScreen or 'unknown publisher' prompt on every update. This is unconfirmed. (architect)
- medium: An imported freewheel-mappings.json can bind a button to launch any executable with any arguments (LaunchApp), so a shared mapping file can carry code execution (T1). (architect)
- medium: The re-auth timeout comes from a guess in bmd.py's comments ('I think ... 600'). If it is wrong, the device goes silent mid-session. (architect)
- medium: Dxva2 SetMonitorBrightness takes about 50 ms per monitor. If the override controls run on the device/engine thread, jog and key latency will stall. (architect)
- medium: The updater private key is held only by the owner. If it is lost, no update can ever reach installed users, and if it leaks, anyone can push signed updates. (architect)
- medium: Bluetooth, Resolve hand-off and Input Monitoring are first tested on real hardware only at the C38 release checklist. A wrong assumption would show up after M2-M6 are built. (architect)
- medium: running the C39 spike with bmd.py from Terminal ties the Input Monitoring permission to Terminal, not to freewheel's bundle. The result may not hold for the signed app, so C30 should keep checking both permissions. (architect)
- medium: macOS per-app volume (C24, C25, C41: 165 minutes) and macOS private brightness (C40, C22, C23: 150 minutes) are high-uncertainty. A no-go wastes the spike time and leaves 'unavailable' paths in the UI. (architect)
- medium: macos-15-intel is the only x86_64 hosted image. When it retires, A6 (CI on Intel) cannot be met without the owner deciding to drop Intel or test it manually. (architect)
- medium: The updater's private key is a long-lived secret. If it is lost, installed users can never be updated again (the updater docs say so), so the owner needs a backup plan before the first release. (pdm)
- medium: Rust 1.100.0 ships on 12 Nov 2026, so 'pin the latest stable' goes stale within days unless there is a bump policy. (pdm)
- medium: Combining the universal macOS build with the updater's latest.json platform keys (darwin-aarch64 / darwin-x86_64) is not confirmed by any page I fetched. (pdm)
- medium: Updates to the unsigned Windows build may trigger SmartScreen on each new installer. This is unverified. (pdm)
- medium: Bluetooth PID, report IDs and auth are undocumented (research gap 6). The owner made Bluetooth a v1 release gate but chose to check it by hand at release, so a Bluetooth difference found then could delay v1. (pdm)
- medium: Resolve may not re-acquire the device without a replug (gap 7). v1 handles this with the notify-and-guide notification (r2q2). (pdm)
- medium: macOS per-app volume depends on whether a process tap can mute the original audio (gap 8). If it cannot, the card ships the 'not available on macOS yet' fallback (r4q2). (pdm)
- medium: macOS built-in display brightness uses undocumented methods that differ between Intel and Apple Silicon (gap 9), and a macOS update could break it. That case shows an error, as the owner accepted in r3q6. (pdm)
- medium: It is unconfirmed whether macOS needs Input Monitoring permission for hidapi (gap 5). The permissions card covers both cases and the manual checklist verifies it. (pdm)
- medium: Losing the updater signing key ends updates for every installed user. The owner never answered who holds it. Default: the owner generates it and keeps it in a GitHub secret plus an offline backup. No key value is written anywhere. (pdm)
- medium: How the universal macOS build maps to the darwin-aarch64 and darwin-x86_64 keys in latest.json is unconfirmed (gap 3). The release card has an acceptance item that checks it. (pdm)
- medium: Updating the unsigned Windows build may show SmartScreen or 'unknown publisher' prompts on each update (gap 4). This is accepted with the unsigned choice (r3q3) and recorded on the checklist. (pdm)
- medium: The two leftover bytes in the jog protocol (gap 11) are passed through as in bmd.py. Their meaning is unverified. (pdm)
- medium: Under 'Move with CI' (r4q6), the macOS 15 minimum rises when GitHub retires the macos-15 runners. There is no date for that yet (gap 12). (pdm)
- medium: macOS brightness for the built-in panel and Apple Silicon DDC relies on private symbols that a macOS update can break. S2 may come back no-go for one architecture, which hides that path. (pdm)
- medium: Windows 11 is never exercised in CI (hosted windows-2025 only, r3q1). Behaviour specific to Windows 11 is caught only by the manual checklist. (pdm)
- medium: owner-reply:1 mentioned 'Windows 10/11'. The scope follows r2q10 (Windows 11 minimum), so Windows 10 users are unsupported; the owner should confirm this at sign-off. (pdm)
- medium: macOS CI images will retire and the macOS minimum moves with them (r4q6), so the supported OS floor will rise during maintenance. (pdm)
- low: enigo/SendInput cannot inject into elevated (admin) windows on Windows because of UIPI, so mappings silently do nothing there. (architect)
- low: How long the macos-15-intel hosted runner will stay available is unknown, so Intel CI coverage could disappear mid-life. (architect)
- low: Windows 10 support is ambiguous (owner-reply:1 vs the spec). Windows 10 is past end of support and is not in the manual checklist. (architect)
- low: the Rust 1.99.1 pin goes stale when 1.100 ships on 12 Nov 2026. The bump policy is still a default (ADR-14). (architect)
- low: owner-reply:1 says 'Windows 10/11'. The spec relies on later answers r2q10 and r3q1 for Windows 11 only, so the owner should confirm this at sign-off. (architect)
- low: each update of the unsigned NSIS build may trigger a SmartScreen prompt. This is accepted for v1 (ADR-12, ADR-13). (architect)
- low: Windows builds are unsigned, so users see a SmartScreen warning on install and on every update. (pdm)
- low: Rust 1.99.1 is pinned as the owner instructed (r4q5). If that release is unavailable on hosted runners, C1 and C2 will fail until the pin is corrected. (pdm)
