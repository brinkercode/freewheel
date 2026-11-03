# freewheel ADR-003: ADR-03 Clean-room auth port from smunaut bmd.py (Apache-2.0)

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- Port from smunaut/blackmagic-misc bmd.py with NOTICE attribution
- Port from the Unbound repo
- Port from octimot/blackmagic-speededitor

## Decision Outcome

Chosen option: "Port from smunaut/blackmagic-misc bmd.py with NOTICE attribution", because Round 1 fetched https://raw.githubusercontent.com/smunaut/blackmagic-misc/master/bmd.py. It is SPDX Apache-2.0 and contains bmd_kbd_auth, the handshake order and the report layouts. Apache-2.0 code can go into an MIT app if NOTICE gives attribution [A9].

## Why the other options were rejected

- Port from the Unbound repo: It has no licence (all rights reserved), and the brief forbids copying it.
- Port from octimot/blackmagic-speededitor: It is derived from smunaut's code and adds nothing for a Rust port.

## Open questions

- Is the status u16 really the re-auth timeout in seconds? bmd.py's author only says 'I think ... 600'. (decide before card "Auth handshake and re-auth scheduling"; default: Re-auth at min(timeout, 600) - 10 s, and treat silence after that as a disconnect. C39 checks this on hardware.)
