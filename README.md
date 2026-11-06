# freewheel

## Install on Windows

The NSIS installer needs nothing else: it downloads and installs WebView2 if
it is missing. The portable `freewheel.exe` has the C runtime built in but
needs WebView2, which every Windows 11 has built in.

## Try it without a Speed Editor

Run `freewheel --simulate`. The app starts with a simulated device instead of the real one and opens an
extra Simulator window: connect over USB or Bluetooth, disconnect, set the
battery, press and release every key, turn the wheel in each wheel mode, and
switch freewheel Off and On. **Load sample mappings** binds several keys and a
layer page so you can see pages and lit keys change.

- Simulator mode keeps its own config in a `simulator` folder inside the normal
  config folder, so your real mappings are never read or changed.
- Key injection is off: each action shows in the Simulator window and the log
  as e.g. `would press Ctrl+C`. Tick **Send for real** to send the keys to the
  focused app.
- Window titles contain "Simulator" so you can tell it from the real app.

## Test builds

Every push to main (or a manual **Run workflow**) runs the `test-build`
workflow. To get a Windows build, open that workflow's run on the repository's
**Actions** tab and download the `freewheel-windows-…` artifact from the
run's artifacts. It holds the NSIS installer (`freewheel_…_x64-setup.exe`) and
the portable `freewheel.exe`, and is kept for 1 day.

The build is unsigned, so Windows SmartScreen warns when you start it: click
**More info**, then **Run anyway**.

## Manual checklist

The release gate, checked by hand with a real Speed Editor on each OS before a
release. A draft release is published only when every item below passes on
Windows 11 and on macOS 15 on both Apple Silicon and Intel; one failure keeps
the draft unpublished. Tick the items in a copy of this list for each release.

### Windows 11

- [ ] [C9a] USB: plugged in, the Speed Editor opens and authenticates (status shows connected over USB); a Press fires its binding and a Jog turn scrolls. Confirms the USB path on real hardware.
- [ ] [C9a] Bluetooth: paired and with the USB cable out, the Speed Editor opens and authenticates (status shows connected over Bluetooth); a Press fires its binding and a Jog turn scrolls. Confirms Bluetooth on Windows 11, which the hardware spike left unconfirmed.
- [ ] Re-auth: left connected and idle past the auth timeout (up to 10 minutes) over USB and over Bluetooth, the device re-authenticates on its own and a Press still works without a replug.
- [ ] Reconnect: unplug USB (or switch the device off on Bluetooth); the status shows disconnected, and on plug-in or power-on it reconnects and authenticates within a few seconds without restarting freewheel.
- [ ] On/Off: holding a Held binding and switching Off from the tray releases the key (no stuck key in a text field), closes the device and turns the LEDs off; switching On re-opens and re-authenticates it.
- [ ] [R1] Resolve hand-off: with freewheel On, starting DaVinci Resolve switches freewheel Off and shows the replug notification; Resolve then drives the timeline with the Speed Editor, with or without a replug (record which). Quitting Resolve switches freewheel back On; after a manual Off it stays Off. Confirms whether Resolve re-acquires the device on Windows without a replug.
- [ ] LEDs: the jog-mode LED follows the jog mode, the active layer's push button is lit, and the active override's toggle button is lit; all go dark on Off.
- [ ] Key injection: the debug command taps Shift+A into a focused text field (an "A" appears) and scrolls a long page in the browser.
- [ ] Jog mode: left and right fire their actions once per tick threshold.
- [ ] Scroll mode (the default): turning the wheel scrolls the focused page up and down.
- [ ] Shuttle mode: off-centre repeats its action at a throttled rate that rises with the angle, and stops at centre.
- [ ] System volume dial override: turning the jog wheel right raises and left lowers the default output device's volume (the Windows volume flyout moves), and it follows a change of default output device.
- [ ] Brightness override: an external DDC/CI monitor and a laptop's built-in panel both step up and down with the jog wheel; with DDC/CI off on one monitor (and on a desktop with no built-in panel) that display shows its own error and the others still step.
- [ ] Per-app volume: with the dial on the app-volume override and a video playing in the focused app, turning the jog wheel moves only that app's slider in the Volume Mixer; focusing an app with no audio reports 'no audio session'.
- [ ] Battery: on Bluetooth the tray tooltip shows the battery level, and one notification appears when it drops below 15%.
- [ ] Start at login: on a fresh install the tray's "Start at login" is unchecked. Checking it adds freewheel to Settings > Apps > Startup and freewheel starts after signing out and in; unchecking it removes the entry and freewheel no longer starts. The checkbox matches the Startup entry after a restart, including when the entry was switched off there.
- [ ] Import and export: export writes freewheel-mappings.json; importing it asks for confirmation, backs up the current config first, and an imported LaunchApp stays disabled until opted in.
- [ ] SmartScreen on install: running the unsigned NSIS installer shows SmartScreen or not (record what is shown); "More info > Run anyway" installs per-user.
- [ ] Update: installing the previous release and starting it offers the new version; accepting downloads, installs and restarts into it. SmartScreen on update: record whether the update's installer prompts again.
- [ ] [T3] Update with a tampered .sig: serving a latest.json whose signature is altered (one character changed) makes the update fail with a signature error and is rejected; the installed version is unchanged.

