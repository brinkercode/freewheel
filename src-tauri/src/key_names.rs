//! The named keys a mappings file can press beyond the key panel's own
//! (core mappings_file::names: F13-F24, the number pad, Insert, Print
//! Screen, Pause, the lock keys), as enigo keys. Where enigo has no enum
//! variant on an OS, the OS key code goes through `Key::Other` (Windows
//! virtual-key codes, macOS kVK codes, X11/Wayland keysyms). `None`: the key
//! does not exist on this OS (macOS has no F21-F24, Insert, Print Screen,
//! Pause or lock keys).

use enigo::Key;

/// The enigo key for a stored key name, if it is one of the extra keys.
pub fn extra_key(name: &str) -> Option<Key> {
    if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
        return function_key(n);
    }
    code(name).map(Key::Other)
}

fn function_key(n: u32) -> Option<Key> {
    Some(match n {
        13 => Key::F13,
        14 => Key::F14,
        15 => Key::F15,
        16 => Key::F16,
        17 => Key::F17,
        18 => Key::F18,
        19 => Key::F19,
        20 => Key::F20,
        21..=24 => return high_function_key(n),
        _ => return None,
    })
}

#[cfg(target_os = "macos")]
fn high_function_key(_n: u32) -> Option<Key> {
    None
}

#[cfg(windows)]
fn high_function_key(n: u32) -> Option<Key> {
    // VK_F21 = 0x84.
    Some(Key::Other(0x84 + n - 21))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn high_function_key(n: u32) -> Option<Key> {
    // XK_F21 = 0xffd2.
    Some(Key::Other(0xffd2 + n - 21))
}

/// Number pad digits, from 0.
fn digit(name: &str) -> Option<u32> {
    let d = name.strip_prefix("Num")?;
    (d.len() == 1).then(|| d.parse().ok()).flatten()
}

#[cfg(windows)]
fn code(name: &str) -> Option<u32> {
    if let Some(d) = digit(name) {
        return Some(0x60 + d); // VK_NUMPAD0
    }
    Some(match name {
        "NumAdd" => 0x6B,
        "NumSubtract" => 0x6D,
        "NumMultiply" => 0x6A,
        "NumDivide" => 0x6F,
        "NumDecimal" => 0x6E,
        "NumEnter" => 0x0D, // VK_RETURN (Windows has no separate code)
        "NumLock" => 0x90,
        "ScrollLock" => 0x91,
        "Insert" => 0x2D,
        "PrintScreen" => 0x2C,
        "Pause" => 0x13,
        "CapsLock" => 0x14,
        _ => return None,
    })
}

#[cfg(target_os = "macos")]
fn code(name: &str) -> Option<u32> {
    // kVK_ANSI_Keypad0... (Carbon Events.h).
    const DIGITS: [u32; 10] = [0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5B, 0x5C];
    if let Some(d) = digit(name) {
        return DIGITS.get(d as usize).copied();
    }
    Some(match name {
        "NumAdd" => 0x45,
        "NumSubtract" => 0x4E,
        "NumMultiply" => 0x43,
        "NumDivide" => 0x4B,
        "NumDecimal" => 0x41,
        "NumEnter" => 0x4C,
        "CapsLock" => 0x39,
        _ => return None,
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn code(name: &str) -> Option<u32> {
    if let Some(d) = digit(name) {
        return Some(0xffb0 + d); // XK_KP_0
    }
    Some(match name {
        "NumAdd" => 0xffab,
        "NumSubtract" => 0xffad,
        "NumMultiply" => 0xffaa,
        "NumDivide" => 0xffaf,
        "NumDecimal" => 0xffae,
        "NumEnter" => 0xff8d,
        "NumLock" => 0xff7f,
        "ScrollLock" => 0xff14,
        "Insert" => 0xff63,
        "PrintScreen" => 0xff61,
        "Pause" => 0xff13,
        "CapsLock" => 0xffe5,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_key_a_file_can_write_is_pressable_or_known_missing() {
        // The key panel's own names go through inject.rs; the rest through here.
        let missing_on_mac = [
            "F21",
            "F22",
            "F23",
            "F24",
            "Insert",
            "PrintScreen",
            "Pause",
            "NumLock",
            "ScrollLock",
        ];
        for k in freewheel_core::mappings_file::names::named_keys() {
            let stroke = freewheel_core::inject::Keystroke::Key(k.stored.to_string());
            let found = crate::inject::to_enigo(&stroke).is_ok();
            if cfg!(target_os = "macos") && missing_on_mac.contains(&k.stored) {
                assert!(!found, "{}", k.stored);
            } else {
                assert!(found, "{} cannot be pressed", k.stored);
            }
        }
        assert!(extra_key("F25").is_none() && extra_key("Num10").is_none());
    }
}
