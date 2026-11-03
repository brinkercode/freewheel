# freewheel ADR-017: ADR-17 Imported LaunchApp bindings are disabled until the user explicitly opts in

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- Import LaunchApp bindings disabled, with explicit opt-in
- Always drop LaunchApp on import
- Import LaunchApp bindings enabled

## Decision Outcome

Chosen option: "Import LaunchApp bindings disabled, with explicit opt-in", because Owner answer r4q10 (C35 ref). A shared mappings file is untrusted, and LaunchApp runs an executable. Keeping imports disabled until opt-in keeps the feature without silently running code, and Launcher never uses a shell [T1][T2][A20].

## Why the other options were rejected

- Always drop LaunchApp on import: Loses legitimate shared setups. The owner chose opt-in.
- Import LaunchApp bindings enabled: A crafted file could run any executable on the next button press.
