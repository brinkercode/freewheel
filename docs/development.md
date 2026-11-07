# Developing freewheel

freewheel is a [Tauri 2](https://tauri.app) app: a React settings window
(`src/`) on a Rust core (`crates/freewheel-core`: config, the Speed Editor
protocol, the engine) and the app shell (`src-tauri`: device, tray, key
injection, updates).

## Set up

You need Node 22 and the Rust version in [rust-toolchain.toml](../rust-toolchain.toml)
(`rustup` installs it for you). On Linux, Tauri's system packages are needed
for the app itself; the tests and the UI build run without them.

```sh
npm ci
npx tauri dev -- -- --simulate   # the app with the simulator, no device needed
npx tauri dev                    # the app with a real Speed Editor
```

## The simulator

`--simulate` starts freewheel with a simulated Speed Editor instead of the
real one and opens a second window that drives it: connect over USB or
Bluetooth, disconnect, set the battery, press and release every key, turn the
wheel in each mode, open and close DaVinci Resolve, publish a fake update,
click the tray menu's items, and jump the settings window to any screen.
**Several set** loads sample keys on two pages.

- The simulator keeps its own config in a `simulator` folder inside the
  normal config folder, so your real keys are never read or changed.
- Key injection is off: each action shows in the Simulator's log (e.g.
  `would press Ctrl+C`). Tick **Send for real** to send it to the focused app.
- Window titles contain "Simulator".

On Windows, `freewheel-simulate.cmd` (in `src-tauri/windows/`, and next to
the exe in the test-build and e2e artifacts) starts `freewheel.exe
--simulate` with a double-click. Release zips do not include it.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="images/dark-simulator.png">
  <img alt="The Simulator window" src="images/light-simulator.png" width="600">
</picture>

## Tests

```sh
npm test                                     # UI and script tests (vitest)
npm run build                                # the UI bundle
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

CI ([ci.yml](../.github/workflows/ci.yml)) runs these on Linux, Windows and
macOS (Apple Silicon and Intel).

## End-to-end tests (Windows)

[e2e-windows.yml](../.github/workflows/e2e-windows.yml) builds the real
`freewheel.exe`, starts it with `--simulate` under WebDriver and clicks
through the first run, every tab, the key panel and every Simulator and tray
control ([scripts/e2e-windows.mjs](../scripts/e2e-windows.mjs)). Every step
is screenshotted into the `e2e-screenshots` artifact; its `readme/` folder
holds the stills used in the README (`docs/images/`). The same workflow
measures the footprint (memory, CPU, size) and runs a full portable update
against a local release server.

To refresh the README screenshots, download `e2e-screenshots` from a green
run, copy `readme/*.png` over `docs/images/` and optimise them (for example
oxipng or pngquant).

## Test builds

Every push to main builds a portable Windows exe (with
`freewheel-simulate.cmd`); see [releasing.md](releasing.md#test-builds).
