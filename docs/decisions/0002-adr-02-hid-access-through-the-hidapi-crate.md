# freewheel ADR-002: ADR-02 HID access through the hidapi crate

Status: accepted (Approved by brinkercode on 2026-11-03T17:41:32.778336+00:00: Owner approved in #personal-brinker: accept all)

## Considered Options

- hidapi crate
- rusb/libusb raw USB
- Hand-written hid.dll / IOHIDManager bindings

## Decision Outcome

Chosen option: "hidapi crate", because Round 1 cited https://github.com/libusb/hidapi: one code path for USB and Bluetooth HID on Windows (hid.dll) and macOS (IOHidManager), with the feature-report support the auth needs.

## Why the other options were rejected

- rusb/libusb raw USB: The hidapi README says the libusb backend does not work with Bluetooth devices, and Bluetooth is a v1 requirement.
- Hand-written hid.dll / IOHIDManager bindings: Two platform implementations with nothing gained over hidapi.

## Open questions

- Does the Speed Editor appear over Bluetooth with the same VID/PID, report IDs and feature-report-6 auth? (decide before card "Parse input reports into device events"; default: Treat Bluetooth as identical to USB. If C39 shows otherwise, add a transport variant.)
- Does macOS need Input Monitoring before hidapi can read this device? (decide before card "macOS permission detection and the 'Needs permission' tray state"; default: Check and show both Accessibility and Input Monitoring.)