### macOS 15 (Apple Silicon and Intel)

Run every item on an Apple Silicon Mac and on an Intel Mac, from the universal dmg.

- [ ] [C9a] USB: plugged in, the Speed Editor opens and authenticates (status shows connected over USB); a Press fires its binding and a Jog turn scrolls.
- [ ] [C9a] Bluetooth: paired and with the USB cable out, the Speed Editor opens and authenticates (status shows connected over Bluetooth); a Press fires its binding and a Jog turn scrolls.
- [ ] Re-auth: left connected and idle past the auth timeout (up to 10 minutes) over USB and over Bluetooth, the device re-authenticates on its own and a Press still works without a replug.
- [ ] Reconnect: unplug USB (or switch the device off on Bluetooth); the status shows disconnected, and on plug-in or power-on it reconnects and authenticates within a few seconds without restarting freewheel.
- [ ] On/Off: holding a Held binding and switching Off from the menu bar releases the key (no stuck key in a text field), closes the device and turns the LEDs off; switching On re-opens and re-authenticates it.
- [ ] Resolve hand-off: with freewheel On, starting DaVinci Resolve switches freewheel Off and shows the replug notification; Resolve then drives the timeline with the Speed Editor, with or without a replug (record which). Quitting Resolve switches freewheel back On; after a manual Off it stays Off.
- [ ] [P1] Permissions: on a fresh install, or with Accessibility and Input Monitoring revoked, the menu bar shows 'Needs permission (click to fix)' and open_permission_pane lands on Privacy & Security > Accessibility and > Input Monitoring; once both are granted, Press events arrive. Confirms whether macOS Input Monitoring is needed to read the device.
- [ ] LEDs: the jog-mode LED follows the jog mode, the active layer's push button is lit, and the active override's toggle button is lit; all go dark on Off.
- [ ] Key injection: the debug command taps Shift+A into a focused text field (an "A" appears) and scrolls a long page in the browser.
- [ ] Jog mode: left and right fire their actions once per tick threshold.
- [ ] Scroll mode (the default): turning the wheel scrolls the focused page up and down.
- [ ] Shuttle mode: off-centre repeats its action at a throttled rate that rises with the angle, and stops at centre.
- [ ] System volume: with the System volume override on, turning the jog wheel right raises and left lowers the default output device's volume (the menu bar volume follows), stopping at 0 and full. Switch the output (built-in speakers, then headphones or a USB/Bluetooth device) and it follows the new default. With an output that has no volume control (e.g. HDMI/DisplayPort audio), the settings window shows an override error instead.
- [ ] Brightness override: with the dial on the brightness override, turning the jog wheel steps the built-in panel (or settings shows 'built-in display brightness unavailable' and the log says why) and every DDC monitor; unplugging one monitor's DDC cable gives that display its own error while the others still step.
- [ ] Per-app volume: switching the app-volume override on for the first time shows the audio-capture prompt and the status shows 'permission needed' until it is allowed; then, with audio in two apps, turning the jog wheel changes only the frontmost one (to silence at 0), follows a focus switch, and leaving the override restores the app's own volume.
- [ ] Battery: on Bluetooth the menu bar tooltip shows the battery level, and one notification appears when it drops below 15%.
- [ ] Start at login: on a fresh install the menu bar's "Start at login" is unchecked. Checking it adds freewheel to System Settings > General > Login Items and freewheel starts after logging out and in; unchecking it removes the item and freewheel no longer starts. The checkbox matches Login Items after a restart, including when the item was removed there.
- [ ] Import and export: export writes freewheel-mappings.json; importing it asks for confirmation, backs up the current config first, and an imported LaunchApp stays disabled until opted in.
- [ ] Update: installing the previous release and starting it offers the new version; accepting downloads, installs and restarts into it, and Gatekeeper accepts the notarized app.
- [ ] [T3] Update with a tampered .sig: serving a latest.json whose signature is altered (one character changed) makes the update fail with a signature error and is rejected; the installed version is unchanged.

### Open items the hardware spike left unconfirmed

Each is confirmed only by its real-device check above; record the result in the release notes.

- C9a: Bluetooth on Windows 11 (open, auth, Press and Jog over Bluetooth), checked by the [C9a] items.
- R1: Resolve re-acquire on Windows (does Resolve take the device back without a replug), checked by the [R1] item.
- P1: macOS Input Monitoring (is it needed to read the device), checked by the [P1] item.
- SmartScreen on the unsigned NSIS build, on install and on each update (default: accept the prompt for v1 and note it in the release notes).
