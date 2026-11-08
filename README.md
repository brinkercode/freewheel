<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset=".github/assets/freewheel-logo-dark.svg"><img alt="freewheel: No telemetry, no accounts, no analytics, always portable" src=".github/assets/freewheel-logo-light.svg" width="800"></picture>
  <br><br>
  <a href="https://github.com/brinkercode/freewheel/releases/latest"><img src="https://img.shields.io/github/v/release/brinkercode/freewheel?sort=semver" alt="Release"></a> <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="License"></a> <a href="https://github.com/brinkercode/freewheel/actions/workflows/ci.yml"><img src="https://github.com/brinkercode/freewheel/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
</p>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/dark-keys.png">
  <img alt="freewheel's Keys tab: a drawing of the Speed Editor with STOP/PLAY selected and its panel open, set to the Play/Pause media key" src="docs/images/light-keys.png" width="1000">
</picture>

## What it does

Use a Blackmagic Speed Editor with any app, without DaVinci Resolve.

- **Remap every key** to a shortcut, a media key, an app, a file or a website.
- **Pages:** switch the keys between sets of jobs with a key.
- **Wheel modes:** Jog, Shuttle, Scroll, or a knob for volume or screen brightness.
- **Steps aside for DaVinci Resolve** while it's open.
- **Key lights** show the wheel mode, toggled keys and the page.
- **Lives in the tray**, updates itself, and backs up to a file you can edit by hand ([SCHEMA.md](SCHEMA.md)).
- **Small:** about 31 MB of memory and 0.15% of one CPU core idle in the tray (measured on Windows CI).

Works over USB and Bluetooth on Windows 11, macOS 15 and Linux.

## Download and run

Download the file for your system from [Releases](https://github.com/brinkercode/freewheel/releases/latest) and run it. Nothing to install.

- **Windows:** `freewheel-<version>-windows-x64.exe`. Keep it somewhere you can write to (not Program Files) so it can update itself. SmartScreen may ask the first time: **More info → Run anyway**.
- **macOS:** `freewheel-<version>-macos-universal.dmg`. Drag freewheel to Applications and open it. The first time, allow it in **System Settings → Privacy & Security → Open Anyway**, then under **Accessibility** and **Input Monitoring**.
- **Linux:** `freewheel-<version>-linux-x86_64.AppImage`. Keep it somewhere you can write to (e.g. `~/Applications`) so it can update itself, make it executable and run it (needs FUSE 2: `libfuse2`, on Ubuntu 24.04 `libfuse2t64`); the first start asks once for permission to use the Speed Editor.

Settings live in your user folder, so replacing or moving the app keeps them. How updates work: [docs/updates.md](docs/updates.md).

## Screens

| Wheel | DaVinci Resolve |
| --- | --- |
| <img alt="Wheel tab" src="docs/images/light-wheel.png" width="400"> | <img alt="DaVinci Resolve tab" src="docs/images/light-resolve.png" width="400"> |
| **General** | **Help & backup** |
| <img alt="General tab" src="docs/images/light-general.png" width="400"> | <img alt="Help & backup tab" src="docs/images/light-help.png" width="400"> |

## Troubleshooting

- **Help & backup → More → Device test** shows the connection, battery, keys and wheel live; **Copy diagnostics** copies it as text.
- **Download logs** (Help & backup, or the tray) saves a log file to Downloads; attach it to a [bug report](https://github.com/brinkercode/freewheel/issues/new/choose). Turn on **detailed logs** first for more.
- Step-by-step USB and Bluetooth test: [docs/device-testing.md](docs/device-testing.md).

Building from source: see [docs/development.md](docs/development.md).
