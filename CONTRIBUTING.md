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

## Pull requests

- One change per pull request, with tests for it. CI must be green: lint,
  tests on Windows and macOS, the Windows end-to-end run, the footprint check
  and `oss-readiness`.
- Commit messages and the PR title follow
  [Conventional Commits](https://www.conventionalcommits.org/) (`fix:`,
  `feat:`, `docs:` …); release-please writes the changelog from them.
- Sign off your commits (`git commit -s`): it certifies the
  [Developer Certificate of Origin](https://developercertificate.org/).
  Use your GitHub no-reply address (`<you>@users.noreply.github.com`) if you
  do not want your e-mail address in the public history.
- By contributing you agree that your contribution is licensed under the
  [MIT License](LICENSE). Third-party code or assets need a compatible licence
  and an entry in [NOTICE](NOTICE).

## Keeping the repository public-ready

The `oss-readiness` check (`node scripts/oss-readiness.mjs`) fails on private
hostnames, home-directory paths, internal ticket ids, personal e-mail
addresses and similar in files, commit messages and the PR text. Its rules are
in `scripts/oss-denylist.txt`. Secrets are caught by gitleaks
(`.gitleaks.toml`) over the full history; never commit keys or tokens, even in
tests (generate them at test time).
