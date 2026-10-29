//! Nintendo Switch and Switch 2 controllers.
//!
//! The protocols are the publicly documented ones (dekuNukem's and ndeadly's research,
//! SDL `SDL_hidapi_switch*.c`). Everything here is pure, so it is unit-tested on every OS:
//! [`switch1`] covers the first-generation protocol (Pro Controller, Joy-Con, charging
//! grip, Nintendo Switch Online pads), [`switch2`] the Switch 2 protocol on USB and, with
//! [`switch2ble`], over Bluetooth LE, [`wired`] third-party pads that only send a plain input
//! report, and [`gcadapter`] the GameCube controller adapter.

pub mod gcadapter;
pub mod switch1;
pub mod switch2;
pub mod switch2ble;
pub mod wired;

use dp_core::pad::{Axis, PadButton, PadState};

pub const NINTENDO_VID: u16 = 0x057E;

/// Nintendo product ids.
pub mod pid {
    pub const JOYCON_L: u16 = 0x2006;
    pub const JOYCON_R: u16 = 0x2007;
    /// The pair joycond (Linux) makes of two Joy-Con.
    pub const JOYCOND_PAIR: u16 = 0x2008;
    pub const PRO: u16 = 0x2009;
    pub const CHARGING_GRIP: u16 = 0x200E;
    pub const SNES: u16 = 0x2017;
    pub const N64: u16 = 0x2019;
    pub const GENESIS: u16 = 0x201E;
    pub const SWITCH2_JOYCON_R: u16 = 0x2066;
    pub const SWITCH2_JOYCON_L: u16 = 0x2067;
    /// The id we give a pair of Joy-Con 2 (not a real device).
    pub const SWITCH2_PAIR: u16 = 0x2068;
    pub const SWITCH2_PRO: u16 = 0x2069;
    pub const SWITCH2_GAMECUBE: u16 = 0x2073;
}

/// User calibration blocks in flash start with this when they are set.
pub(crate) const CAL_MAGIC: [u8; 2] = [0xB2, 0xA1];
/// The player lights of player 1 (the first light), what a pad shows when it connects.
pub(crate) const FIRST_PLAYER: u8 = 0x01;

pub(crate) const HORI_VID: u16 = 0x0F0D;
pub(crate) const PDP_VID: u16 = 0x0E6F;
pub(crate) const POWERA_VID: u16 = 0x20D6;
pub(crate) const ZUIKI_VID: u16 = 0x33DD;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Switch2Model {
    JoyConLeft,
    JoyConRight,
    Pro,
    GameCube,
}

/// Which protocol a device speaks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Model {
    /// First-generation protocol; the controller type byte tells the exact device.
    Switch1,
    /// Switch 2 protocol: HID input plus a USB vendor interface for commands.
    Switch2(Switch2Model),
    /// Third-party wired pad with a plain input-only report.
    Wired,
}

pub fn model_for(vid: u16, pid: u16) -> Option<Model> {
    match (vid, pid) {
        (
            NINTENDO_VID,
            pid::JOYCON_L
            | pid::JOYCON_R
            | pid::PRO
            | pid::CHARGING_GRIP
            | pid::SNES
            | pid::N64
            | pid::GENESIS,
        )
        // Third-party pads with the full protocol: HORI Wireless Switch Pad, PDP Afterglow
        // Wireless, PDP REALMz.
        | (HORI_VID, 0x00F6)
        | (PDP_VID, 0x0186 | 0x018C) => Some(Model::Switch1),
        (NINTENDO_VID, pid::SWITCH2_JOYCON_R) => Some(Model::Switch2(Switch2Model::JoyConRight)),
        (NINTENDO_VID, pid::SWITCH2_JOYCON_L) => Some(Model::Switch2(Switch2Model::JoyConLeft)),
        (NINTENDO_VID, pid::SWITCH2_PRO) => Some(Model::Switch2(Switch2Model::Pro)),
        (NINTENDO_VID, pid::SWITCH2_GAMECUBE) => Some(Model::Switch2(Switch2Model::GameCube)),
        (HORI_VID, 0x00C1 | 0x0092 | 0x00AA | 0x00F0)
        | (PDP_VID, 0x0180 | 0x0181 | 0x0184 | 0x0185 | 0x0187 | 0x0188 | 0x018B)
        | (POWERA_VID, 0xA711..=0xA716 | 0xA718)
        | (ZUIKI_VID, 0x0001..=0x0003) => Some(Model::Wired),
        _ => None,
    }
}

