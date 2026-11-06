# freewheel

Personal project, built through the Opal software factory. Scope comes from the inception.

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

The release gate, checked by hand on each OS before a release.

### Both

- [ ] Settings window: it opens at 1000×640 with the system's own title bar, resizes down to 820×520 and no smaller, and closing it hides it while freewheel keeps running in the tray (Open settings... brings it back).
- [ ] Settings window toolbar: the tabs Keys, Wheel, DaVinci Resolve, General and Help & backup; the status reads Connected (USB) over a cable and Connected (Bluetooth · N%) over Bluetooth. The On/Off switch and the notice bar's Turn on really switch freewheel (the keys stop and start working, the tray toggle and icon follow) and a short "freewheel is Off." / "freewheel is On." toast shows bottom-centre.
- [ ] Resolve pause: with DaVinci Resolve open the status reads Paused (Resolve) and the notice bar says Paused for DaVinci Resolve (never Off or Turn on); quitting Resolve brings back Connected.

### Windows 11

- [ ] System volume dial override: turning the jog wheel right raises and left lowers the default output device's volume (the Windows volume flyout moves), and it follows a change of default output device.
- [ ] Brightness override: an external DDC/CI monitor and a laptop's built-in panel both step up and down with the jog wheel; with DDC/CI off on one monitor (and on a desktop with no built-in panel) that display shows its own error and the others still step.
- [ ] Start at login: on a fresh install the tray's "Start at login" is unchecked. Checking it adds freewheel to Settings > Apps > Startup and freewheel starts after signing out and in; unchecking it removes the entry and freewheel no longer starts. The checkbox matches the Startup entry after a restart, including when the entry was switched off there.

### macOS 15

- [ ] Start at login: on a fresh install the menu bar's "Start at login" is unchecked. Checking it adds freewheel to System Settings > General > Login Items and freewheel starts after logging out and in; unchecking it removes the item and freewheel no longer starts. The checkbox matches Login Items after a restart, including when the item was removed there.
- [ ] System volume: with the System volume override on, turning the jog wheel right raises and left lowers the default output device's volume (the menu bar volume follows), stopping at 0 and full. Switch the output (built-in speakers, then headphones or a USB/Bluetooth device) and it follows the new default. With an output that has no volume control (e.g. HDMI/DisplayPort audio), the settings window shows an override error instead.
