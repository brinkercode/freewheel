# Releasing freewheel

Releases come from [release-please](https://github.com/googleapis/release-please)
and conventional commits (`fix:` → patch, `feat:` → minor while below 1.0).

## How a release flows

1. Every push to `main` runs `release-please.yml`. It opens (or updates) one
   PR, `chore(main): release X.Y.Z`, that bumps the version in `package.json`,
   `package-lock.json`, `src-tauri/tauri.conf.json`, both `Cargo.toml` files and
   `Cargo.lock`, and adds the notes to `CHANGELOG.md`. It then starts `ci.yml`
   on that PR's branch (a PR made with `GITHUB_TOKEN` starts no workflow by
   itself), so the four required checks run.
2. Squash-merge the release PR. On that push release-please tags `vX.Y.Z`,
   publishes the GitHub release with the notes, and starts `release.yml` on the
   tag.
3. `release.yml` runs the CI matrix, then builds on Windows and macOS with
   tauri-action in the `release` environment and uploads the NSIS installer,
   the dmg, the signed updater bundles and `latest.json` (its `notes` are the
   release's notes) to the published release. `latest-json` checks every
   platform and verifies each download's signature against
   `vars.TAURI_UPDATER_PUBKEY`; `update-e2e` installs the previous release on
   Windows and updates it to this one with the installed app's own updater.
4. Installed apps find it at
   `https://github.com/brinkercode/freewheel/releases/latest/download/latest.json`.
   After the update and restart the settings window shows
   **What's new in vX.Y.Z** once, with the notes and a link to the release.

While a release builds (about 20 minutes) the release is already the latest
one but has no `latest.json`: update checks fail quietly and retry next start.

## Keys and settings

- Updater key: `TAURI_SIGNING_PRIVATE_KEY` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` are secrets of the `release`
  environment; the public key is the repository variable
  `TAURI_UPDATER_PUBKEY`, injected into release builds by
  `scripts/tauri-release.mjs` (no key is committed). Losing the private key
  means installed apps can never update again: keep the backup.
- macOS: until the `APPLE_*` secrets exist, the app is ad-hoc signed and not
  notarized (Gatekeeper asks on first open).
- Repository settings this relies on: Actions may create pull requests;
  `googleapis/release-please-action@*` is an allowed action; the `release`
  environment allows `v*` tags.

## First release (v0.1.0)

`release-please-config.json` has `"release-as": "0.1.0"` for the first
release only.

1. Merge the PR that adds release-please; the next release-please run opens
   `chore(main): release 0.1.0`.
2. Wait for its checks, squash-merge it. Watch `release-please`, then
   `release` on the `v0.1.0` tag. `update-e2e` is skipped (no older release).
3. Check `https://github.com/brinkercode/freewheel/releases/latest/download/latest.json`
   shows `"version": "0.1.0"` and the notes.

## A patch (v0.1.1)

1. In one PR: remove `"release-as": "0.1.0"` from `release-please-config.json`
   and include a small `fix:` change (the PR title is the squash commit, so
   it must start with `fix:`).
2. Merge it; release-please opens `chore(main): release 0.1.1`. Merge that.
3. `release.yml` on `v0.1.1` ends with `update-e2e`, which installs v0.1.0 and
   updates it to v0.1.1. It can also be run by hand: Actions → update-e2e →
   Run workflow (optionally with the tag to start from).
