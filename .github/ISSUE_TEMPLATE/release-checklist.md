---
name: Release checklist
about: The hand check with a real Speed Editor on each OS before a release is published.
title: "Release checklist: vX.Y.Z"
labels: ["release"]
---

# Release checklist

The release gate, checked by hand with a real Speed Editor on each OS before a
release. A draft release is published only when every item below passes on
Windows 11 and on macOS 15 on both Apple Silicon and Intel; one failure keeps
the draft unpublished. Open one issue from this template per release and tick the items in it.

### Both

- [ ] Settings window: it opens at 1000×640 with the system's own title bar, resizes down to 820×520 and no smaller, and closing it hides it while freewheel keeps running in the tray (Open settings... brings it back).
- [ ] Settings window toolbar: the tabs Keys, Wheel, DaVinci Resolve, General and Help & backup; the status reads Connected (USB) over a cable and Connected (Bluetooth · N%) over Bluetooth. The On/Off switch and the notice bar's Turn on really switch freewheel (the keys stop and start working, the tray toggle and icon follow) and a short "freewheel is Off." / "freewheel is On." toast shows bottom-centre.
- [ ] General tab: picking Match my computer, Light or Dark changes the window at once and is kept after a restart; "Start freewheel when I log in" matches the OS login item and switching it adds or removes the item (the tray's Start at login follows after a restart); the version reads "freewheel <version>"; Check for updates shows Checking for updates…, then You're up to date. or the download and Restart to update; Update automatically is on on a fresh install and, switched off, a newer version is offered instead of downloaded.
- [ ] Keys tab: pressing a key on the Speed Editor shows "You pressed X" for 2 s and selects that key in the drawing. In the key panel's choices list pick "Press a shortcut", record Ctrl+Shift+K in the detail view and press Save: that key sends Ctrl+Shift+K. Go back with "‹ All choices", pick "Hold a key while I hold this one", record a shortcut such as Shift+A, and Save: it stays down while the key is held. Do the same with "Turn a key on and off": it stays down until the next press. On either choice, tap a Quick pick (Shift, Ctrl or Alt; ⇧, ⌘ or ⌥ on a Mac): it saves at once, and the Speed Editor key holds or toggles that modifier alone; switching Off releases it.
- [ ] Action editor (Keys tab): select a key, pick Open an app (path, one argument per line, the allow toggle), Go to a page, Set the wheel mode or Turn the wheel into a dial, and Save; the key does it at once and after a restart. An empty app path or a missing page shows an error under the field and saves nothing.
- [ ] Wheel tab: for Jog, Shuttle and Scroll set Turn left / Turn right (Scroll, a shortcut or an app) and Ticks per step or Sensitivity, and the dial steps (%); Save, and the wheel does it at once. A step of 0 shows an error under the field and saves nothing.
- [ ] DaVinci Resolve tab: the program name shows Resolve.exe on Windows and Resolve on macOS as the default; Reset restores it, and freewheel steps aside when a program of the saved name starts.
- [ ] Resolve pause: with DaVinci Resolve open the status reads Paused (Resolve) and the notice bar says Paused for DaVinci Resolve (never Off or Turn on); quitting Resolve brings back Connected.

### Windows 11

- [ ] [C9a] USB: plugged in, the Speed Editor opens and authenticates (status shows connected over USB); a Press fires its binding and a Jog turn scrolls. Confirms the USB path on real hardware.
- [ ] [C9a] Bluetooth: paired and with the USB cable out, the Speed Editor opens and authenticates (status shows connected over Bluetooth); a Press fires its binding and a Jog turn scrolls. Confirms Bluetooth on Windows 11, which the hardware spike left unconfirmed.
- [ ] Re-auth: left connected and idle past the auth timeout (up to 10 minutes) over USB and over Bluetooth, the device re-authenticates on its own and a Press still works without a replug.
- [ ] Reconnect: unplug USB (or switch the device off on Bluetooth); the status shows disconnected, and on plug-in or power-on it reconnects and authenticates within a few seconds without restarting freewheel.
- [ ] On/Off: holding a key set to "Hold a key while I hold this one" (choices list → detail view → Save) and switching Off from the tray releases the key (no stuck key in a text field), closes the device and turns the LEDs off; switching On re-opens and re-authenticates it.
- [ ] [freewheel-m5-2] Resolve hand-off: with freewheel On, starting DaVinci Resolve switches freewheel Off and shows the replug notification; Resolve then drives the timeline with the Speed Editor, with or without a replug (record which). Quitting Resolve switches freewheel back On; after a manual Off it stays Off. Confirms whether Resolve re-acquires the device on Windows without a replug.
- [ ] LEDs: SCRL is lit after connecting (default wheel mode); pressing SHTL / JOG / SCRL moves the light to that button; a key set to "Turn a key on and off" (e.g. TRANS) is lit while on and dark after the second press; a page key (e.g. CAM 1) is lit while its page is active and dark back on the first page; the active override's toggle button is lit; everything goes dark on Off, when freewheel steps aside for Resolve and on Quit, and comes back after unplug/replug.
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
- [ ] SmartScreen on install: running the unzipped, unsigned `freewheel.exe` the first time shows SmartScreen or not (record what is shown); "More info > Run anyway" starts it.
- [ ] Update: unzipping the previous release into a folder and starting it offers the new version; accepting downloads it with progress, Restart to update restarts into it from the same folder, What's new shows, the keys and settings are unchanged and `freewheel.exe.old` is gone. SmartScreen on update: record whether the updated exe prompts again (it should not: the app downloads it, so it has no mark of the web). In a folder you can't change (Program Files) the update says to move freewheel or download it.
- [ ] [T3] Update with a tampered .sig: serving a latest.json whose signature is altered (one character changed) makes the update fail with a signature error and is rejected; the folder's version is unchanged.

### macOS 15 (Apple Silicon and Intel)

Run every item on an Apple Silicon Mac and on an Intel Mac, from the universal zip.

- [ ] [C9a] USB: plugged in, the Speed Editor opens and authenticates (status shows connected over USB); a Press fires its binding and a Jog turn scrolls.
- [ ] [C9a] Bluetooth: paired and with the USB cable out, the Speed Editor opens and authenticates (status shows connected over Bluetooth); a Press fires its binding and a Jog turn scrolls.
- [ ] Re-auth: left connected and idle past the auth timeout (up to 10 minutes) over USB and over Bluetooth, the device re-authenticates on its own and a Press still works without a replug.
- [ ] Reconnect: unplug USB (or switch the device off on Bluetooth); the status shows disconnected, and on plug-in or power-on it reconnects and authenticates within a few seconds without restarting freewheel.
- [ ] On/Off: holding a key set to "Hold a key while I hold this one" (choices list → detail view → Save) and switching Off from the menu bar releases the key (no stuck key in a text field), closes the device and turns the LEDs off; switching On re-opens and re-authenticates it.
- [ ] Resolve hand-off: with freewheel On, starting DaVinci Resolve switches freewheel Off and shows the replug notification; Resolve then drives the timeline with the Speed Editor, with or without a replug (record which). Quitting Resolve switches freewheel back On; after a manual Off it stays Off.
- [ ] [freewheel-m5-5] Permissions: on a fresh install, or with Accessibility and Input Monitoring revoked, the menu bar shows 'Needs permission (click to fix)' and open_permission_pane lands on Privacy & Security > Accessibility and > Input Monitoring; once both are granted, Press events arrive. Confirms whether macOS Input Monitoring is needed to read the device.
- [ ] LEDs: SCRL is lit after connecting (default wheel mode); pressing SHTL / JOG / SCRL moves the light to that button; a key set to "Turn a key on and off" (e.g. TRANS) is lit while on and dark after the second press; a page key (e.g. CAM 1) is lit while its page is active and dark back on the first page; the active override's toggle button is lit; everything goes dark on Off, when freewheel steps aside for Resolve and on Quit, and comes back after unplug/replug.
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
- [ ] Update: unzipping the previous release, moving freewheel.app to Applications and starting it offers the new version; accepting downloads it, Restart to update replaces freewheel.app in place and restarts into it (no Gatekeeper prompt), What's new shows, Login Items and Accessibility/Input Monitoring still list freewheel (record whether the permissions had to be granted again: ad-hoc signed builds may need it), and `freewheel.app.old` is gone.
- [ ] [T3] Update with a tampered .sig: serving a latest.json whose signature is altered (one character changed) makes the update fail with a signature error and is rejected; the app's version is unchanged.

### Open items the hardware spike left unconfirmed

Each is confirmed only by its real-device check above; record the result in the release notes.

- C9a: Bluetooth on Windows 11 (open, auth, Press and Jog over Bluetooth), checked by the [C9a] items.
- freewheel-m5-2: Resolve re-acquire on Windows (does Resolve take the device back without a replug), checked by the [freewheel-m5-2] item.
- freewheel-m5-5: macOS Input Monitoring (is it needed to read the device), checked by the [freewheel-m5-5] item.
- SmartScreen on the unsigned portable exe, on first start and after each update (default: accept the prompt for v1 and note it in the release notes).
