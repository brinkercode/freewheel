# Updates, backups and privacy

## Updates

freewheel checks GitHub for a newer release when it starts and when you click
**Check for updates** (General tab, or the tray). With **Update automatically**
on (the default) it downloads the new version's file for your system, verifies
its signature against the key built into your copy, and offers **Restart to
update**. That puts the new version in place of the one you run (same file,
same name, same place), restarts, shows what's new, and deletes the old copy
and the download. **What's new in v…** under the version (General tab) shows
the notes of the version you run at any time. Your settings are kept. If any step fails, the version you
have keeps running. Turn **Update automatically** off to be asked before the
download.

## Back up and restore

**Help & backup → Back up…** saves your keys, pages and wheel settings to
`freewheel-mappings.json`. **Restore from a backup…** asks before it replaces
anything and keeps a copy of your current settings first.

A backup can hold keys that open apps. Restored keys that open apps stay off
unless you turn on **Let restored keys open apps**: leave it off for a file
someone sent you.

Write or edit a mappings file by hand: see [SCHEMA.md](../SCHEMA.md).

## Privacy

No telemetry, no accounts, no analytics. freewheel's only network requests
are for updates and their notes, to GitHub, over HTTPS:

1. the release manifest,
   `https://github.com/brinkercode/freewheel/releases/latest/download/latest.json`,
   at start and when you click **Check for updates**;
2. the file for your system that the manifest lists (the exe, the .dmg or
   the AppImage of the same GitHub release), only when an update is
   downloaded;
3. the release notes of the version you run, from its GitHub release,
   `https://api.github.com/repos/brinkercode/freewheel/releases/tags/v<version>`
   (no account, no token), only when you open **What's new** and the notes
   were not kept from the update. They are kept for next time.

Builds without the release's update key (built from source, CI test builds)
make no requests at all. Links you click (What's new, a key set to open a
website) open in your browser.

Releases are made with [release-please](https://github.com/googleapis/release-please):
see [releasing.md](releasing.md).
