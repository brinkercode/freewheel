# Releasing freewheel

Releases come from [release-please](https://github.com/googleapis/release-please)
and conventional commits (`fix:` → patch, `feat:` → minor while below 1.0).
freewheel ships portable packages only: no installer.

## What a release holds

| Asset | What it is |
| --- | --- |
| `freewheel-X.Y.Z-windows-x64.zip` | `freewheel.exe` only (the simulator `.cmd` is in test builds, not releases) |
| `freewheel-X.Y.Z-macos-universal.zip` | `freewheel.app` (universal; ad-hoc signed until the `APPLE_*` secrets exist, then signed and notarized) |
| `<zip>.sig` | minisign signature of the zip by the updater key, bound to the version (`tauri signer sign --app-version`) |
| `latest.json` | the manifest the app reads: `version`, `notes` (release-please's, user-facing: see Release notes below), `pub_date`, and `platforms.{windows-x86_64,darwin-aarch64,darwin-x86_64}.{url,signature}` |

## How updates work

`src-tauri/src/selfupdate.rs`, driven by the settings window (src/updater.js):

1. **Check** (at start, once; and on Check for updates):
   `GET https://github.com/brinkercode/freewheel/releases/latest/download/latest.json`
   (a plain download, no API rate limit). 204/404 = no release. Newer by
   semver → offered.
2. **Download**: first checks the app's folder is writable (else: "move the
   folder or download it yourself"), then streams the zip for this OS/arch to
   `<temp>/freewheel-update/` with progress, and verifies its signature
   against the pubkey baked in at build (`FREEWHEEL_UPDATER_PUBKEY`) and that
   the signature is for the offered version. A bad package is deleted;
   nothing next to the app has been touched.
3. **Restart to update**: unpacks `freewheel.exe` / `freewheel.app` to
   `<name>.new` next to the running one, renames the running one to
   `<name>.old` (Windows allows renaming a running exe), renames `.new` into
   place, starts it with `--after-update <pid>` and exits. Any failure puts
   the old one back and shows the error.
4. **Next start**: waits for the old process to exit, deletes `<name>.old`,
   any stale `<name>.new` and the temp folder, and shows What's new once.

Settings (`%APPDATA%\freewheel`, `~/Library/Application Support/freewheel`)
are never touched; Start at login points at the same path. On macOS an
ad-hoc signed app gets a new code signature each release, so macOS may ask
for Accessibility and Input Monitoring again after an update until the app is
signed with a Developer ID.

`freewheel --update-check-and-install` runs the same check, download, verify
and swap headless (exit 0 updated, 2 up to date, 3 no pubkey, 1 failed);
`update-e2e.yml` uses it.

`freewheel --simulate` runs the same state machine with a simulated source
(the Simulator's **New version** publishes one; the download is a committed
test package verified with a test key) and never installs.

## How a release flows

1. Every push to `main` runs `release-please.yml`. It opens (or updates) one
   PR, `chore(main): release X.Y.Z`, that bumps the version in `package.json`,
   `package-lock.json`, `src-tauri/tauri.conf.json`, both `Cargo.toml` files and
   `Cargo.lock`, and adds the notes to `CHANGELOG.md`. It then starts `ci.yml`
   and `e2e-windows.yml` on that PR's branch (a PR made with `GITHUB_TOKEN`
   starts no workflow by itself), so the required checks run.
2. Squash-merge the release PR. On that push release-please tags `vX.Y.Z`,
   publishes the GitHub release with the notes, and starts `release.yml` on the
   tag.
3. `release.yml` runs the CI matrix, then:
   - `build` (Windows, macOS; `release` environment): `npm run tauri -- build`
     (`scripts/tauri-release.mjs` bakes `vars.TAURI_UPDATER_PUBKEY` in),
     checks the binary (`check-windows-exe.mjs` zero-install gate on Windows,
     `codesign --verify` on macOS, `check-release-binary.mjs`: the pubkey is in,
     no e2e-only code), packs the zip (`scripts/portable-release.mjs package`).
   - `publish` (`release` environment): signs each zip with the updater key
     (the only step that sees it), writes `latest.json` with the release's
     user-facing notes (the `notes` job's `scripts/release-notes.mjs filter`),
     verifies every signature locally, uploads all to the release.
   - `latest-json` re-downloads the published `latest.json` and verifies every
     package against `vars.TAURI_UPDATER_PUBKEY`.
   - `update-e2e` unzips the previous release into a folder on Windows and
     updates it to this one with that app's own updater.

While a release builds (about 20 minutes) the release is already the latest
one but has no `latest.json`: update checks find nothing and retry next start.

Every PR also runs `update e2e (windows-2025, local release server)` in
`e2e-windows.yml`: two e2e builds (A = the repo's version, B = next minor)
with a throwaway key, A unzipped into a folder with keys and a page set up,
B served from 127.0.0.1 (the `e2e` feature's `FREEWHEEL_E2E_UPDATE_URL`;
shipped builds never have it, `check-release-binary.mjs` enforces that), a
tampered B rejected with A intact, then B downloaded, swapped in and
restarted from the same path with What's new showing B's notes verbatim
(the next release's real notes, `release-notes.mjs preview`), the config
unchanged and no `.old` or temp download left.

## Release notes

- release-please writes `CHANGELOG.md` and the GitHub release from the
  conventional commits; `changelog-sections` in `release-please-config.json`
  shows Features, Bug Fixes and Performance Improvements and hides chore, ci,
  test, build, docs, refactor, style and revert. CHANGELOG.md and the GitHub
  release keep everything in those sections.
- The app's What's new gets the user-facing cut: `scripts/release-notes.mjs
  filter` drops bullets whose scope is developer-only
  (`scripts/release-notes.json` `hidden_scopes`: scripts, ci, e2e, release,
  deps-dev, test, footprint, workflow), strips commit hashes, drops the
  `**freewheel-…:**` ticket labels (`unlabelled_scopes`) and keeps PR links.
  Write commit subjects for users when the change is user-visible; give
  developer-only fixes one of the hidden scopes.
- Preview what the next release's What's new will say:
  `node scripts/release-notes.mjs preview` (`--raw` for release-please's
  version). It groups `git log <last v* tag>..HEAD` as release-please does and
  runs the same filter. Test and e2e builds embed it
  (`FREEWHEEL_NEXT_NOTES_FILE`, read by `src-tauri/build.rs`) for the
  Simulator's "New version's notes: Next release (real notes)"; "Sample notes"
  and "Paste notes…" are the other choices. The simulated update then carries
  exactly those notes through check, download, restart and What's new.
- What's new renders a safe markdown subset (`src/notesMarkdown.js`): `##`/`###`
  headings, bullets, bold, inline code and links; no HTML. Links open in the
  browser through the opener and only to `https://github.com/`.

## Keys and settings

- Updater key: `TAURI_SIGNING_PRIVATE_KEY` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` are secrets of the `release`
  environment, used only by the `publish` job's sign step; the public key is
  the repository variable `TAURI_UPDATER_PUBKEY`, baked into release builds
  (no key is committed). Losing the private key means released apps can never
  update again: keep the backup.
- macOS: until the `APPLE_*` secrets exist, the app is ad-hoc signed and not
  notarized (Gatekeeper asks on first open: right-click → Open).
- Repository settings this relies on: Actions may create pull requests;
  `googleapis/release-please-action@*` is an allowed action; the `release`
  environment allows `v*` tags.

## First release (v0.1.0)

`release-please-config.json` has `"release-as": "0.1.0"` for the first
release only.

1. Merge the release PR `chore(main): release 0.1.0` once its checks are green
   (rebase it on main first if release-please has not refreshed it since the
   portable-update change: its version files must match).
2. Watch `release-please`, then `release` on the `v0.1.0` tag. `update-e2e`
   is skipped (no older release).
3. Check the release lists the two zips, their `.sig` files and `latest.json`,
   and that
   `https://github.com/brinkercode/freewheel/releases/latest/download/latest.json`
   shows `"version": "0.1.0"`, the notes, and both zip URLs.
4. By hand: unzip the Windows zip into a folder, start `freewheel.exe`;
   General → Check for updates says You're up to date.

## A patch (v0.1.1)

1. In one PR: remove `"release-as": "0.1.0"` from `release-please-config.json`
   and include a small `fix:` change (the PR title is the squash commit, so
   it must start with `fix:`).
2. Merge it; release-please opens `chore(main): release 0.1.1`. Merge that.
3. `release.yml` on `v0.1.1` ends with `update-e2e`, which unzips v0.1.0 into
   a folder and updates it to v0.1.1 headless, then checks the same path runs
   0.1.1, `freewheel.exe.old` and the temp download are gone and a second
   check is up to date. It can also be run by hand: Actions → update-e2e →
   Run workflow (optionally with the tag to start from).
4. By hand: the v0.1.0 folder from step 4 above, started, offers 0.1.1;
   Download and install → Restart to update → the same `freewheel.exe` is
   0.1.1, What's new in v0.1.1 shows, your keys are unchanged.

## Manual checklist

Before a draft release is published, every item of the
[release checklist](../.github/ISSUE_TEMPLATE/release-checklist.md) must pass
with a real Speed Editor on Windows 11 and macOS 15 (Apple Silicon and
Intel). Open a new issue from that template for each release and tick it
there.

## Test builds

Every push to main (or a manual **Run workflow**) runs the `test-build`
workflow. To get a Windows build, open the latest run of
[test-build on the **Actions** tab](https://github.com/brinkercode/freewheel/actions/workflows/test-build.yml)
and download the `freewheel-windows-…` artifact from the run's
**Artifacts** section at the bottom of its summary page. It holds the
portable `freewheel.exe` (and `freewheel-simulate.cmd`), which runs with
nothing to install, and is kept for 1 day. Test builds can't update
themselves (they have no updater key).

The build is unsigned, so Windows SmartScreen warns when you start it: click
**More info**, then **Run anyway**.
