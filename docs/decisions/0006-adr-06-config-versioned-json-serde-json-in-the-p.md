# freewheel ADR-006: ADR-06 Config: versioned JSON (serde_json) in the ProjectDirs config dir, written atomically

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- config.json with {version} via serde_json
- TOML
- tauri-plugin-store

## Decision Outcome

Chosen option: "config.json with {version} via serde_json", because The owner chose JSON (r3q8). Writing a temp file and renaming it means a crash cannot leave a half-written file. A newer version is rejected, and a malformed file is renamed to config.json.bad-<timestamp> before the first save, so it is never lost (spec §6, C3) [A8].

## Why the other options were rejected

- TOML: The owner chose JSON.
- tauri-plugin-store: It would tie the config to Tauri, so freewheel-core could not load or test it.
