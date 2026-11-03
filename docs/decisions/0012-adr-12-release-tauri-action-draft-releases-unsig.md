# freewheel ADR-012: ADR-12 Release: tauri-action draft releases, unsigned per-user NSIS, Developer ID notarized universal dmg

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- Unsigned NSIS + notarized universal dmg via tauri-action drafts
- Signed Windows (OV / Azure Artifact Signing) + notarized dmg
- Per-architecture dmgs

## Decision Outcome

Chosen option: "Unsigned NSIS + notarized universal dmg via tauri-action drafts", because Owner answers r3q3, r4q3 and r4q4. Round 1 cited https://github.com/tauri-apps/tauri-action, which supports releaseDraft and uploadUpdaterJson. C37 overrides its '@stable' and per-architecture defaults with the 1.99.1 pin and --target universal-apple-darwin, and builds only after the C2 matrix passes [A6][A17][T5][T6].

## Why the other options were rejected

- Signed Windows (OV / Azure Artifact Signing) + notarized dmg: The owner chose unsigned Windows for v1.
- Per-architecture dmgs: The owner chose one universal binary.

## Open questions

- Does the universal bundle produce both darwin-aarch64 and darwin-x86_64 keys in latest.json? (decide before card "Release workflow: unsigned NSIS and signed universal dmg"; default: Do a dry-run draft (C37 item 5). If either key is missing, post-process latest.json in the workflow.)
