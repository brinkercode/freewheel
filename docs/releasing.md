# Releasing freewheel

Releases come from [release-please](https://github.com/googleapis/release-please)
and conventional commits (`fix:` → patch, `feat:` → minor while below 1.0).
freewheel ships files you download and run: no installer, no zip.

## What a release holds

| Asset | What it is |
| --- | --- |
| `freewheel-X.Y.Z-windows-x64.exe` | the portable exe itself: download and run (the simulator `.cmd` is in test builds, not releases) |
| `freewheel-X.Y.Z-macos-universal.dmg` | `freewheel.app` (universal; ad-hoc signed until the `APPLE_*` secrets exist, then signed and notarized) and an `Applications` link to drag it onto. A `.app` is a folder, so it needs a container; the `.dmg` is opened, not installed |
| `<file>.sig` | minisign signature of the file by the updater key, bound to the version (`tauri signer sign --app-version`) |
| `latest.json` | the manifest the app reads: `version`, `notes` (release-please's, user-facing: see Release notes below), `pub_date`, and `platforms.{windows-x86_64,darwin-aarch64,darwin-x86_64}.{url,signature}` |

## How updates work

`src-tauri/src/selfupdate.rs`, driven by the settings window (src/updater.js):

1. **Check** (at start, once; and on Check for updates):
   `GET https://github.com/brinkercode/freewheel/releases/latest/download/latest.json`
   (a plain download, no API rate limit). 204/404 = no release. Newer by
   semver → offered.
2. **Download**: first checks the app's folder is writable (else: "move the
   folder or download it yourself"), then streams the file for this OS/arch to
   `<temp>/freewheel-update/` with progress, and verifies its signature
   against the pubkey baked in at build (`FREEWHEEL_UPDATER_PUBKEY`) and that
   the signature is for the offered version (on macOS the signature covers
   the `.dmg`). A bad download is deleted; nothing next to the app has been
   touched.
3. **Restart to update**: puts the new version next to the running one as
   `<name>.new`. Windows: the verified exe itself is copied there, whatever
   the running exe is named (it keeps the name the user downloaded it under).
   macOS: the verified `.dmg` is attached read-only
   (`hdiutil attach -nobrowse -readonly`), its `freewheel.app` copied out with
   `ditto`, the image detached. Then renames the running one to
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
   `Cargo.lock`, and adds the notes to `CHANGELOG.md`. It then starts `ci.yml`,
   `e2e-windows.yml` and `e2e-linux.yml` on that PR's branch (a PR made with `GITHUB_TOKEN`
   starts no workflow by itself), so the required checks run.
2. Squash-merge the release PR. On that push release-please tags `vX.Y.Z`,
   publishes the GitHub release with the notes, and starts `release.yml` on the
   tag.
3. `release.yml` runs the CI matrix, then:
   - `build` (Windows, macOS; `release` environment): `npm run tauri -- build`
     (`scripts/tauri-release.mjs` bakes `vars.TAURI_UPDATER_PUBKEY` in),
     checks the binary (`check-windows-exe.mjs` zero-install gate on Windows,
     `codesign --verify` on macOS, `check-release-binary.mjs`: the pubkey is in,
     no e2e-only code), makes the release file (`scripts/portable-release.mjs
     package`: the exe copied under its release name; on macOS the `.dmg`
     made with `hdiutil create`, then `hdiutil verify`).
   - `publish` (`release` environment): signs each file with the updater key
     (the only step that sees it), writes `latest.json` with the release's
     app notes (the `notes` job's `scripts/release-notes.mjs filter`),
     verifies every signature locally, uploads all to the release.
   - `latest-json` re-downloads the published `latest.json` and verifies every
     package against `vars.TAURI_UPDATER_PUBKEY`.
   - `update-e2e` downloads the previous release's exe into a folder on
     Windows and updates it to this one with that app's own updater.

While a release builds (about 20 minutes) the release is already the latest
one but has no `latest.json`: update checks find nothing and retry next start.

Every PR also runs `update e2e (windows-2025, local release server)` in
`e2e-windows.yml`: two e2e builds (A = the repo's version, B = next minor)
with a throwaway key, A's release exe (`freewheel-A-windows-x64.exe`) in a
folder as downloaded, with keys and a page set up,
B served from 127.0.0.1 (the `e2e` feature's `FREEWHEEL_E2E_UPDATE_URL`;
shipped builds never have it, `check-release-binary.mjs` enforces that), a
tampered B rejected with A intact, then B downloaded, swapped in and
restarted from the same path with What's new showing B's notes verbatim
(the next release's real notes, `release-notes.mjs preview`), the config
unchanged and no `.old` or temp download left.

## Release notes

- release-please writes `CHANGELOG.md` and the GitHub release from the
  conventional commits; `changelog-sections` in `release-please-config.json`
  shows New (feat), Fixed (fix) and Faster (perf) and hides chore, ci, test,
  build, docs, refactor, style and revert. CHANGELOG.md keeps everything in
  those sections.
- The `notes` job replaces the GitHub release body with the cleaned cut,
  `scripts/release-notes.mjs release`: it drops bullets whose scope is
  developer-only (`scripts/release-notes.json` `hidden_scopes`: scripts, ci,
  e2e, release, deps-dev, test, footprint, workflow), strips commit hashes,
  drops the `**freewheel-…:**` ticket labels (`unlabelled_scopes`) and keeps
  the other scope labels and the PR links.
- The app's What's new (`latest.json`) gets the app cut, `release-notes.mjs
  filter`: the cleaned cut without the `**scope:**` labels and the PR links,
  each line starting with a capital, the version heading without its compare
  link. The page links to the GitHub release for the rest.
- `feat:`, `fix:` and `perf:` PR titles are the release notes, so write them
  for users (`fix: the Speed Editor reconnects after sleep`); the
  `oss-readiness` check enforces it (`pr-title`). Give developer-only fixes
  one of the hidden scopes.
- Hand-written notes: put them in `release-notes/v<version>.md` (plain
  Markdown, one line per paragraph or bullet, written for users). When that
  file is in the tag, the `notes` job makes it the GitHub release body and
  What's new shows it as is, in place of the generated notes
  (`release-notes.mjs release --version` / `filter --version`). `preview`
  uses it too.
- Preview what the next release's What's new will say:
  `node scripts/release-notes.mjs preview` (`--release` for the release
  page's cut, `--raw` for release-please's version). It groups `git log <last v* tag>..HEAD` as release-please does and
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

## Later releases (v1.1.0 and on)

release-please computes the version from the commits since the last tag
(`feat:` a minor, `fix:` a patch). To force one, put a `Release-As: 1.2.0`
footer in a commit merged to main.

1. Merge the release PR `chore(main): release X.Y.Z` once its checks are
   green.
2. `release.yml` on `vX.Y.Z` ends with `update-e2e`, which downloads the
   previous release's exe into a folder and updates it headless, then checks
   the same path runs the new version, its `.old` copy and the temp download
   are gone and a second check is up to date. Then it opens the updated app's
   What's new and requires the new release's notes from its GitHub release,
   formatted. It can also be run by hand: Actions → update-e2e → Run workflow
   (optionally with the tag to start from).
3. By hand: an older exe, started, offers the new version; Download and
   install → Restart to update → the same file is the new version, What's
   new shows its notes, your keys are unchanged.

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
and download the `freewheel.exe` artifact from the run's **Artifacts**
section at the bottom of its summary page: it downloads as the exe itself,
not a zip (`archive: false`). It is the portable app, which runs with
nothing to install; `freewheel-simulate.cmd` is a second artifact (put it
next to the exe and double-click it to start `freewheel.exe --simulate`).
Both are kept for 1 day. Test builds can't update themselves (they have no
updater key).

The build is unsigned, so Windows SmartScreen warns when you start it: click
**More info**, then **Run anyway**.
