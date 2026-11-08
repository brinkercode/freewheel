# Contributing to freewheel

Thanks for helping. Bug reports and feature requests go to the Issues tab
(the forms ask for what is needed); security problems go to the Security tab
instead (see [SECURITY.md](SECURITY.md)). Everyone taking part follows the
[Code of Conduct](CODE_OF_CONDUCT.md).

## Building and testing

You need Node 22 with npm, and the Rust toolchain pinned in
`rust-toolchain.toml` (rustup picks it up). On Linux, Tauri's system packages
(WebKitGTK and friends) are needed to build the app itself.

```sh
npm ci
npm test                                  # vitest: UI and script tests
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run tauri dev                         # run the app
```

`freewheel --simulate` opens a Simulator window in place of a real Speed
Editor, so every screen can be tried without the hardware.

## Issues

One issue is one problem or one idea, small enough for one pull request.

- **Labels:** `bug` or `enhancement`, plus one priority:
  - `p1`: fix now; it goes in the next patch release.
  - `p2`: the release after that.
  - `p3`: when there's time.
- **A bug needs proof:** exact steps, or a failing test, or the file and line
  with an input that triggers it. "It might…" is not a bug yet.
- **Work goes highest priority first, oldest first.** Comment on an issue
  before you start, so two people don't fix the same thing.
- **The pull request says `Fixes #N`** and adds a test that fails without the
  fix. Merging it closes the issue.

## Pull requests

- One change per pull request, with tests for it. CI must be green: lint,
  tests on Windows and macOS, the Windows end-to-end run, the footprint check
  and `oss-readiness`.
- Commit messages and the PR title follow
  [Conventional Commits](https://www.conventionalcommits.org/) (`fix:`,
  `feat:`, `docs:` …); release-please writes the changelog from them.
- `feat:`/`fix:`/`perf:` titles are the release notes: write them for users,
  e.g. `fix: the Speed Editor reconnects after sleep` (at least four words,
  no closing period, no developer jargon such as CI, e2e, lint or
  workflow). Developer-only changes take a hidden scope
  (`scripts/release-notes.json`, e.g. `fix(ci): …`). `oss-readiness` checks
  the title.
- Sign off your commits (`git commit -s`): it certifies the
  [Developer Certificate of Origin](https://developercertificate.org/).
  Use your GitHub no-reply address (`<you>@users.noreply.github.com`) if you
  do not want your e-mail address in the public history.
- By contributing you agree that your contribution is licensed under the
  [MIT License](LICENSE). Third-party code or assets need a compatible licence
  and an entry in [NOTICE](NOTICE).

## AI-assisted contributions

AI-assisted contributions are welcome and held to the same bar as any other.
You, the human submitter, are accountable for the change:

- Disclose substantial AI help in the pull request description: what the tool
  did and what you did.
- You must understand every line you submit and be able to explain it in
  review.
- Your sign-off covers AI-assisted code the same way it covers your own.
- Generated code, text and images must be ones you have the right to submit
  under the project's licence; don't paste in code you can't trace.
- Run the tests yourself before opening the pull request; "the AI said it
  works" is not a test result.

## Keeping the repository public-ready

The `oss-readiness` check (`node scripts/oss-readiness.mjs`) fails on private
hostnames, home-directory paths, internal ticket ids, personal e-mail
addresses and similar in files, commit messages and the PR text. Its rules are
in `scripts/oss-denylist.txt`. Secrets are caught by gitleaks
(`.gitleaks.toml`) over the full history; never commit keys or tokens, even in
tests (generate them at test time).
