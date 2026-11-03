# freewheel ADR-013: ADR-13 Updates through tauri-plugin-updater with a static latest.json on GitHub Releases

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- tauri-plugin-updater + latest.json on GitHub Releases
- No auto-update (users download manually)
- Custom update server

## Decision Outcome

Chosen option: "tauri-plugin-updater + latest.json on GitHub Releases", because Round 1 cited https://v2.tauri.app/plugin/updater/: the signature cannot be disabled and TLS is enforced in production. Signature checking is independent of OS code signing, so it also protects the unsigned Windows build [A11][T3][T7].

## Why the other options were rejected

- No auto-update (users download manually): The owner chose the updater (r4q9).
- Custom update server: Hosting cost and another surface to secure, for no benefit over static JSON on GitHub.

## Open questions

- Does each update of the unsigned NSIS build trigger a SmartScreen prompt? (decide before card "Manual hardware test checklist per OS"; default: Accept the prompt for v1 and note it in the release notes. C38 records what is observed.)
