# freewheel ADR-008: ADR-08 Resolve hand-off by sysinfo polling

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- Poll the process list with sysinfo
- OS process-start notifications (WMI Win32_ProcessStartTrace / NSWorkspace)
- No auto hand-off; manual Off only

## Decision Outcome

Chosen option: "Poll the process list with sysinfo", because sysinfo is MIT and works on both OSes, and one ProcessLister trait can be faked in tests. The owner chose auto-off, with auto-on only when auto-off did the switching [A10].

## Why the other options were rejected

- OS process-start notifications (WMI Win32_ProcessStartTrace / NSWorkspace): Two platform implementations, and WMI tracing needs admin rights.
- No auto hand-off; manual Off only: The owner chose auto-off.

## Open questions

- How often should the process list be polled? (decide before card "Resolve watcher with auto-off and auto-on"; default: Every 2 s, refreshing only process names.)
- If Resolve cannot re-acquire the device without a replug, should the flow stay notify-only? (decide before card "Resolve watcher with auto-off and auto-on"; default: Keep the replug notification, worded to match what C39 observes.)