/// Whether `vid:pid` is a Switch 2 controller.
pub fn is_switch2(vid: u16, pid: u16) -> bool {
    matches!(model_for(vid, pid), Some(Model::Switch2(_)))
}

/// Devices that enumerate over USB but cannot be talked to there (the HORI Wireless
/// Switch Pad only works over Bluetooth).
pub fn usb_unsupported(vid: u16, pid: u16) -> bool {
    (vid, pid) == (HORI_VID, 0x00F6)
}

/// Apply the three button bytes shared by both generations (right, shared, left).
///
/// Faces map by position, ZL/ZR become fully pressed or released triggers, and the
/// Joy-Con rail buttons become paddles: left SL → upper-left, left SR → lower-left,
/// right SR → upper-right, right SL → lower-right.
pub fn apply_buttons(s: &mut PadState, right: u8, shared: u8, left: u8) {
    s.set(PadButton::X, right & 0x01 != 0); // Y
    s.set(PadButton::Y, right & 0x02 != 0); // X
    s.set(PadButton::A, right & 0x04 != 0); // B
    s.set(PadButton::B, right & 0x08 != 0); // A
    s.set(PadButton::PaddleUR, right & 0x10 != 0); // right Joy-Con SR
    s.set(PadButton::PaddleLR, right & 0x20 != 0); // right Joy-Con SL
    s.set(PadButton::RB, right & 0x40 != 0);
    s.set_axis(Axis::RightTrigger, f32::from(right & 0x80 != 0));

    s.set(PadButton::Back, shared & 0x01 != 0); // Minus
    s.set(PadButton::Start, shared & 0x02 != 0); // Plus
    s.set(PadButton::RS, shared & 0x04 != 0);
    s.set(PadButton::LS, shared & 0x08 != 0);
    s.set(PadButton::Guide, shared & 0x10 != 0); // Home
    s.set(PadButton::Misc, shared & 0x20 != 0); // Capture
    s.set(PadButton::Misc2, shared & 0x40 != 0); // C (Switch 2)

    s.set(PadButton::DpadDown, left & 0x01 != 0);
    s.set(PadButton::DpadUp, left & 0x02 != 0);
    s.set(PadButton::DpadRight, left & 0x04 != 0);
    s.set(PadButton::DpadLeft, left & 0x08 != 0);
    s.set(PadButton::PaddleLL, left & 0x10 != 0); // left Joy-Con SR
    s.set(PadButton::PaddleUL, left & 0x20 != 0); // left Joy-Con SL
    s.set(PadButton::LB, left & 0x40 != 0);
    s.set_axis(Axis::LeftTrigger, f32::from(left & 0x80 != 0));
}

/// Player lights for player `n` (1–4) the way the console lights them: LED 1, LEDs 1–2,
/// 1–3, all four; 0 turns them off.
pub(crate) fn player_mask(n: u8) -> u8 {
    match n {
        0 => 0,
        n => (1u8 << n.min(4)) - 1,
    }
}

/// Player lights: what the app wants and what the pad was sent last.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PlayerLights {
    want: Option<u8>,
    sent: Option<u8>,
}

impl PlayerLights {
    /// A pad that shows player 1, as it does once it is set up.
    pub(crate) fn new() -> Self {
        PlayerLights {
            want: None,
            sent: Some(FIRST_PLAYER),
        }
    }

    /// The lights of `player` from now on; `None` leaves them as they are.
    pub(crate) fn set(&mut self, player: Option<u8>) {
        self.want = player.map(player_mask);
    }

    /// The wanted lights if they differ from the last sent; they count as sent.
    pub(crate) fn take_change(&mut self) -> Option<u8> {
        let mask = self.want.filter(|m| Some(*m) != self.sent)?;
        self.sent = Some(mask);
        Some(mask)
    }

    /// The wanted lights, else player 1's.
    pub(crate) fn wanted_or_first(&self) -> u8 {
        self.want.unwrap_or(FIRST_PLAYER)
    }

    pub(crate) fn mark_sent(&mut self, mask: u8) {
        self.sent = Some(mask);
    }
}

/// GameCube triggers end at about this raw value ...
const GC_TRIGGER_END: f32 = 232.0;
/// ... and rest at most at this one (higher readings at rest are taken as this).
const GC_TRIGGER_ZERO_MAX: u8 = 200;

