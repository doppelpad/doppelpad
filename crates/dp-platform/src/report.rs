//! Reading and writing controller reports: little-endian numbers, pairs of 12-bit values,
//! the hat switch and 8-bit sticks.

use dp_core::pad::{PadButton, PadState, Stick, stick_from_hid_u8};

/// The little-endian `i16` at `at`.
pub(crate) fn le_i16(b: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([b[at], b[at + 1]])
}

/// The little-endian `u16` at `at`.
pub(crate) fn le_u16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

/// The little-endian `u32` at `at`.
pub(crate) fn le_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// The little-endian `f32` at `at`.
pub(crate) fn le_f32(b: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Unpack two 12-bit values from three bytes (`x = b0 | b1.lo << 8`, `y = b1.hi | b2 << 4`).
pub(crate) fn unpack12(b: &[u8]) -> (u16, u16) {
    let x = u16::from(b[0]) | ((u16::from(b[1]) & 0x0F) << 8);
    let y = (u16::from(b[1]) >> 4) | (u16::from(b[2]) << 4);
    (x, y)
}

/// Pack two 12-bit values into three bytes, the inverse of [`unpack12`].
pub(crate) fn pack12(x: u16, y: u16) -> [u8; 3] {
    [
        x as u8,
        ((x >> 8) as u8 & 0x0F) | ((y as u8 & 0x0F) << 4),
        (y >> 4) as u8,
    ]
}

/// The d-pad from a hat switch (low nibble): 0 up, then clockwise in eighths; 8 and above
/// released.
pub(crate) fn apply_hat(state: &mut PadState, v: u8) {
    let (up, right, down, left) = match v & 0x0F {
        0 => (true, false, false, false),
        1 => (true, true, false, false),
        2 => (false, true, false, false),
        3 => (false, true, true, false),
        4 => (false, false, true, false),
        5 => (false, false, true, true),
        6 => (false, false, false, true),
        7 => (true, false, false, true),
        _ => (false, false, false, false),
    };
    state.set(PadButton::DpadUp, up);
    state.set(PadButton::DpadRight, right);
    state.set(PadButton::DpadDown, down);
    state.set(PadButton::DpadLeft, left);
}

/// The hat switch value of the d-pad (8 when released).
pub(crate) fn hat_from_dpad(s: &PadState) -> u8 {
    match (
        s.pressed(PadButton::DpadUp),
        s.pressed(PadButton::DpadRight),
        s.pressed(PadButton::DpadDown),
        s.pressed(PadButton::DpadLeft),
    ) {
        (true, false, _, false) => 0,
        (true, true, _, _) => 1,
        (false, true, false, _) => 2,
        (_, true, true, _) => 3,
        (false, false, true, false) => 4,
        (_, false, true, true) => 5,
        (false, false, false, true) => 6,
        (true, false, _, true) => 7,
        _ => 8,
    }
}

/// Both sticks from four unsigned 8-bit HID axes: left x, left y, right x, right y.
pub(crate) fn apply_u8_sticks(state: &mut PadState, r: &[u8]) {
    state.set_stick(Stick::Left, stick_from_hid_u8(r[0], r[1]));
    state.set_stick(Stick::Right, stick_from_hid_u8(r[2], r[3]));
}

/// A motor speed (0–1) as 0–255, truncated.
pub(crate) fn motor_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_bit_pairs() {
        assert_eq!(unpack12(&[0x00, 0x08, 0x80]), (0x800, 0x800));
        assert_eq!(unpack12(&[0xFF, 0x0F, 0x00]), (0xFFF, 0x000));
        assert_eq!(unpack12(&[0x34, 0x12, 0xAB]), (0x234, 0xAB1));
        assert_eq!(pack12(0x234, 0xAB1), [0x34, 0x12, 0xAB]);
        assert_eq!(unpack12(&pack12(1919, 942)), (1919, 942));
    }

    #[test]
    fn hat_round_trip() {
        for v in 0..=8 {
            let mut s = PadState::default();
            apply_hat(&mut s, v);
            assert_eq!(hat_from_dpad(&s), v);
        }
        let mut s = PadState::default();
        apply_hat(&mut s, 0x13);
        assert!(s.pressed(PadButton::DpadRight) && s.pressed(PadButton::DpadDown));
    }

    #[test]
    fn little_endian_readers() {
        let b = [0xFF, 0x34, 0x12, 0x00, 0x00, 0x80, 0x3F];
        assert_eq!(le_u16(&b, 1), 0x1234);
        assert_eq!(le_i16(&b, 0), 0x34FF);
        assert_eq!(le_u32(&b, 1), 0x0000_1234);
        assert_eq!(le_f32(&b, 3), 1.0);
        assert_eq!(motor_u8(0.999), 254, "truncates");
    }
}
