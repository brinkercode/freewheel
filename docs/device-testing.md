# Live Speed Editor test (Windows, then Linux): USB, then Bluetooth

What to do, what should happen, and what to send back when it does not.
Takes about 20 minutes. You need the Speed Editor, its USB-C cable, a
Windows PC with Bluetooth (and for section 4 a Linux desktop), and a
freewheel build from `main`.

Everything you see is also in **Help & backup → More → Device test**: the
connection, the unlock (auth) handshake, the battery, the keys held and
pressed, the wheel, and the last raw reports both ways. **Copy diagnostics**
there copies all of it as text.

## 0. Before you start

1. Quit DaVinci Resolve (it talks to the Speed Editor too; freewheel steps
   aside while it is open).
2. Start freewheel. Open the settings window from the tray icon.
3. **Help & backup → More**: turn on **Turn on detailed logs for
   troubleshooting**. From now on the log holds every raw report in and out
   (`device report in` / `device report out` lines) and every handshake.
4. **Help & backup → More → Open device test**. Leave it open; it updates by
   itself.

What to send back for any step that fails:
- the step number and what you saw;
- **Copy diagnostics** pasted into your reply (Device test view);
- **Help & backup → Download logs** (the zip in your Downloads folder).

## 1. USB

| # | Do this | This should happen |
|---|---------|--------------------|
| 1.1 | Plug the Speed Editor in with the USB cable (Bluetooth off on the PC, or the editor not paired). | Within 3 s: Device test **Status: Connected**, **Connected by: USB**, **Unlock: Passed**, **Session: 10 min 0 s** (600 s). The toolbar says connected. Last reports shows one **out** line `03 00 00 00 00 00 ff` (set wheel mode 0) — or `03 03 …` if your wheel mode is Shuttle. |
| 1.2 | Press and release **CUT**. | Two **in** lines: `04 0f 00 …` (keys held: CUT) then `04 00 00 …` (keys held: none). **Pressed** shows CUT. Whatever CUT is set to happens once. |
| 1.3 | Hold **IN**, press **OUT**, release both. | **Held now** shows `IN, OUT` while both are down, then None. No key stays stuck afterwards (type in Notepad to check). |
| 1.4 | Press every key once, left to right, top to bottom (43 keys). | Each press adds one name to **Pressed**. **Last reports** never shows `unknown keycode`. |
| 1.5 | In the Wheel tab pick **Jog** (or Scroll). Turn the wheel slowly right, then left. | **in** lines `03 00 …` (wheel mode 0, relative) with small values, + right / − left. **Total** goes up, then down. The wheel action happens in the right direction and stops when you stop. |
| 1.6 | Pick **Shuttle** in the Wheel tab. Turn the wheel a little right and hold it there, then back to where it started. | An **out** line `03 03 00 00 00 00 ff` (set wheel mode 3). Then **in** lines `03 03 …` with the position: it keeps repeating while you are off-centre and stops at the centre. |
| 1.7 | Pick **Jog** again and turn the wheel. | **out** `03 00 …`, then relative values again (no jump, no runaway). |
| 1.8 | Leave it untouched for 1 minute, then press CUT. | **Next check in** counts down from 5 s and restarts; **Handshakes … passed** goes up every ~5 s while idle, **failed** stays 0. CUT still works at once. |
| 1.9 | Unplug the cable. | Within a second: **Status: Not connected**, **disconnects** +1. Nothing is left held. |
| 1.10 | Plug it back in. | Within 3 s: Connected again, a new handshake passed, the wheel mode is set again (**out** line), keys work. |
| 1.11 | Sleep the PC (Start → Power → Sleep), wake it, press CUT. | Within ~5 s of waking: Connected (it may show a disconnect and reconnect first). CUT works. |
| 1.12 | Open DaVinci Resolve, then quit it. | freewheel switches Off while Resolve is open (if *Step aside* is on) and back On after; Device test shows a reconnect and the wheel mode set again. Keys and the wheel work as before Resolve. |

## 2. Bluetooth

Unplug the cable first. The Speed Editor pairs as a Bluetooth LE device.

