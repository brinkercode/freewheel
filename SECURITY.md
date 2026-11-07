# Security policy

## Reporting a vulnerability

Please do not open a public issue, discussion or pull request for a security
problem. Report it privately instead, through
[private vulnerability reporting](https://github.com/brinkercode/freewheel/security/advisories/new)
(the **Security** tab, **Report a vulnerability**).

Include what you found, the freewheel version and OS, and the steps to
reproduce it. You will get an answer within 7 days; a fix ships in a new
release, and the advisory is published once that release is out.

## Supported versions

Only the latest release gets security fixes. freewheel updates itself
(Check for updates on the General tab), so stay on the newest version.

## What counts

freewheel reads a USB/Bluetooth HID device and types keys, launches apps and
changes volume or brightness for you, so these are in scope: anything that lets
another program or an imported mappings file run code or send keys you did not
set up, the update check or download accepting a file that the release key did
not sign, and log files holding what you type.
