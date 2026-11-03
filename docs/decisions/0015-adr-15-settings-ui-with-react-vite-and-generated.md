# freewheel ADR-015: ADR-15 Settings UI with React + Vite and generated TS types; MIT licence

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- React + Vite (create-tauri-app), TS types generated from Rust (ts-rs)
- React + hand-written TS types
- Rust-native UI (Leptos)

## Decision Outcome

Chosen option: "React + Vite (create-tauri-app), TS types generated from Rust (ts-rs)", because The owner chose React (r4q8). Generating TS types from the serde types turns drift into a CI failure (C31 item 5). The licence is MIT per spec §1 and r1q6, with a NOTICE for smunaut [A9].

## Why the other options were rejected

- React + hand-written TS types: Drift is caught only by an extra test, and every Action variant has to be duplicated by hand.
- Rust-native UI (Leptos): The owner chose React.

## Open questions

- ts-rs or specta for type generation? (decide before card "Tauri commands and status events for the settings window"; default: ts-rs, with a CI check that the regenerated files have no diff.)
