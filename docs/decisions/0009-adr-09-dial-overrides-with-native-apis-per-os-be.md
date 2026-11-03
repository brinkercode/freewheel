# freewheel ADR-009: ADR-09 Dial overrides with native APIs per OS behind control traits, best effort, with per-display errors

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- Native per OS: Windows IAudioEndpointVolume / ISimpleAudioVolume / Dxva2 + WMI; macOS Core Audio / DDC / private DisplayServices
- Simulate media and brightness keys through enigo
- Ship overrides on Windows only

## Decision Outcome

Chosen option: "Native per OS: Windows IAudioEndpointVolume / ISimpleAudioVolume / Dxva2 + WMI; macOS Core Audio / DDC / private DisplayServices", because Owner answers r1q3 and r2q4 ('both displays, best effort'). The traits keep the engine testable. Each failing display reports its own error [A16]. Brightness is throttled to at most one call per 100 ms [A19].

## Why the other options were rejected

- Simulate media and brightness keys through enigo: No per-app volume, no per-display brightness, and step sizes cannot be controlled.
- Ship overrides on Windows only: The owner wants both OSes.

## Open questions

- Do the macOS private brightness symbols work on macOS 15 for arm64 and x86_64? (decide before card "macOS brightness for external DDC displays"; default: Spike C40 decides. On no-go, settings shows 'built-in display brightness unavailable' and hides the path.)
