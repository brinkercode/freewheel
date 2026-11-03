# freewheel ADR-011: ADR-11 CI on GitHub-hosted windows-2025, macos-15 and macos-15-intel with fakes only

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- Hosted windows-2025 + macos-15 + macos-15-intel
- ubuntu-latest only
- Self-hosted runner on the owner's PC

## Decision Outcome

Chosen option: "Hosted windows-2025 + macos-15 + macos-15-intel", because owner-reply:1 says to test on every supported OS on hosted runners, macOS on both Apple Silicon and Intel. Answer r3q1 says hosted only. CI has no secrets and read-only permissions [A6][T4].

## Why the other options were rejected

- ubuntu-latest only: owner-reply:1 forbids it, and Linux is not a target.
- Self-hosted runner on the owner's PC: The owner chose hosted only, and a persistent runner on a public repo is a compromise risk.

## Open questions

- When the last hosted Intel macOS image retires, should Intel be dropped from the universal build or kept as manual-only testing? (decide before card "CI matrix on hosted Windows and macOS runners"; default: Keep the universal build. Intel is then covered only by the C38 Intel checklist section, and the owner decides at that time.)
