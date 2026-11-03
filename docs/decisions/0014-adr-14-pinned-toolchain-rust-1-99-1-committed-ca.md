# freewheel ADR-014: ADR-14 Pinned toolchain Rust 1.99.1, committed Cargo.lock and package-lock.json

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- rust-toolchain.toml channel 1.99.1 + rust-version + committed lockfiles
- Floating stable

## Decision Outcome

Chosen option: "rust-toolchain.toml channel 1.99.1 + rust-version + committed lockfiles", because Owner answer r4q5. 1.99.1 is above the MSRVs of enigo and the plugins. CI and release build from the same pin and lockfiles [A7].

## Why the other options were rejected

- Floating stable: Builds would not be reproducible, and the release could differ from what CI tested.

## Open questions

- Who bumps the pin, and when (1.100.0 is due 12 Nov 2026)? (decide before card "CI matrix on hosted Windows and macOS runners"; default: A manual bump PR after each stable release, gated by the full CI matrix.)
