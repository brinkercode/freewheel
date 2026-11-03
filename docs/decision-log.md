# freewheel decision log

Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all

## Decisions

| ADR | title | chosen option |
| --- | --- | --- |
| ADR-001 | ADR-01 Workspace split: OS-free core crate plus Tauri shell, with every OS effect behind a trait | freewheel-core lib (no Tauri, no OS calls in tested logic) + src-tauri app crate, with traits HidTransport, KeyInjector, Launcher, VolumeControl, AppVolumeControl, BrightnessControl, ProcessLister, Clock |
| ADR-002 | ADR-02 HID access through the hidapi crate | hidapi crate |
| ADR-003 | ADR-03 Clean-room auth port from smunaut bmd.py (Apache-2.0) | Port from smunaut/blackmagic-misc bmd.py with NOTICE attribution |
| ADR-004 | ADR-04 Concurrency: one device thread owns the transport and the engine; slow OS controls run on a worker | Dedicated std thread owning HidTransport + engine, with commands over a channel and blocking reads with a timeout |
| ADR-005 | ADR-05 Key and scroll injection through enigo behind KeyInjector | enigo behind a KeyInjector trait |
| ADR-006 | ADR-06 Config: versioned JSON (serde_json) in the ProjectDirs config dir, written atomically | config.json with {version} via serde_json |
| ADR-007 | ADR-07 Layer semantics: a stack, with automatic back and held/latched keys persisting | Layer stack; the push button pops on the target layer; held keys stay down until their physical release; latched keys persist |
| ADR-008 | ADR-08 Resolve hand-off by sysinfo polling | Poll the process list with sysinfo |
| ADR-009 | ADR-09 Dial overrides with native APIs per OS behind control traits, best effort, with per-display errors | Native per OS: Windows IAudioEndpointVolume / ISimpleAudioVolume / Dxva2 + WMI; macOS Core Audio / DDC / private DisplayServices |
| ADR-010 | ADR-10 macOS per-app volume through a Core Audio process tap, gated by spike C24 | Process tap with muting plus playback at gain, gated by spike C24 |
| ADR-011 | ADR-11 CI on GitHub-hosted windows-2025, macos-15 and macos-15-intel with fakes only | Hosted windows-2025 + macos-15 + macos-15-intel |
| ADR-012 | ADR-12 Release: tauri-action draft releases, unsigned per-user NSIS, Developer ID notarized universal dmg | Unsigned NSIS + notarized universal dmg via tauri-action drafts |
| ADR-013 | ADR-13 Updates through tauri-plugin-updater with a static latest.json on GitHub Releases | tauri-plugin-updater + latest.json on GitHub Releases |
| ADR-014 | ADR-14 Pinned toolchain Rust 1.99.1, committed Cargo.lock and package-lock.json | rust-toolchain.toml channel 1.99.1 + rust-version + committed lockfiles |
| ADR-015 | ADR-15 Settings UI with React + Vite and generated TS types; MIT licence | React + Vite (create-tauri-app), TS types generated from Rust (ts-rs) |
| ADR-016 | ADR-16 Supported platforms: Windows 11 x64 and macOS 15+ universal; the macOS minimum follows the oldest hosted CI image | Windows 11 x64 + macOS 15+ universal |
| ADR-017 | ADR-17 Imported LaunchApp bindings are disabled until the user explicitly opts in | Import LaunchApp bindings disabled, with explicit opt-in |

## Review lenses

