//! Speed Editor input reports (0x04 keys, 0x03 jog, 0x07 battery) decoded
//! into device events. Layouts follow bmd.py from smunaut/blackmagic-misc.
//! Bluetooth is parsed the same as USB (ADR-002 default).

use std::fmt;

macro_rules! keys {
    ($($name:ident = $code:literal),* $(,)?) => {
        /// A Speed Editor key, with its keycode from bmd.py.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u16)]
        pub enum Key { $($name = $code),* }

        impl Key {
            /// Every key, in keycode declaration order.
            pub const ALL: &[Key] = &[$(Key::$name),*];
        }

        impl TryFrom<u16> for Key {
            type Error = ReportError;
            fn try_from(code: u16) -> Result<Self, ReportError> {
                match code {
                    $($code => Ok(Key::$name),)*
                    _ => Err(ReportError::UnknownKey(code)),
                }
            }
        }
    };
}

keys! {
    SmartInsrt = 0x01, Appnd = 0x02, RiplOwr = 0x03, CloseUp = 0x04,
    PlaceOnTop = 0x05, SrcOwr = 0x06,
    In = 0x07, Out = 0x08, TrimIn = 0x09, TrimOut = 0x0a, Roll = 0x0b,
    SlipSrc = 0x0c, SlipDest = 0x0d, TransDur = 0x0e, Cut = 0x0f, Dis = 0x10,
    SmthCut = 0x11,
    Source = 0x1a, Timeline = 0x1b,
    Shtl = 0x1c, Jog = 0x1d, Scrl = 0x1e,
    Esc = 0x31, SyncBin = 0x1f, AudioLevel = 0x2c, FullView = 0x2d,
    Trans = 0x22, Split = 0x2f, Snap = 0x2e, RiplDel = 0x2b,
    Cam1 = 0x33, Cam2 = 0x34, Cam3 = 0x35, Cam4 = 0x36, Cam5 = 0x37,
    Cam6 = 0x38, Cam7 = 0x39, Cam8 = 0x3a, Cam9 = 0x3b,
    LiveOwr = 0x30, VideoOnly = 0x25, AudioOnly = 0x26, StopPlay = 0x3c,
}

/// An event from the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceEvent {
    Press(Key),
    Release(Key),
    /// `mode` is the device's raw jog mode byte.
    Jog {
        mode: u8,
        value: i32,
    },
    Battery {
        charging: bool,
        level: u8,
    },
    Connected,
    Disconnected,
}

/// Why an input report could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportError {
    Empty,
    Short { id: u8, len: usize },
    UnknownReport(u8),
    UnknownKey(u16),
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReportError::Empty => write!(f, "empty input report"),
            ReportError::Short { id, len } => write!(f, "report {id:#04x} too short: {len} bytes"),
            ReportError::UnknownReport(id) => write!(f, "unknown report id {id:#04x}"),
            ReportError::UnknownKey(code) => write!(f, "unknown keycode {code:#06x}"),
        }
    }
}

impl std::error::Error for ReportError {}

/// Decodes input reports, remembering the held keys to diff report 0x04.
#[derive(Debug, Default)]
pub struct ReportParser {
    held: Vec<Key>,
}

impl ReportParser {
    /// The keys held down after the last key report, in report order.
    pub fn held(&self) -> &[Key] {
        &self.held
    }

    /// Decodes one input report (report id in byte 0) into events.
    pub fn parse(&mut self, report: &[u8]) -> Result<Vec<DeviceEvent>, ReportError> {
        let (&id, _) = report.split_first().ok_or(ReportError::Empty)?;
        let need = match id {
            0x03 => 6,
            0x04 => 13,
            0x07 => 3,
            _ => return Err(ReportError::UnknownReport(id)),
        };
        if report.len() < need {
            return Err(ReportError::Short {
                id,
                len: report.len(),
            });
        }
        match id {
            0x03 => Ok(vec![DeviceEvent::Jog {
                mode: report[1],
                value: i32::from_le_bytes([report[2], report[3], report[4], report[5]]),
            }]),
            0x07 => Ok(vec![DeviceEvent::Battery {
                charging: report[1] != 0,
                level: report[2],
            }]),
            _ => {
                let keys = report[1..13]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&c| u16::from_le_bytes(c))
                    .filter(|&c| c != 0)
                    .map(Key::try_from)
                    .collect::<Result<Vec<_>, _>>()?;
                let released = self
                    .held
                    .iter()
                    .filter(|k| !keys.contains(k))
                    .map(|&k| DeviceEvent::Release(k));
                let pressed = keys
                    .iter()
                    .filter(|k| !self.held.contains(k))
                    .map(|&k| DeviceEvent::Press(k));
                let events = released.chain(pressed).collect();
                self.held = keys;
                Ok(events)
            }
        }
    }
}

