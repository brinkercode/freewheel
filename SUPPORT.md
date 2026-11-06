# Getting help

To report a problem, open a **Bug report** on the repository's Issues tab.
Security problems go to the Security tab instead, never to a public issue.

## Finding your log files

freewheel writes log files that help find out what went wrong. They never
hold what you type, your shortcuts or your window titles. The newest file is
`freewheel.log`; older ones end in `.1` to `.4`.

- **Windows:** press Windows key + R, paste
  `%LOCALAPPDATA%\freewheel\data\logs` and press Enter.
- **macOS:** in Finder, choose **Go > Go to Folder…**, paste
  `~/Library/Application Support` and press Return. Open the folder with
  freewheel in its name, then its `logs` folder.

## Attaching them

Drag `freewheel.log` (and any `freewheel.log.1` to `.4`) into the **Log files**
box of the bug report. If the upload is refused, put the files in a zip first.
