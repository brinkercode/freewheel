# freewheel ADR-001: ADR-01 Workspace split: OS-free core crate plus Tauri shell, with every OS effect behind a trait

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- Single src-tauri crate
- freewheel-core lib (no Tauri, no OS calls in tested logic) + src-tauri app crate, with traits HidTransport, KeyInjector, Launcher, VolumeControl, AppVolumeControl, BrightnessControl, ProcessLister, Clock
- Core crate + one crate per platform adapter

## Decision Outcome

Chosen option: "freewheel-core lib (no Tauri, no OS calls in tested logic) + src-tauri app crate, with traits HidTransport, KeyInjector, Launcher, VolumeControl, AppVolumeControl, BrightnessControl, ProcessLister, Clock", because The brief requires the protocol, auth and mapping engine to be testable without hardware. CI has no device and must never inject real input. A Tauri-free core can be tested with cargo test on hosted runners using fakes only [A5]. Kept from round 1, and the spec §Interfaces lists the same traits.

## Why the other options were rejected

- Single src-tauri crate: Tests would link Tauri and OS code, and the boundary between logic and OS effects could not be enforced.
- Core crate + one crate per platform adapter: More crates to pin and maintain for about 10 adapter files. cfg-gated modules in src-tauri are enough.