/// A key LED, with its bit in the 0x02 output report mask (bmd.py).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Led {
    CloseUp = 1 << 0,
    Cut = 1 << 1,
    Dis = 1 << 2,
    SmthCut = 1 << 3,
    Trans = 1 << 4,
    Snap = 1 << 5,
    Cam7 = 1 << 6,
    Cam8 = 1 << 7,
    Cam9 = 1 << 8,
    LiveOwr = 1 << 9,
    Cam4 = 1 << 10,
    Cam5 = 1 << 11,
    Cam6 = 1 << 12,
    VideoOnly = 1 << 13,
    Cam1 = 1 << 14,
    Cam2 = 1 << 15,
    Cam3 = 1 << 16,
    AudioOnly = 1 << 17,
}

impl Led {
    /// The LED of `key`, if the key has one.
    pub fn for_key(key: Key) -> Option<Led> {
        Some(match key {
            Key::CloseUp => Led::CloseUp,
            Key::Cut => Led::Cut,
            Key::Dis => Led::Dis,
            Key::SmthCut => Led::SmthCut,
            Key::Trans => Led::Trans,
            Key::Snap => Led::Snap,
            Key::Cam7 => Led::Cam7,
            Key::Cam8 => Led::Cam8,
            Key::Cam9 => Led::Cam9,
            Key::LiveOwr => Led::LiveOwr,
            Key::Cam4 => Led::Cam4,
            Key::Cam5 => Led::Cam5,
            Key::Cam6 => Led::Cam6,
            Key::VideoOnly => Led::VideoOnly,
            Key::Cam1 => Led::Cam1,
            Key::Cam2 => Led::Cam2,
            Key::Cam3 => Led::Cam3,
            Key::AudioOnly => Led::AudioOnly,
            _ => return None,
        })
    }
}

/// A jog-mode LED, with its bit in the 0x04 output report (bmd.py).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum JogLed {
    Jog = 1 << 0,
    Shtl = 1 << 1,
    Scrl = 1 << 2,
}

/// How the device reports the jog wheel, set by output report 0x03 (bmd.py).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DeviceJogMode {
    Relative = 0,
    Absolute = 1,
    Relative2 = 2,
    AbsoluteDeadzero = 3,
}

/// Output report 0x02: the lit key LEDs as a little-endian u32 mask.
pub fn led_report(leds: &[Led]) -> [u8; 5] {
    let mask = leds.iter().fold(0u32, |m, &l| m | l as u32).to_le_bytes();
    [0x02, mask[0], mask[1], mask[2], mask[3]]
}

/// Output report 0x04: the lit jog-mode LEDs.
pub fn jog_led_report(leds: &[JogLed]) -> [u8; 2] {
    [0x04, leds.iter().fold(0u8, |m, &l| m | l as u8)]
}

/// Output report 0x03: mode byte, a zero le32 and 0xff, as bmd.py packs it.
pub fn set_jog_mode(mode: DeviceJogMode) -> [u8; 7] {
    [0x03, mode as u8, 0, 0, 0, 0, 0xff]
}

#[cfg(test)]
mod tests {
    use super::DeviceEvent::{Press, Release};
    use super::*;

    fn keys_report(codes: &[u16]) -> Vec<u8> {
        let mut r = vec![0x04];
        for i in 0..6 {
            r.extend_from_slice(&codes.get(i).copied().unwrap_or(0).to_le_bytes());
        }
        r
    }

    #[test]
    fn key_enum_covers_every_bmd_keycode() {
        assert_eq!(Key::ALL.len(), 43);
        for &k in Key::ALL {
            assert_eq!(Key::try_from(k as u16), Ok(k));
        }
        assert_eq!(Key::try_from(0x12), Err(ReportError::UnknownKey(0x12)));
    }

    #[test]
    fn overlapping_key_reports_diff_into_press_and_release() {
        let mut p = ReportParser::default();
        assert_eq!(
            p.parse(&keys_report(&[0x07, 0x08])).unwrap(),
            vec![Press(Key::In), Press(Key::Out)]
        );
        assert_eq!(
            p.parse(&keys_report(&[0x08, 0x0f])).unwrap(),
            vec![Release(Key::In), Press(Key::Cut)]
        );
        assert_eq!(p.parse(&keys_report(&[0x08, 0x0f])).unwrap(), vec![]);
        assert_eq!(
            p.parse(&keys_report(&[])).unwrap(),
            vec![Release(Key::Out), Release(Key::Cut)]
        );
    }

    #[test]
    fn six_held_keys_all_press_and_release() {
        let mut p = ReportParser::default();
        let six = [0x33, 0x34, 0x35, 0x36, 0x37, 0x38];
        let presses = p.parse(&keys_report(&six)).unwrap();
        assert_eq!(
            presses,
            six.iter()
                .map(|&c| Press(Key::try_from(c).unwrap()))
                .collect::<Vec<_>>()
        );
        let swap = p
            .parse(&keys_report(&[0x33, 0x34, 0x35, 0x36, 0x37, 0x3c]))
            .unwrap();
        assert_eq!(swap, vec![Release(Key::Cam6), Press(Key::StopPlay)]);
    }