/// A GameCube trigger's pull (0–1) from its raw value and its raw value at rest.
pub(crate) fn gc_trigger(v: u8, zero: u8) -> f32 {
    let zero = f32::from(zero.min(GC_TRIGGER_ZERO_MAX));
    ((f32::from(v) - zero) / (GC_TRIGGER_END - zero)).clamp(0.0, 1.0)
}

/// Calibration of one stick axis: raw centre and the raw distance to each end.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AxisCal {
    pub center: f32,
    pub above: f32,
    pub below: f32,
}

impl AxisCal {
    fn normalize(&mut self, raw: u16, grow: bool) -> f32 {
        let d = f32::from(raw) - self.center;
        let extent = if d >= 0.0 {
            &mut self.above
        } else {
            &mut self.below
        };
        if grow && d.abs() > *extent {
            *extent = d.abs();
        }
        if *extent <= 0.0 {
            return 0.0;
        }
        (d / *extent).clamp(-1.0, 1.0)
    }
}

/// Stick calibration. Raw values grow up and to the right, like our axes.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct StickCal {
    pub x: AxisCal,
    pub y: AxisCal,
    /// Widen the extents whenever the stick goes further than calibrated.
    pub grow: bool,
}

/// Used when a calibration field is unset (all ones).
const UNSET: u16 = 0xFFF;

impl StickCal {
    /// A first-generation calibration whose fields are all unset.
    pub fn switch1_unset() -> StickCal {
        let unset = (UNSET, UNSET);
        StickCal::switch1(unset, unset, unset)
    }

    /// Uncalibrated 12-bit stick: centre 2048, full range.
    pub const DEFAULT: StickCal = StickCal {
        x: AxisCal {
            center: 2048.0,
            above: 2048.0,
            below: 2048.0,
        },
        y: AxisCal {
            center: 2048.0,
            above: 2048.0,
            below: 2048.0,
        },
        grow: false,
    };

    /// First-generation calibration (centre, distance above, distance below per axis).
    /// Real sticks rarely reach their calibrated ends, so the extents start at 70% and
    /// grow with use. Unset fields fall back to centre 2048 and range 0.7 × centre.
    pub fn switch1(center: (u16, u16), above: (u16, u16), below: (u16, u16)) -> StickCal {
        let axis = |c: u16, a: u16, b: u16| {
            let c = if c == UNSET { 2048.0 } else { f32::from(c) };
            let range = |v: u16| if v == UNSET { c * 0.7 } else { f32::from(v) };
            AxisCal {
                center: c,
                above: range(a) * 0.7,
                below: range(b) * 0.7,
            }
        };
        StickCal {
            x: axis(center.0, above.0, below.0),
            y: axis(center.1, above.1, below.1),
            grow: true,
        }
    }

    /// Fixed calibration (Switch 2). A zero field means "uncalibrated".
    pub fn fixed(center: (u16, u16), above: (u16, u16), below: (u16, u16)) -> StickCal {
        let axis = |c: u16, a: u16, b: u16, default: AxisCal| {
            if c == 0 || a == 0 || b == 0 {
                default
            } else {
                AxisCal {
                    center: f32::from(c),
                    above: f32::from(a),
                    below: f32::from(b),
                }
            }
        };
        StickCal {
            x: axis(center.0, above.0, below.0, Self::DEFAULT.x),
            y: axis(center.1, above.1, below.1, Self::DEFAULT.y),
            grow: false,
        }
    }

    pub fn normalize(&mut self, (x, y): (u16, u16)) -> (f32, f32) {
        let grow = self.grow;
        (self.x.normalize(x, grow), self.y.normalize(y, grow))
    }
}