| lens | status | note | trigger |
| --- | --- | --- | --- |
| Security | decided | Updater signature and TLS: ADR-13, T3. CI has no secrets and least privilege: ADR-11, T4. Release secrets stay in the protected environment and actions are SHA-pinned: ADR-12, T5-T7. Imported LaunchApp needs opt-in and there is no shell: ADR-17, T1/T2 (owner r4q10). IPC capabilities are minimal and save_config validates: T8. Vector generation is sandboxed: T9. |  |
| Reproducibility | decided | Rust 1.99.1 pin, committed lockfiles, npm ci, cargo --locked, and releases built on the same toolchain (ADR-14, ADR-12) [A7]. The bump cadence is an open default in ADR-14, to settle before C2. |  |
| Cost | decided | Agent cost is 1845 minutes for 43 cards; the P3 split adds about 15. Cash cost is the Apple Developer Program at <cost> a year (ADR-12). Hosted runners are free for public repos, and Windows signing is skipped in v1. Owner hardware time is needed for C39, C40, C24 and the C38 checklist. |  |
| Operations / CI | decided | The hosted matrix runs fmt, clippy, tests and the UI build with fakes only (ADR-11). C37 builds with needs: on the reused C2 workflow. Retirement of the Intel image is an open default in ADR-11. |  |
| Distribution / release | open | Unsigned NSIS plus notarized universal dmg drafts and the updater are decided (ADR-12, ADR-13). Still open: whether the universal bundle yields both darwin keys in latest.json, and how SmartScreen behaves on update. | decide before card "Release workflow: unsigned NSIS and signed universal dmg"; default: Do a dry-run draft and post-process latest.json if a darwin key is missing. Accept SmartScreen prompts in v1. |
| Compatibility | decided | Windows 11 x64 and macOS 15+ universal (ADR-16, spec §1 r2q10/r3q1). Windows 10 is excluded even though owner-reply:1 named it, so the owner should confirm this at sign-off. |  |
| Data / privacy | decided | No telemetry and no accounts, and the config stays local. The process list is only matched against the configured name. Tapped audio is never written or sent anywhere [A21]. The updater reveals the user's IP to GitHub only (ADR-13). |  |

## Open questions