    #[test]
    fn jog_report_gives_mode_and_signed_value() {
        let mut p = ReportParser::default();
        let mut r = vec![0x03, 0x01];
        r.extend_from_slice(&(-4096i32).to_le_bytes());
        r.push(0);
        assert_eq!(
            p.parse(&r).unwrap(),
            vec![DeviceEvent::Jog {
                mode: 1,
                value: -4096
            }]
        );
    }

    #[test]
    fn battery_report_gives_charging_and_level() {
        let mut p = ReportParser::default();
        assert_eq!(
            p.parse(&[0x07, 1, 87]).unwrap(),
            vec![DeviceEvent::Battery {
                charging: true,
                level: 87
            }]
        );
        assert_eq!(
            p.parse(&[0x07, 0, 14]).unwrap(),
            vec![DeviceEvent::Battery {
                charging: false,
                level: 14
            }]
        );
    }

    #[test]
    fn short_or_unknown_reports_are_errors() {
        let mut p = ReportParser::default();
        assert_eq!(p.parse(&[]), Err(ReportError::Empty));
        assert_eq!(
            p.parse(&[0x03, 0, 1, 2]),
            Err(ReportError::Short { id: 0x03, len: 4 })
        );
        assert_eq!(
            p.parse(&[0x04, 0x07]),
            Err(ReportError::Short { id: 0x04, len: 2 })
        );
        assert_eq!(
            p.parse(&[0x07, 1]),
            Err(ReportError::Short { id: 0x07, len: 2 })
        );
        assert_eq!(
            p.parse(&[0x09, 0, 0]),
            Err(ReportError::UnknownReport(0x09))
        );
        assert_eq!(
            p.parse(&keys_report(&[0x99])),
            Err(ReportError::UnknownKey(0x99))
        );
    }

    #[test]
    fn led_report_is_0x02_and_le32_mask() {
        assert_eq!(led_report(&[]), [0x02, 0, 0, 0, 0]);
        assert_eq!(led_report(&[Led::CloseUp]), [0x02, 0x01, 0, 0, 0]);
        assert_eq!(
            led_report(&[Led::Cut, Led::LiveOwr, Led::AudioOnly]),
            [0x02, 0x02, 0x02, 0x02, 0x00]
        );
        let all = [
            Led::CloseUp,
            Led::Cut,
            Led::Dis,
            Led::SmthCut,
            Led::Trans,
            Led::Snap,
            Led::Cam7,
            Led::Cam8,
            Led::Cam9,
            Led::LiveOwr,
            Led::Cam4,
            Led::Cam5,
            Led::Cam6,
            Led::VideoOnly,
            Led::Cam1,
            Led::Cam2,
            Led::Cam3,
            Led::AudioOnly,
        ];
        for (bit, &l) in all.iter().enumerate() {
            assert_eq!(l as u32, 1 << bit);
        }
        assert_eq!(led_report(&all), [0x02, 0xff, 0xff, 0x03, 0x00]);
    }

    #[test]
    fn led_for_key_matches_by_name_and_keys_without_led_have_none() {
        let lit: Vec<Key> = Key::ALL
            .iter()
            .copied()
            .filter(|&k| Led::for_key(k).is_some())
            .collect();
        assert_eq!(lit.len(), 18);
        for k in lit {
            assert_eq!(format!("{:?}", Led::for_key(k).unwrap()), format!("{k:?}"));
        }
        assert_eq!(Led::for_key(Key::In), None);
    }

    #[test]
    fn jog_led_report_covers_jog_shtl_scrl() {
        assert_eq!(jog_led_report(&[]), [0x04, 0]);
        assert_eq!(jog_led_report(&[JogLed::Jog]), [0x04, 0x01]);
        assert_eq!(jog_led_report(&[JogLed::Shtl]), [0x04, 0x02]);
        assert_eq!(jog_led_report(&[JogLed::Scrl]), [0x04, 0x04]);
        assert_eq!(
            jog_led_report(&[JogLed::Jog, JogLed::Shtl, JogLed::Scrl]),
            [0x04, 0x07]
        );
    }

    #[test]
    fn set_jog_mode_packs_mode_zero_le32_and_ff() {
        use DeviceJogMode::*;
        assert_eq!(set_jog_mode(Relative), [0x03, 0, 0, 0, 0, 0, 0xff]);
        assert_eq!(set_jog_mode(Absolute), [0x03, 1, 0, 0, 0, 0, 0xff]);
        assert_eq!(set_jog_mode(Relative2), [0x03, 2, 0, 0, 0, 0, 0xff]);
        assert_eq!(set_jog_mode(AbsoluteDeadzero), [0x03, 3, 0, 0, 0, 0, 0xff]);
    }
}