| # | Do this | This should happen |
|---|---------|--------------------|
| 2.1 | Windows Settings → Bluetooth & devices → Add device → Bluetooth. Put the Speed Editor in pairing mode (as its manual says). Pick it. | Windows says it is connected. Within 3 s in freewheel: **Connected by: Bluetooth**, **Unlock: Passed**. |
| 2.2 | Look at **Connection → Device**. | `0x1edb:0xda0e`, a usage page and interface, **N listed**. Note them (they come with Copy diagnostics). |
| 2.3 | Repeat 1.2 – 1.7 (keys, chord, all keys, Jog, Shuttle, back to Jog). | Same as over USB. Bluetooth reports may arrive a little later. If **Shuttle** does not repeat, check that the **out** line for the mode says nothing about *write failed*. |
| 2.4 | Look at **Battery**. | A level (and *charging* if the cable is in). If it stays **No battery report yet** for 2 minutes, say so: the battery may come only through Windows' Bluetooth battery service over Bluetooth. |
| 2.5 | Leave it alone until the Speed Editor sleeps (its lights go off; a few minutes), then press CUT. | Within ~5–10 s: Device test shows a failed handshake or a disconnect, then **Connected** again with a new handshake passed. CUT works (the first press may only wake it). Tell us how long it took. |
| 2.6 | Turn the Speed Editor's Bluetooth off (or walk out of range), wait 10 s, turn it back on. | Not connected, then Connected again by itself within ~5 s of coming back. |
| 2.7 | Plug the USB cable in while it is connected over Bluetooth. | It may switch to **USB** (freewheel prefers USB when both are there) or stay on Bluetooth until the next reconnect. Keys keep working either way. Tell us which. |

## 3. Send back (always, even when everything worked)

1. Device test → **Copy diagnostics** after step 2.4, pasted into your reply.
   It includes the HID report descriptor, which tells us how Windows sees
   the Speed Editor over Bluetooth.
2. **Help & backup → Download logs** after the whole run.
3. Turn **detailed logs** off again (they turn themselves off after a day).

Privacy: the diagnostics and logs hold the device's report bytes and key
names. They never hold the unlock (auth) bytes, what you type, your
shortcuts, or the apps you open.

## 4. Linux (X11 and Wayland)

Run steps 0 to 3 on a Linux desktop too (GNOME or KDE; one X11 and one
Wayland session if you can), with freewheel's AppImage, plus these first:

| # | Do this | This should happen |
|---|---------|--------------------|
| 4.1 | Plug in the Speed Editor **before** installing the device rule; open **Help & backup → More → Open device test** | *Last open error* says `permission denied: freewheel's device rule is not installed`, with an **Install the device rule** button |
| 4.2 | Click **Install the device rule** (or the first run's / Permissions' button) | One password prompt (pkexec); the line says *Installed freewheel's device rule*; `/etc/udev/rules.d/60-freewheel.rules` exists |
| 4.3 | Unplug the Speed Editor and plug it in again | *Status* Connected, *Connected by* USB, the handshake passes |
| 4.4 | Pair it in the desktop's Bluetooth settings (the first run's **Open Bluetooth settings**), unplug the cable | *Connected by* Bluetooth (BlueZ HID, a hidraw node too) |
| 4.5 | Map a key to Ctrl+C and press it in a text editor, on X11 and on Wayland | It copies in both (X11: XTest; Wayland: freewheel's uinput virtual keyboard) |
| 4.6 | Wheel tab: Change the volume / Change screen brightness, turn the wheel | The desktop's volume / brightness sliders move (PipeWire or PulseAudio; laptop backlight through logind, monitors over DDC/CI when `/dev/i2c-*` is open to you) |
| 4.7 | GNOME without the AppIndicator extension | **General** says *No tray icon* and how to get it; launching freewheel again opens its settings |
| 4.8 | Quit from the tray while a key is held or latched | Nothing stays pressed (the virtual keyboard is destroyed, which releases every key) |

In CI (ubuntu-latest) the unit tests, the rule's syntax (`udevadm verify`),
a real XTest key press into an X11 window under Xvfb, and a real uinput key
press into the kernel are proven; the steps above need a real desktop and
device.

## What the lines mean

- `in 04 …` — key report: up to 6 held keys, 2 bytes each (little-endian keycode).
- `in 03 MM VV VV VV VV ..` — wheel report: `MM` mode (0/2 relative, 1/3
  absolute), `VV` a signed 32-bit little-endian value.
- `in 07 CC LL` — battery: `CC` 1 = charging, `LL` level in %.
- `out 03 MM 00 00 00 00 ff` — freewheel setting the wheel mode.
- Windows pads every input report with zeros to the longest one; the view
  drops trailing zeros beyond the report's own length.
- *Unlock (auth)*: the Speed Editor sends nothing until the PC answers its
  challenge (feature report 6). freewheel answers on connect, again 10 s
  before the session the device grants runs out (600 s), and after 5 s
  without any report (so a Speed Editor that slept or lost power comes back
  within seconds instead of after 10 minutes).

Sources for the protocol: smunaut/blackmagic-misc `bmd.py` (Apache-2.0),
JamesBalazs/speed-editor-client, puzzleemptym/Unbound-editor-device-customizer
(USB and Bluetooth), libusb/hidapi Known Issues (Windows Bluetooth writes).