/// First-generation IMU frame (x toward the triggers, y left, z up; also hid-nintendo's)
/// to ours: x = pitch, y = yaw, z = roll.
pub fn imu_to_pad([x, y, z]: [f32; 3]) -> [f32; 3] {
    [-y, z, -x]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_lights_fill_up_from_the_first() {
        assert_eq!(
            [0, 1, 2, 3, 4, 7].map(player_mask),
            [0x0, 0x1, 0x3, 0x7, 0xF, 0xF]
        );
    }

    #[test]
    fn models() {
        assert_eq!(model_for(0x057E, 0x2009), Some(Model::Switch1));
        assert_eq!(model_for(0x057E, 0x200E), Some(Model::Switch1));
        assert_eq!(
            model_for(0x057E, 0x2069),
            Some(Model::Switch2(Switch2Model::Pro))
        );
        assert_eq!(
            model_for(0x057E, 0x2066),
            Some(Model::Switch2(Switch2Model::JoyConRight))
        );
        assert_eq!(model_for(0x057E, 0x2072), None, "safe mode");
        assert_eq!(model_for(0x057E, 0x2008), None, "virtual pair");
        assert_eq!(model_for(0x20D6, 0xA713), Some(Model::Wired));
        assert_eq!(model_for(0x20D6, 0x0002), None, "charging only");
        assert_eq!(model_for(0x0E6F, 0x018A), None, "charging only");
        assert_eq!(model_for(0x0F0D, 0x00F6), Some(Model::Switch1));
        assert!(usb_unsupported(0x0F0D, 0x00F6));
        assert_eq!(model_for(0x054C, 0x0CE6), None);
    }

    #[test]
    fn buttons_by_position() {
        let mut s = PadState::default();
        // right: B + R + ZR, shared: Plus + Capture + C, left: Up + L.
        apply_buttons(&mut s, 0x04 | 0x40 | 0x80, 0x02 | 0x20 | 0x40, 0x02 | 0x40);
        assert!(s.pressed(PadButton::A), "Nintendo B is the south button");
        assert!(s.pressed(PadButton::RB) && s.pressed(PadButton::LB));
        assert!(s.pressed(PadButton::Start) && s.pressed(PadButton::Misc));
        assert!(s.pressed(PadButton::Misc2) && s.pressed(PadButton::DpadUp));
        assert_eq!(s.axis(Axis::RightTrigger), 1.0);
        assert_eq!(s.axis(Axis::LeftTrigger), 0.0);
        assert!(!s.pressed(PadButton::B) && !s.pressed(PadButton::DpadDown));

        let mut s = PadState::default();
        apply_buttons(&mut s, 0x08 | 0x01 | 0x02, 0, 0);
        assert!(s.pressed(PadButton::B) && s.pressed(PadButton::X) && s.pressed(PadButton::Y));
    }

    #[test]
    fn rail_buttons_are_paddles() {
        let mut s = PadState::default();
        apply_buttons(&mut s, 0x10, 0, 0x20);
        assert!(s.pressed(PadButton::PaddleUR), "right SR");
        assert!(s.pressed(PadButton::PaddleUL), "left SL");
        let mut s = PadState::default();
        apply_buttons(&mut s, 0x20, 0, 0x10);
        assert!(s.pressed(PadButton::PaddleLR), "right SL");
        assert!(s.pressed(PadButton::PaddleLL), "left SR");
    }

    #[test]
    fn switch1_calibration() {
        let mut cal = StickCal::switch1((2000, 2100), (1400, 1500), (1300, 1200));
        assert_eq!(cal.normalize((2000, 2100)), (0.0, 0.0));
        // 70% of the calibrated range already counts as full deflection.
        let (x, y) = cal.normalize((2000 + 980, 2100 - 840));
        assert!((x - 1.0).abs() < 1e-3 && (y + 1.0).abs() < 1e-3, "{x} {y}");
        let (x, _) = cal.normalize((2000 + 490, 2100));
        assert!((x - 0.5).abs() < 1e-3);
        // Pushing further widens the range.
        cal.normalize((2000 + 1400, 2100));
        let (x, _) = cal.normalize((2000 + 700, 2100));
        assert!((x - 0.5).abs() < 1e-3);
    }

    #[test]
    fn unset_calibration_falls_back() {
        let mut cal = StickCal::switch1_unset();
        assert_eq!(cal.x.center, 2048.0);
        assert_eq!(cal.normalize((2048, 2048)), (0.0, 0.0));
        let (x, _) = cal.normalize((4095, 2048));
        assert_eq!(x, 1.0);
    }

    #[test]
    fn fixed_calibration() {
        let mut cal = StickCal::fixed((2000, 2000), (1500, 1500), (1000, 1000));
        let (x, y) = cal.normalize((2750, 1500));
        assert!((x - 0.5).abs() < 1e-3 && (y + 0.5).abs() < 1e-3);
        assert_eq!(cal.normalize((4095, 0)), (1.0, -1.0));
        let mut uncal = StickCal::fixed((0, 0), (0, 0), (0, 0));
        assert_eq!(uncal, StickCal::DEFAULT);
        assert_eq!(uncal.normalize((2048, 2048)), (0.0, 0.0));
    }

    #[test]
    fn imu_frame() {
        // Nose up = rotation about the device's y axis (left) in the negative sense.
        assert_eq!(imu_to_pad([1.0, 2.0, 3.0]), [-2.0, 3.0, -1.0]);
    }
}
