//! HD rumble encoding.

use dp_core::pad::Rumble;

use super::{ControllerType, NEUTRAL_RUMBLE};

/// Amplitude index 0..=100 (100 = amplitude 1.0, the safe maximum).
fn amplitude_index(amp: f32) -> u8 {
    let amp = amp.clamp(0.0, 1.0);
    let a = if amp > 0.23 {
        32.0 * (8.7 * amp).log2()
    } else if amp > 0.12 {
        16.0 * (17.0 * amp).log2()
    } else {
        amp / 0.12 * 16.0 * (17.0f32 * 0.12).log2()
    };
    a.round().clamp(0.0, 100.0) as u8
}

/// HD rumble data for one actuator: a high band and a low band, both at `amp`.
fn hd_rumble_at(high_hz: f32, low_hz: f32, amp: f32) -> [u8; 4] {
    let code = |f: f32| (32.0 * (f / 10.0).log2()).round() as i32;
    let hf = ((code(high_hz) - 0x60) * 4).clamp(0x04, 0x1FC) as u16;
    let lf = (code(low_hz) - 0x40).clamp(0x01, 0x7F) as u8;
    let a = amplitude_index(amp);
    [
        (hf & 0xFF) as u8,
        ((hf >> 8) as u8 & 0x01) | (a * 2),
        lf | ((a & 1) << 7),
        0x40 + (a >> 1),
    ]
}

/// HD rumble at 320 Hz / 160 Hz.
pub(super) fn hd_rumble(amp: f32) -> [u8; 4] {
    hd_rumble_at(320.0, 160.0, amp)
}

/// Rumble data for both actuators. A Pro Controller gets the heavy motor on its left
/// actuator and the light one on the right; a Joy-Con has a single actuator.
pub(super) fn rumble_data(ty: ControllerType, r: Rumble) -> [u8; 8] {
    let (left, right) = if ty.joycon().is_some() {
        let v = hd_rumble(r.low.max(r.high));
        (v, v)
    } else {
        (hd_rumble(r.low), hd_rumble(r.high))
    };
    let mut out = [0u8; 8];
    out[..4].copy_from_slice(&left);
    out[4..].copy_from_slice(&right);
    out
}

/// Both motors at rest.
pub(super) const NEUTRAL_RUMBLE8: [u8; 8] = {
    let n = NEUTRAL_RUMBLE;
    [n[0], n[1], n[2], n[3], n[0], n[1], n[2], n[3]]
};
