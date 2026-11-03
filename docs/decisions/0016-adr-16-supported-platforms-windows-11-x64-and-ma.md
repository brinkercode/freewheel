# freewheel ADR-016: ADR-16 Supported platforms: Windows 11 x64 and macOS 15+ universal; the macOS minimum follows the oldest hosted CI image

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- Windows 11 x64 + macOS 15+ universal
- Windows 10 and 11 x64 + macOS 15+
- Windows 11 + macOS 14+

## Decision Outcome

Chosen option: "Windows 11 x64 + macOS 15+ universal", because Spec §1 cites owner answers r2q10, r3q1, r3q4, r4q4 and r4q6. The only hosted Windows runner in the plan is windows-2025, so Windows 10 could not be tested on CI. macOS 15 is needed for process taps (ADR-10) and is the oldest hosted image [A17].

## Why the other options were rejected

- Windows 10 and 11 x64 + macOS 15+: Windows 10 is out of support and has no hosted CI image in the plan. owner-reply:1 named it, but the spec says r2q10 settled it.
- Windows 11 + macOS 14+: macOS 14 has no hosted image in the matrix and lacks the process tap API that ADR-10 needs.
