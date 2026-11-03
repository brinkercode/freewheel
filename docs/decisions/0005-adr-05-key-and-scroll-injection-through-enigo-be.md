# freewheel ADR-005: ADR-05 Key and scroll injection through enigo behind KeyInjector

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- enigo behind a KeyInjector trait
- Direct windows-rs SendInput + core-graphics CGEvent

## Decision Outcome

Chosen option: "enigo behind a KeyInjector trait", because Owner answer r1q5. enigo is MIT and covers SendInput and CGEvent, including scroll, so it can serve the default scrolling dial once P1 adds KeyInjector::scroll. Its MSRV is below the 1.99.1 pin. Tests use only MockInjector [A5].

## Why the other options were rejected

- Direct windows-rs SendInput + core-graphics CGEvent: Two hand-written platform layers with no stated benefit.

## Open questions

- Is the default dial action a scroll-wheel event or arrow keys? (decide before card "Versioned config model with defaults and persistence"; default: Scroll-wheel events through a new Action::Scroll{amount} and KeyInjector::scroll (P1).)
