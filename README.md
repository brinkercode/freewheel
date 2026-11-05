# freewheel

Personal project, built through the Opal software factory. Scope comes from the inception.

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

## Manual checklist

The release gate, checked by hand on each OS before a release.

### Windows 11

- [ ] System volume dial override: turning the jog wheel right raises and left lowers the default output device's volume (the Windows volume flyout moves), and it follows a change of default output device.
- [ ] Brightness override: an external DDC/CI monitor and a laptop's built-in panel both step up and down with the jog wheel; with DDC/CI off on one monitor (and on a desktop with no built-in panel) that display shows its own error and the others still step.
- [ ] Start at login: on a fresh install the tray's "Start at login" is unchecked. Checking it adds freewheel to Settings > Apps > Startup and freewheel starts after signing out and in; unchecking it removes the entry and freewheel no longer starts. The checkbox matches the Startup entry after a restart, including when the entry was switched off there.

### macOS 15

- [ ] Start at login: on a fresh install the menu bar's "Start at login" is unchecked. Checking it adds freewheel to System Settings > General > Login Items and freewheel starts after logging out and in; unchecking it removes the item and freewheel no longer starts. The checkbox matches Login Items after a restart, including when the item was removed there.
