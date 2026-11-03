# freewheel ADR-004: ADR-04 Concurrency: one device thread owns the transport and the engine; slow OS controls run on a worker

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- Dedicated std thread owning HidTransport + engine, with commands over a channel and blocking reads with a timeout
- tokio tasks with spawn_blocking for HID
- Run the engine on the Tauri main thread

## Decision Outcome

Chosen option: "Dedicated std thread owning HidTransport + engine, with commands over a channel and blocking reads with a timeout", because hidapi reads block. A single owner makes release_all on Off, disconnect or quit deterministic [A1]. Brightness calls are slow, so overrides run on a worker fed by a coalescing queue and never stall key handling [A19, C17 item 4].

## Why the other options were rejected

- tokio tasks with spawn_blocking for HID: Adds an async runtime and splits ownership of the engine state, which makes the ordering of releases harder to prove.
- Run the engine on the Tauri main thread: Blocking reads and slow brightness calls would freeze the tray and the UI.
