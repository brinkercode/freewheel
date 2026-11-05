//! Speed Editor authentication: the challenge-response of feature report 6.
//!
//! Ported from `bmd_kbd_auth` in smunaut's bmd.py (Apache-2.0, credited in
//! NOTICE):
//! <https://github.com/smunaut/blackmagic-misc/blob/c0e71caa1b544026ef7f422b48a1b66b0288d91f/bmd.py>

const AUTH_EVEN_TBL: [u64; 8] = [
    0x3ae1206f97c10bc8,
    0x2a9ab32bebf244c6,
    0x20a6f8b8df9adf0a,
    0xaf80ece52cfc1719,
    0xec2ee2f7414fd151,
    0xb055adfd73344a15,
    0xa63d2e3059001187,
    0x751bf623f42e0dde,
];

const AUTH_ODD_TBL: [u64; 8] = [
    0x3e22b34f502e7fde,
    0x24656b981875ab1c,
    0xa17f3456df7bf8c3,
    0x6df72e1941aef698,
    0x72226f011e66ab94,
    0x3831a3c606296b42,
    0xfd7ff81881332c89,
    0x61a3f6474ff236c6,
];

const MASK: u64 = 0xa79a63f585d37bf0;

/// bmd.py's `rol8`: rotates by one byte (the low byte moves to the top).
fn rol8(v: u64) -> u64 {
    v.rotate_right(8)
}

/// Computes the response to a device challenge (bmd.py `bmd_kbd_auth`).
pub fn compute_response(challenge: u64) -> u64 {
    let n = (challenge & 7) as usize;
    let mut v = challenge.rotate_right(8 * n as u32);
    let k = if (v & 1) == ((0x78 >> n) & 1) {
        AUTH_EVEN_TBL[n]
    } else {
        v ^= rol8(v);
        AUTH_ODD_TBL[n]
    };
    v ^ (rol8(v) & MASK) ^ k
}

#[cfg(test)]
mod tests {
    use super::compute_response;

    const VECTORS: &str = include_str!("auth_vectors.txt");

    fn hex(s: &str) -> u64 {
        u64::from_str_radix(s.trim_start_matches("0x"), 16).expect("hex u64")
    }

    #[test]
    fn matches_every_recorded_bmd_py_vector() {
        let pairs: Vec<(u64, u64)> = VECTORS
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .map(|l| {
                let mut f = l.split_whitespace();
                (hex(f.next().unwrap()), hex(f.next().unwrap()))
            })
            .collect();
        assert_eq!(pairs.len(), 8);
        for (challenge, response) in pairs {
            assert_eq!(
                compute_response(challenge),
                response,
                "challenge {challenge:#018x}"
            );
        }
    }

    #[test]
    fn vectors_file_records_its_provenance() {
        for needle in [
            "c0e71caa1b544026ef7f422b48a1b66b0288d91f",
            "963d2d3c278b9df2b5f3f1ac369038a5c062d2db1d8b2a167f7d4ffaec89ddac",
            "sed -n 134,177p bmd.py",
            "python3 -I gen_vectors.py",
        ] {
            assert!(VECTORS.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn notice_credits_smunaut() {
        let notice = include_str!("../../../NOTICE");
        assert!(notice.contains("smunaut/blackmagic-misc"));
        assert!(notice.contains("Apache License, Version 2.0"));
    }
}
