//! Platform-neutral gamepad state.
//!
//! Every backend (XInput, HID, evdev, ...) normalizes its device into a [`PadState`],
//! and every virtual output (ViGEm X360/DS4, uinput, ...) is fed from one.

use serde::{Deserialize, Serialize};

use crate::names::named_enum;

named_enum! {
/// Digital gamepad buttons, named by their Xbox position.
///
/// PlayStation/Nintendo layouts map by *position*: Cross = `A`, Circle = `B`,
/// Square = `X`, Triangle = `Y`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum PadButton {
    A = 0,
    B,
    X,
    Y,
    /// Left shoulder (LB / L1 / L).
    LB,
    /// Right shoulder (RB / R1 / R).
    RB,
    /// View / Back / Share / Create / Minus.
    Back,
    /// Menu / Start / Options / Plus.
    Start,
    /// Left stick click (LS / L3).
    LS,
    /// Right stick click (RS / R3).
    RS,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    /// Xbox / PS / Home button.
    Guide,
    /// Touchpad click (DS4 / DualSense).
    Touchpad,
    /// Share (Xbox Series) / Mute (DualSense) / Capture (Switch).
    Misc,
    /// Back paddles (Elite / Edge / Steam Deck / Switch 2 GL-GR / Joy-Con SL-SR):
    /// upper-left, upper-right, lower-left, lower-right.
    PaddleUL,
    PaddleUR,
    PaddleLL,
    PaddleLR,
    /// Second extra button: C (Switch 2 controllers).
    Misc2,
}
}

impl PadButton {
    #[inline]
    pub const fn bit(self) -> u32 {
        1 << self as u8
    }

    /// The bits of these buttons together.
    pub fn mask(buttons: &[PadButton]) -> u32 {
        buttons.iter().fold(0, |m, b| m | b.bit())
    }
}

/// Analog axes. Sticks are in `[-1, 1]` with **up and right positive**;
/// triggers are in `[0, 1]`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum Axis {
    LeftX = 0,
    LeftY,
    RightX,
    RightY,
    LeftTrigger,
    RightTrigger,
}

named_enum! {
/// Which stick.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stick {
    Left,
    Right,
}
}

impl Stick {
    pub const fn axes(self) -> (Axis, Axis) {
        match self {
            Stick::Left => (Axis::LeftX, Axis::LeftY),
            Stick::Right => (Axis::RightX, Axis::RightY),
        }
    }
}

named_enum! {
/// Which trigger.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Left,
    Right,
}
}

impl Trigger {
    pub const fn axis(self) -> Axis {
        match self {
            Trigger::Left => Axis::LeftTrigger,
            Trigger::Right => Axis::RightTrigger,
        }
    }
}

/// One finger on a touchpad, normalized to `[0, 1]` (origin top-left).
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TouchPoint {
    pub x: f32,
    pub y: f32,
}

/// Motion sensor sample. Gyro in degrees/second, accelerometer in g.
///
/// Axes follow the SDL convention with the pad lying flat:
/// gyro `x` = pitch (nose up positive), `y` = yaw (turn left positive), `z` = roll.
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Motion {
    pub gyro: [f32; 3],
    pub accel: [f32; 3],
}

/// Complete normalized state of one gamepad at one instant.
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PadState {
    /// Bitset over [`PadButton::bit`].
    pub buttons: u32,
    /// Indexed by [`Axis`] discriminant.
    pub axes: [f32; 6],
    pub motion: Option<Motion>,
    /// Up to two touch points (DS4 / DualSense / Steam Deck).
    pub touch: [Option<TouchPoint>; 2],
}

impl PadState {
    #[inline]
    pub fn pressed(&self, b: PadButton) -> bool {
        self.buttons & b.bit() != 0
    }

    #[inline]
    pub fn set(&mut self, b: PadButton, down: bool) {
        if down {
            self.buttons |= b.bit();
        } else {
            self.buttons &= !b.bit();
        }
    }

