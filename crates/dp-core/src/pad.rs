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

impl Axis {
    pub const ALL: [Axis; 6] = [
        Axis::LeftX,
        Axis::LeftY,
        Axis::RightX,
        Axis::RightY,
        Axis::LeftTrigger,
        Axis::RightTrigger,
    ];
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

/// Convert an unsigned 8-bit centered axis (HID convention, 0x80 = center,
/// 0 = left/up) to `[-1, 1]`. `invert` flips the sign (use for HID Y axes,
/// where 0 means *up*).
#[inline]
pub fn axis_from_u8(v: u8, invert: bool) -> f32 {
    let f = (f32::from(v) - 127.5) / 127.5;
    if invert { -f } else { f }
}

/// Inverse of [`axis_from_u8`].
#[inline]
pub fn axis_to_u8(v: f32, invert: bool) -> u8 {
    let v = if invert { -v } else { v }.clamp(-1.0, 1.0);
    (v * 127.5 + 127.5).round().clamp(0.0, 255.0) as u8
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
    fn u8_center_and_invert() {
        assert!(axis_from_u8(0, false) <= -0.99);
        assert!(axis_from_u8(0, true) >= 0.99);
        assert_eq!(axis_to_u8(1.0, true), 0);
        assert_eq!(axis_to_u8(-1.0, false), 0);
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