- ADR-002: Does the Speed Editor appear over Bluetooth with the same VID/PID, report IDs and feature-report-6 auth? (decide before card "Parse input reports into device events"; default: Treat Bluetooth as identical to USB. If C39 shows otherwise, add a transport variant.)
- ADR-002: Does macOS need Input Monitoring before hidapi can read this device? (decide before card "macOS permission detection and the 'Needs permission' tray state"; default: Check and show both Accessibility and Input Monitoring.)
- ADR-003: Is the status u16 really the re-auth timeout in seconds? bmd.py's author only says 'I think ... 600'. (decide before card "Auth handshake and re-auth scheduling"; default: Re-auth at min(timeout, 600) - 10 s, and treat silence after that as a disconnect. C39 checks this on hardware.)
- ADR-005: Is the default dial action a scroll-wheel event or arrow keys? (decide before card "Versioned config model with defaults and persistence"; default: Scroll-wheel events through a new Action::Scroll{amount} and KeyInjector::scroll (P1).)
- ADR-007: What is the maximum layer stack depth? (decide before card "Engine: layer stack with automatic back"; default: 8. Pushing deeper is ignored and logged.)
- ADR-008: How often should the process list be polled? (decide before card "Resolve watcher with auto-off and auto-on"; default: Every 2 s, refreshing only process names.)
- ADR-008: If Resolve cannot re-acquire the device without a replug, should the flow stay notify-only? (decide before card "Resolve watcher with auto-off and auto-on"; default: Keep the replug notification, worded to match what C39 observes.)
- ADR-009: Do the macOS private brightness symbols work on macOS 15 for arm64 and x86_64? (decide before card "macOS brightness for external DDC displays"; default: Spike C40 decides. On no-go, settings shows 'built-in display brightness unavailable' and hides the path.)
- ADR-010: Does the hardened runtime need an audio entitlement for taps, in addition to NSAudioCaptureUsageDescription? (decide before card "macOS per-app volume: process tap and gain for one PID"; default: C24 records this (P6). C37's entitlements file adds it only if needed.)
- ADR-011: When the last hosted Intel macOS image retires, should Intel be dropped from the universal build or kept as manual-only testing? (decide before card "CI matrix on hosted Windows and macOS runners"; default: Keep the universal build. Intel is then covered only by the C38 Intel checklist section, and the owner decides at that time.)
- ADR-012: Does the universal bundle produce both darwin-aarch64 and darwin-x86_64 keys in latest.json? (decide before card "Release workflow: unsigned NSIS and signed universal dmg"; default: Do a dry-run draft (C37 item 5). If either key is missing, post-process latest.json in the workflow.)
- ADR-013: Does each update of the unsigned NSIS build trigger a SmartScreen prompt? (decide before card "Manual hardware test checklist per OS"; default: Accept the prompt for v1 and note it in the release notes. C38 records what is observed.)
- ADR-014: Who bumps the pin, and when (1.100.0 is due 12 Nov 2026)? (decide before card "CI matrix on hosted Windows and macOS runners"; default: A manual bump PR after each stable release, gated by the full CI matrix.)
- ADR-015: ts-rs or specta for type generation? (decide before card "Tauri commands and status events for the settings window"; default: ts-rs, with a CI check that the regenerated files have no diff.)
- lens Distribution / release: Unsigned NSIS plus notarized universal dmg drafts and the updater are decided (ADR-12, ADR-13). Still open: whether the universal bundle yields both darwin keys in latest.json, and how SmartScreen behaves on update. (decide before card "Release workflow: unsigned NSIS and signed universal dmg"; default: Do a dry-run draft and post-process latest.json if a darwin key is missing. Accept SmartScreen prompts in v1.)

## Foundations

| foundation | uncertainty | spike | exit criteria | real check |
| --- | --- | --- | --- | --- |
| F1 Speed Editor USB HID protocol and feature-report-6 auth | low |  |  | C9 item 5: '[manual, A2/A3] With a real Speed Editor over USB on Windows 11 and macOS 15, the manager opens, authenticates and logs a Press event and a Jog event' |
| F2 Bluetooth HID link (PID, report IDs, auth) | high | Hardware spike: USB and Bluetooth open, auth and read on Windows 11 and macOS 15, Input Monitoring, Resolve re-acquire | The Bluetooth VID/PID, bus and report IDs are recorded from hidapi enumeration on both OSes; Feature-report-6 auth either succeeds over Bluetooth or the difference is documented in docs/adr/0001; Go or no-go for Bluetooth in v1 is recorded | C9a (P3, to add): '[manual] open, auth and Press/Jog over Bluetooth on Windows 11 and macOS 15' |
| F3 DaVinci Resolve re-acquires the device after release | high | Hardware spike: USB and Bluetooth open, auth and read on Windows 11 and macOS 15, Input Monitoring, Resolve re-acquire | With Resolve running, closing freewheel's handle is either followed by Resolve seeing the device without a replug, or the need for a replug is recorded; The behaviour is recorded per OS in docs/adr/0001 | C27 (P4, to add): '[manual] start Resolve while freewheel is On; freewheel goes Off and Resolve controls the timeline' |
| F4 macOS Input Monitoring needed for hidapi reads | high | Hardware spike: USB and Bluetooth open, auth and read on Windows 11 and macOS 15, Input Monitoring, Resolve re-acquire | On macOS 15, a process without Input Monitoring either receives input reports or does not, and the result is recorded along with which process held the permission | C30 (P5, to add): '[manual] with the permission revoked, the tray shows Needs permission and open_permission_pane lands on the correct pane' |
| F5 macOS process tap as per-app volume | high | Spike: can a macOS process tap mute an app's original audio? | A prototype mutes one PID's original audio and plays it back at an adjustable gain; Go or no-go and the entitlement needs are recorded in docs/adr | C25 item 5: 'A manual checklist item covers the gain change on one app' |
| F6 macOS brightness via private APIs (built-in) and DDC (external) | high | Spike: macOS private brightness APIs on Apple Silicon and Intel | Symbols resolved through dlsym change the built-in display's brightness on arm64 and x86_64 macOS 15, or no-go is recorded; DDC brightness changes on one external display per architecture, or no-go is recorded; Each no-go path is listed for C22/C23 to report as unavailable | C22 item 4 and C23 item 4 (manual checklist on both architectures) |
| F7 Windows Core Audio, Dxva2 and WMI brightness | low |  |  | C18 item 4: 'A manual checklist item covers it' |
| F8 OS key injection via enigo | low |  |  | C10 item 4: '[manual] A debug command taps Shift+A into a text field on Windows 11 and macOS 15' |
| F9 Hosted CI runners with the pinned toolchain | low |  |  | C2 item 3: the steps run on all three runners in the real workflow |
| F10 Signing, notarization, updater feed | low |  |  | C37 item 5: 'latest.json lists windows-x86_64, darwin-aarch64 and darwin-x86_64 (checked on a dry-run draft)' |

## The owner's decisions on the open items

- P1 (pushback, fix accepted): P1 (api_contract, C3/C10/C13): spec §6 and C3 item 2 promise a default where 'the jog wheel scrolls', and C13 item 4 tests it. No Action variant and no KeyInjector method can scroll: JogBinding.left/right is Option<Action>, and Action has only key, launch, layer, jog-mode and override variants. Fix: add Action::Scroll{amount: i32} to the data model, add KeyInjector::scroll(amount) to C10 (MockInjector records it, EnigoInjector maps it to enigo's scroll), and make C3's default use Scroll(-1)/Scroll(+1).
- P2 (pushback, fix accepted): P2 (ordering, C15/C16 vs C17): C15 item 3 ('Active dial overrides ... stop on release_all') and C16 item 3 ('An active dial override lights its toggle button') are in M3, but override modes first exist in C17 in M4. Fix: move both items into C17's acceptance, or move C17 into M3 ahead of C15 and C16.
- P3 (pushback, fix accepted): P3 (foundation F2 real check, C9): Bluetooth is a v1 requirement, but no card checks it end to end on hardware. C9 item 5 covers USB only, and C9 is already 55 minutes with transport, manager, retry, On/Off and status events. Fix: split C9 into C9a 'hidapi transport' with a '[manual] open, auth and Press/Jog over USB and Bluetooth on Windows 11 and macOS 15' item, and C9b 'connection manager' with the retry, On/Off and status-event items.
- P4 (pushback, fix accepted): P4 (foundation F3 real check, C27): requested in round 1 and not added. C27 needs '[manual] start Resolve while freewheel is On: freewheel goes Off, notifies, and Resolve controls the timeline (after a replug only if C39 recorded that one is needed)'.
- P5 (pushback, fix accepted): P5 (foundation F4 real check, C30): requested in round 1 and not added. C30 needs '[manual] on macOS 15 with Accessibility (and Input Monitoring if C39 found it needed) revoked, the tray shows Needs permission and open_permission_pane lands on the correct System Settings pane'.
- P6 (pushback, fix accepted): P6 (ADR-10 open question, C24): C24 does not record whether the hardened runtime needs an entitlement for process taps, which C25 and C37 depend on. Add to C24 item 2: 'records whether a hardened, notarized build needs an extra entitlement beyond NSAudioCaptureUsageDescription'.

## Owner question rounds

### Round 1

- r1q1: Is Bluetooth support in v1, or USB only?
  answered by brinkercode (cli):
  > C · USB + BT required
- r1q2: How should freewheel hand the device back to DaVinci Resolve?
  answered by brinkercode (cli):
  > B · Auto-off on Resolve
- r1q3: Which dial override modes (a button switches the jog wheel to a system control) are in v1?
  answered by brinkercode (cli):
  > D · All three
- r1q4: On a layer change, what happens to keys that are held or latched?
  answered by brinkercode (cli):
  > C · Keep both
- r1q5: Key injection: use the enigo crate, or write our own SendInput/CGEvent bindings?
  answered by brinkercode (cli):
  > A · enigo
- r1q6: Which frontend should the settings window use?
  answered by brinkercode (cli):
  > C · React
- r1q7: How is v1 distributed: unsigned or signed builds?
  answered by brinkercode (cli):
  > C · Both signed
- r1q8: Which open-source license?
  answered by brinkercode (cli):
  > B · MIT
- r1q9: Should the button LEDs show freewheel's state?
  answered by brinkercode (cli):
  > C · Full state
- r1q10: Is launch at login in v1?
  answered by brinkercode (cli):
  > B · Yes, default off

Settled by research, not asked:

- Device identity and transport library: USB VID 0x1edb / PID 0xda0e, accessed with the hidapi crate (MIT), which has feature-report and read/write APIs (brief:Prior art; https://docs.rs/hidapi/latest/hidapi/)
- Protocol and auth source: Re-implement in Rust from smunaut/blackmagic-misc bmd.py (Apache-2.0), with attribution in NOTICE; no code from Unbound (brief:Prior art; https://github.com/smunaut/blackmagic-misc)
- Out of scope for v1: OBS websocket actions, cloud sync and Clerk sign-in/accounts (brief:Prior art)
- Shell and platforms: Tauri 2 tray app on Windows and macOS, with an On/Off tray toggle and a small settings window (brief:What; brief:Constraints and preferences; https://v2.tauri.app/learn/system-tray/)
- Config location: Versioned config file in the OS config dir, via the directories crate's ProjectDirs (brief:Constraints and preferences; https://docs.rs/directories/latest/directories/)
- Single instance: Use tauri-plugin-single-instance so a second instance cannot compete for the HID device (https://v2.tauri.app/plugin/single-instance/)
- Testability and CI: Protocol parsing, auth and the mapping engine sit behind traits with unit tests; CI runs on GitHub-hosted Windows and macOS runners (brief:Constraints and preferences)
- Reconnect: Retry every few seconds while the device is absent (Unbound uses 3 s) (brief:Prior art; https://github.com/PuzzleEmptyM/Unbound-editor-device-customizer)
- Layer escape: Layers are a stack; the push button auto-acts as 'back' on the target layer (brief:Prior art)

### Round 2

- r2q1: Bluetooth is a v1 release gate, but its HID protocol (PID, report IDs, auth) is undocumented. Do you have a Bluetooth-capable Speed Editor and both OSes to capture reports on, and should a hardware spike come first?
  answered by brinkercode (cli):
  > B · Build, verify later
- r2q2: If testing shows that Resolve cannot take the device back after freewheel releases it (it needs a replug or a Resolve restart), what should v1 do?
  answered by brinkercode (cli):
  > A · Notify + guide
- r2q3: Per-app volume on macOS has no simple public API (it needs Core Audio process taps on 14.2+, or a virtual driver). What ships on macOS?
  answered by brinkercode (cli):
  > B · macOS 14.2+ taps
- r2q4: Which displays should brightness mode control?
  answered by brinkercode (cli):
  > A · Both, best effort
- r2q5: For signed builds: do you have a paid Apple Developer account already, and which Windows signing route should we use?
  answered by brinkercode (cli):
  > A · Apple + Azure
- r2q6: On Windows, which app does per-app volume control?
  answered by brinkercode (cli):
  > B · Foreground app
- r2q7: If you switched freewheel Off yourself before Resolve started, should it still switch back On when Resolve quits?
  answered by brinkercode (cli):
  > A · Respect manual Off
- r2q8: GitHub-hosted Windows runners are Windows Server 2022/2025 only (there is no Windows 10/11 client image). Is that an acceptable stand-in for Windows 10/11 x64 in CI?
  answered by brinkercode (cli):
  > Windows 11 on my own self-hosted runner (not Windows 10, not Windows Server images)
- r2q9: The device reports battery (charging, level) over Bluetooth. Should freewheel show it?
  answered by brinkercode (cli):
  > A · Tray + low warn
- r2q10: Which minimum OS versions does v1 support?
  answered by brinkercode (cli):
  > B · Win11 + macOS 14

Settled by research, not asked:

- Auto-on after Resolve quits: Settled by the owner: freewheel switches back On when Resolve quits. The only open edge case is a manual Off made before Resolve started (asked below). (owner-answer:r1q2)
- Key injection: enigo behind a KeyInjector trait, with a mock injector in tests (owner-answer:r1q5)
- Settings frontend: React + Vite, from the create-tauri-app React template (owner-answer:r1q6; https://v2.tauri.app/start/create-project/)
- License: Single MIT LICENSE, plus NOTICE with the Apache-2.0 attribution to smunaut (owner-answer:r1q8)
- Layer change semantics: Held keys stay down until their physical button is released; latched keys persist (owner-answer:r1q4)
- LEDs: Full state: jog mode LEDs, the active layer's push button and the dial override button (owner-answer:r1q9)
- Launch at login: In v1 through tauri-plugin-autostart, as a tray checkbox that is off on first run (owner-answer:r1q10; https://v2.tauri.app/plugin/autostart/)
- macOS Intel CI: GitHub-hosted Intel macOS runners exist as macos-15-intel and macos-26-intel; arm64 as macos-15 and macos-latest. macOS 14 images were retired on 2026-11-02. (https://github.com/actions/runner-images)
- CI and manual testing: Full test suite on Windows plus macOS arm64 and Intel, with release builds from the same matrix. Hardware behaviour gets a written manual checklist per OS, run before each release. (owner-reply:1)
- Single instance: tauri-plugin-single-instance stops a second instance from competing for the HID device (https://v2.tauri.app/plugin/single-instance/)

### Round 3

- r3q1: Windows CI conflict: your ticket reply asked for GitHub-hosted Windows 10/11 x64, but r2q8 chose your own self-hosted Windows 11 runner. GitHub only hosts Windows Server x64 (Windows 11 is Arm64 only). Which runs where?
  answered by brinkercode (cli):
  > C · Hosted only
- r3q2: The self-hosted Windows runner on a public repo: may it run only on pushes to main and on PRs a maintainer has approved (never on fork PRs automatically), with no secrets?
  answered by brinkercode (cli):
  > A · Locked down
- r3q3: Azure Artifact Signing Public Trust is limited to individual developers in the US or Canada, or organisations in listed countries. Which are you?
  answered by brinkercode (cli):
  > D · Unsigned Win v1
- r3q4: GitHub no longer hosts macOS 14 runners (retired 2 Nov 2026). macOS CI can run on 15 arm64 + 15 Intel. How do we cover your macOS 14 minimum?
  answered by brinkercode (cli):
  > B · Raise to macOS 15
- r3q5: Per-app volume on macOS captures the app's audio through a process tap. macOS will ask the user for audio-capture ('record system audio') permission. When should that prompt appear?
  answered by brinkercode (cli):
  > A · On first use
- r3q6: macOS built-in display brightness has no public API: only undocumented methods, which differ on Intel and Apple Silicon. Is relying on them OK?
  answered by brinkercode (cli):
  > A · Yes, best effort
- r3q7: Auto-off needs DaVinci Resolve's process name. Can you confirm what Task Manager and Activity Monitor show on your machines?
  answered by brinkercode (cli):
  > A · Resolve.exe/Resolve
- r3q8: The brief allows a versioned JSON or TOML config file. Which one?
  answered by brinkercode (cli):
  > A · JSON
- r3q9: What does a fresh install map the device to?
  answered by brinkercode (cli):
  > A · Empty + jog scroll

Settled by research, not asked:

- Minimum macOS for per-app volume taps: AudioHardwareCreateProcessTap is introduced in macOS 14.2 per Apple's own docs, so the owner's 14.2 gate stands (AudioCap's 14.4 note is superseded). (https://developer.apple.com/tutorials/data/documentation/coreaudio/audiohardwarecreateprocesstap(_:_:).json)
- Can a tap mute the original stream: CATapDescription.muteBehavior is a settable property, so capture-and-replay at a new gain can mute the source; feasibility is no longer an open question, only the work size. (https://developer.apple.com/tutorials/data/documentation/coreaudio/catapdescription/mutebehavior.json)
- Hosted macOS runners: Hosted runners provide arm64 (macos-15, macos-26) and Intel (macos-15-intel, macos-26-intel). macOS 14 images were retired on 2 Nov 2026, so hosted CI cannot test the macOS 14 minimum. (https://raw.githubusercontent.com/actions/runner-images/main/README.md)
- Hosted Windows 11 runners: The only hosted Windows 11 image is Arm64 (windows-11-arm); x64 hosted images are Windows Server 2022/2025. (https://raw.githubusercontent.com/actions/runner-images/main/README.md)
- Windows 10 support: The owner set Windows 11 as the minimum (r2q10), so Windows 10 is out of the CI and test matrix despite the earlier 'Windows 10/11' wording. (owner-answer:r2q10)
- Per-app volume target on macOS: The owner chose the foreground app for Windows. The same rule applies on macOS for one consistent behaviour; this is not asked again. (owner-answer:r2q6)
- Per-app volume on macOS in v1: The owner chose macOS 14.2+ process taps in v1 (r2q3). That answer is binding, so it is not re-asked as v1 vs v1.1. (owner-answer:r2q3)

### Round 4

- r4q1: Per-app volume on macOS: which app does the dial control? (On Windows you chose the foreground app.)
  answered by brinkercode (cli):
  > A · Foreground app
- r4q2: If the macOS tap spike shows a process tap cannot mute the app's original audio (so it can't act as a volume control), what ships in v1?
  answered by brinkercode (cli):
  > A · Hide on macOS
- r4q3: Which Windows installer ships (unsigned) in v1?
  answered by brinkercode (cli):
  > A · NSIS only
- r4q4: How should the macOS build be packaged?
  answered by brinkercode (cli):
  > A · Universal
- r4q5: Which Rust version does the project pin?
  answered by brinkercode (cli):
  > Pin the latest current stable Rust: 1.99.1 (released 2026-10-13). rust-version = "1.99.1" in Cargo.toml and rust-toolchain.toml pinned to 1.99.1; CI uses that pinned toolchain.
- r4q6: When GitHub retires the macos-15 runners (only 2 GA images per OS are kept, and macOS 26 is already GA), what happens to the macOS 15 minimum?
  answered by brinkercode (cli):
  > A · Move with CI
- r4q7: macOS needs Accessibility permission for key injection, and possibly Input Monitoring to read the device (unconfirmed). How should first run guide the user?
  answered by brinkercode (cli):
  > A · Tray + settings
- r4q8: How does the settings window show the device for editing mappings?
  answered by brinkercode (cli):
  > A · Button list
- r4q9: Does v1 include automatic updates?
  answered by brinkercode (cli):
  > B · Tauri updater
- r4q10: Can mappings be exported and imported in the settings window?
  answered by brinkercode (cli):
  > B · Import/export

Settled by research, not asked:

- Manual hardware checklist as release gate: Yes. A written per-OS checklist (USB and BT connect, auth, Resolve hand-off, LEDs, dial modes) is run by hand before each release. (owner-reply:1)
- Windows CI: Hosted windows-2025 x64 only. Windows 11 is covered by the manual checklist. The self-hosted runner and its lockdown rules (r3q2) are dropped. (owner-answer:r3q1)
- Windows signing: Unsigned in v1. Azure Artifact Signing and OV certificates are out of scope. (owner-answer:r3q3)
- macOS minimum: macOS 15, enforced with bundle.macOS.minimumSystemVersion. The 14.2 vs 14.4 tap-availability question no longer applies. (owner-answer:r3q4; https://v2.tauri.app/distribute/macos-application-bundle/)
- Windows WebView2: Built into Windows 11, so the default installer bootstrapper is enough. (https://v2.tauri.app/distribute/windows-installer/)
- Config format: config.json with "version": 1, serialised with serde_json. (owner-answer:r3q8)
- Default mapping: No key bindings; the jog wheel scrolls. (owner-answer:r3q9)
- Resolve process match: Resolve.exe on Windows and Resolve on macOS, editable in settings. (owner-answer:r3q7)
- OS API tests in CI: Every test uses fakes behind traits, so whether Windows Server images have Windows 11-only APIs does not matter. (research-survey (enigo entry, gap 11) + brief:Constraints)

Why the PDM was done asking: Four question rounds (38 answers) settled every high-impact decision:
- Platforms and minimums: Windows 11 and macOS 15, moving with CI.
- Transports: USB plus Bluetooth, with Bluetooth verified at release.
- Resolve hand-off: auto-off, respecting a manual Off, with notify-and-guide.
- Held and latched keys across layers.
- All three overrides, with targets, fallbacks and permission timing.
- Injection through enigo; React UI with a button list.
- MIT license.
- Signing: macOS signed, Windows unsigned NSIS; universal dmg.
- CI on hosted runners only, with Rust pinned to 1.99.1.
- JSON config with empty defaults plus jog scroll.
- Updater; import/export that replaces mappings.

The research questions left over are answered by r4q9, r4q10 and r4q5 or set as safe defaults for sign-off. The remaining unknowns are hardware facts, handled by the spike card and the manual checklist, not product choices.