    #[inline]
    pub fn axis(&self, a: Axis) -> f32 {
        self.axes[a as usize]
    }

    #[inline]
    pub fn set_axis(&mut self, a: Axis, v: f32) {
        self.axes[a as usize] = v;
    }

    pub fn stick(&self, s: Stick) -> (f32, f32) {
        let (x, y) = s.axes();
        (self.axis(x), self.axis(y))
    }

    pub fn set_stick(&mut self, s: Stick, (vx, vy): (f32, f32)) {
        let (x, y) = s.axes();
        self.set_axis(x, vx);
        self.set_axis(y, vy);
    }
}

/// Force-feedback request coming back from a game through a virtual pad.
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rumble {
    /// Low-frequency (left, heavy) motor, `[0, 1]`.
    pub low: f32,
    /// High-frequency (right, light) motor, `[0, 1]`.
    pub high: f32,
}

/// Colour of a controller's light bar (DualShock 4, DualSense).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };
    pub const WHITE: Rgb = Rgb {
        r: 255,
        g: 255,
        b: 255,
    };

    /// A colour written `#rrggbb`.
    pub fn from_hex(s: &str) -> Option<Rgb> {
        let h = s.strip_prefix('#')?;
        if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let v = u32::from_str_radix(h, 16).ok()?;
        Some(Rgb {
            r: (v >> 16) as u8,
            g: (v >> 8) as u8,
            b: v as u8,
        })
    }
}

/// Convert a signed 16-bit raw axis (XInput convention) to `[-1, 1]`.
#[inline]
pub fn axis_from_i16(v: i16) -> f32 {
    if v < 0 {
        f32::from(v) / 32768.0
    } else {
        f32::from(v) / 32767.0
    }
}

/// Convert `[-1, 1]` to a signed 16-bit raw axis, clamping.
#[inline]
pub fn axis_to_i16(v: f32) -> i16 {
    let v = v.clamp(-1.0, 1.0);
    if v < 0.0 {
        (v * 32768.0).round() as i16
    } else {
        (v * 32767.0).round() as i16
    }
}

/// A stick from unsigned 8-bit centred HID axes (0x80 = centre, 0 = left and up), y up.
#[inline]
pub fn stick_from_hid_u8(x: u8, y: u8) -> (f32, f32) {
    (from_hid_u8(x), -from_hid_u8(y))
}

/// Inverse of [`stick_from_hid_u8`].
#[inline]
pub fn stick_to_hid_u8((x, y): (f32, f32)) -> [u8; 2] {
    [to_hid_u8(x), to_hid_u8(-y)]
}

fn from_hid_u8(v: u8) -> f32 {
    (f32::from(v) - 127.5) / 127.5
}

fn to_hid_u8(v: f32) -> u8 {
    (v.clamp(-1.0, 1.0) * 127.5 + 127.5)
        .round()
        .clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i16_roundtrip_extremes() {
        assert_eq!(axis_to_i16(axis_from_i16(i16::MIN)), i16::MIN);
        assert_eq!(axis_to_i16(axis_from_i16(i16::MAX)), i16::MAX);
        assert_eq!(axis_to_i16(0.0), 0);
    }

    #[test]
    fn hid_u8_sticks_have_y_up() {
        let (x, y) = stick_from_hid_u8(0, 0);
        assert!(x <= -0.99 && y >= 0.99, "0 is left and up");
        assert_eq!(stick_to_hid_u8((-1.0, 1.0)), [0, 0]);
        assert_eq!(stick_to_hid_u8((1.0, -1.0)), [255, 255]);
        assert_eq!(stick_to_hid_u8(stick_from_hid_u8(0x12, 0xe0)), [0x12, 0xe0]);
    }

    #[test]
    fn button_bits_are_distinct() {
        let mut seen = 0u32;
        for b in PadButton::ALL {
            assert_eq!(seen & b.bit(), 0);
            seen |= b.bit();
        }
    }
}
