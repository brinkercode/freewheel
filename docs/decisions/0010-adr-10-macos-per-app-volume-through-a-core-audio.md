# freewheel ADR-010: ADR-10 macOS per-app volume through a Core Audio process tap, gated by spike C24

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- Process tap with muting plus playback at gain, gated by spike C24
- Install a virtual audio driver (Background Music approach)
- Not offered on macOS

## Decision Outcome

Chosen option: "Process tap with muting plus playback at gain, gated by spike C24", because macOS has no public per-app volume API. Background Music is GPL-2.0 and needs a system driver. Taps exist on macOS 15, the owner's minimum (r2q3, r4q2). Tapped audio is never stored or sent anywhere [A21].

## Why the other options were rejected

- Install a virtual audio driver (Background Music approach): It is GPL-2.0, which cannot go into an MIT app, and it needs a privileged driver install.
- Not offered on macOS: It stays only as the no-go fallback ('not available on macOS yet').

## Open questions

- Does the hardened runtime need an audio entitlement for taps, in addition to NSAudioCaptureUsageDescription? (decide before card "macOS per-app volume: process tap and gain for one PID"; default: C24 records this (P6). C37's entitlements file adds it only if needed.)
