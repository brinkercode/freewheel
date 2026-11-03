# freewheel ADR-007: ADR-07 Layer semantics: a stack, with automatic back and held/latched keys persisting

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- Layer stack; the push button pops on the target layer; held keys stay down until their physical release; latched keys persist
- Release all keys on every layer change

## Decision Outcome

Chosen option: "Layer stack; the push button pops on the target layer; held keys stay down until their physical release; latched keys persist", because Owner answer r1q4. Auto-back means the user can never get stuck in a layer [A14]. Releases follow physical buttons, so a layer change never leaves a key stuck [A15].

## Why the other options were rejected

- Release all keys on every layer change: It contradicts the owner's answer and breaks modifier-hold workflows that span layers.

## Open questions

- What is the maximum layer stack depth? (decide before card "Engine: layer stack with automatic back"; default: 8. Pushing deeper is ignored and logged.)
