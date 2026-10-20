//! Report parsing for Sony DualShock 4 and DualSense controllers.
//!
//! The report layouts are the publicly documented ones (Linux `hid-playstation`,
//! SDL `SDL_hidapi_ps4/ps5`). Pure functions, so they are unit-tested on every OS.

use std::time::Instant;

use dp_core::pad::{Axis, Motion, PadButton, PadState, Rumble, TouchPoint, axis_from_u8};

use crate::PadFamily;
use crate::padio::{Identity, InitError, PadDriver, ReportIo};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SonyModel {
    DualShock4,
    DualSense,
    DualSenseEdge,
}

pub const SONY_VID: u16 = 0x054C;

pub fn model_for(vid: u16, pid: u16) -> Option<SonyModel> {
    if vid != SONY_VID {
        return None;
    }
    match pid {
        0x05C4 | 0x09CC | 0x0BA0 => Some(SonyModel::DualShock4),
        0x0CE6 => Some(SonyModel::DualSense),
        0x0DF2 => Some(SonyModel::DualSenseEdge),
        _ => None,
    }
}

impl SonyModel {
    pub fn name(self) -> &'static str {
        match self {
            SonyModel::DualShock4 => "DualShock 4",
            SonyModel::DualSense => "DualSense",
            SonyModel::DualSenseEdge => "DualSense Edge",
        }
    }

    /// Feature report that switches a Bluetooth-connected pad into full reports
    /// (motion + touchpad). Reading it is harmless over USB.
    pub fn enable_full_reports_feature(self) -> (u8, usize) {
        match self {
            SonyModel::DualShock4 => (0x02, 37),
            SonyModel::DualSense | SonyModel::DualSenseEdge => (0x05, 41),
        }
    }

    fn touch_extent(self) -> (f32, f32) {
        match self {
            SonyModel::DualShock4 => (1920.0, 943.0),
            _ => (1920.0, 1080.0),
        }
    }
}

const GYRO_DPS_PER_LSB: f32 = 1.0 / 16.0;
const ACCEL_G_PER_LSB: f32 = 1.0 / 8192.0;

fn hat(state: &mut PadState, v: u8) {
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

fn face_buttons(state: &mut PadState, b0: u8, b1: u8) {
    hat(state, b0);
    state.set(PadButton::X, b0 & 0x10 != 0); // Square
    state.set(PadButton::A, b0 & 0x20 != 0); // Cross
    state.set(PadButton::B, b0 & 0x40 != 0); // Circle
    state.set(PadButton::Y, b0 & 0x80 != 0); // Triangle
    state.set(PadButton::LB, b1 & 0x01 != 0);
    state.set(PadButton::RB, b1 & 0x02 != 0);
    state.set(PadButton::Back, b1 & 0x10 != 0); // Share / Create
    state.set(PadButton::Start, b1 & 0x20 != 0); // Options
    state.set(PadButton::LS, b1 & 0x40 != 0);
    state.set(PadButton::RS, b1 & 0x80 != 0);
}

fn sticks(state: &mut PadState, r: &[u8]) {
    state.set_axis(Axis::LeftX, axis_from_u8(r[0], false));
    state.set_axis(Axis::LeftY, axis_from_u8(r[1], true));
    state.set_axis(Axis::RightX, axis_from_u8(r[2], false));
    state.set_axis(Axis::RightY, axis_from_u8(r[3], true));
}

fn i16le(r: &[u8], at: usize) -> f32 {
    i16::from_le_bytes([r[at], r[at + 1]]) as f32
}

fn motion(r: &[u8], gyro_at: usize, accel_at: usize) -> Motion {
    Motion {
        gyro: [
            i16le(r, gyro_at) * GYRO_DPS_PER_LSB,
            i16le(r, gyro_at + 2) * GYRO_DPS_PER_LSB,
            i16le(r, gyro_at + 4) * GYRO_DPS_PER_LSB,
        ],
        accel: [
            i16le(r, accel_at) * ACCEL_G_PER_LSB,
            i16le(r, accel_at + 2) * ACCEL_G_PER_LSB,
            i16le(r, accel_at + 4) * ACCEL_G_PER_LSB,
        ],
    }
}

/// 4-byte touch record: `[inactive:1 id:7] [x lo] [y lo:4 | x hi:4] [y hi]`.
fn touch(r: &[u8], at: usize, (w, h): (f32, f32)) -> Option<TouchPoint> {
    if r[at] & 0x80 != 0 {
        return None;
    }
    let x = r[at + 1] as u16 | ((r[at + 2] as u16 & 0x0F) << 8);
    let y = (r[at + 2] as u16 >> 4) | ((r[at + 3] as u16) << 4);
    Some(TouchPoint {
        x: (x as f32 / w).clamp(0.0, 1.0),
        y: (y as f32 / h).clamp(0.0, 1.0),
    })
}

/// Parse one input report (including its report-ID byte) into a pad state.
pub fn parse_report(model: SonyModel, r: &[u8]) -> Option<PadState> {
    let mut s = PadState::default();
    match (model, r.first()?) {
        // Reduced Bluetooth report (both models) before full mode is enabled:
        // id, lx, ly, rx, ry, buttons[3], l2, r2.
        (_, 0x01) if r.len() < 32 => {
            if r.len() < 10 {
                return None;
            }
            sticks(&mut s, &r[1..5]);
            face_buttons(&mut s, r[5], r[6]);
            s.set(PadButton::Guide, r[7] & 0x01 != 0);
            s.set(PadButton::Touchpad, r[7] & 0x02 != 0);
            s.set_axis(Axis::LeftTrigger, r[8] as f32 / 255.0);
            s.set_axis(Axis::RightTrigger, r[9] as f32 / 255.0);
        }
        (SonyModel::DualShock4, &id @ (0x01 | 0x11)) => {
            let c = if id == 0x11 { 3 } else { 1 };
            if r.len() < c + 42 {
                return None;
            }
            sticks(&mut s, &r[c..c + 4]);
            face_buttons(&mut s, r[c + 4], r[c + 5]);
            s.set(PadButton::Guide, r[c + 6] & 0x01 != 0);
            s.set(PadButton::Touchpad, r[c + 6] & 0x02 != 0);
            s.set_axis(Axis::LeftTrigger, r[c + 7] as f32 / 255.0);
            s.set_axis(Axis::RightTrigger, r[c + 8] as f32 / 255.0);
            s.motion = Some(motion(r, c + 12, c + 18));
            let ext = model.touch_extent();
            s.touch = [touch(r, c + 34, ext), touch(r, c + 38, ext)];
        }
        (_, &id @ (0x01 | 0x31)) => {
            let c = if id == 0x31 { 2 } else { 1 };
            if r.len() < c + 40 {
                return None;
            }
            sticks(&mut s, &r[c..c + 4]);
            s.set_axis(Axis::LeftTrigger, r[c + 4] as f32 / 255.0);
            s.set_axis(Axis::RightTrigger, r[c + 5] as f32 / 255.0);
            face_buttons(&mut s, r[c + 7], r[c + 8]);
            let b2 = r[c + 9];
            s.set(PadButton::Guide, b2 & 0x01 != 0);
            s.set(PadButton::Touchpad, b2 & 0x02 != 0);
            s.set(PadButton::Misc, b2 & 0x04 != 0); // mic mute
            if model == SonyModel::DualSenseEdge {
                s.set(PadButton::PaddleUL, b2 & 0x10 != 0); // left Fn
                s.set(PadButton::PaddleUR, b2 & 0x20 != 0); // right Fn
                s.set(PadButton::PaddleLL, b2 & 0x40 != 0); // left back paddle
                s.set(PadButton::PaddleLR, b2 & 0x80 != 0); // right back paddle
            }
            s.motion = Some(motion(r, c + 15, c + 21));
            let ext = model.touch_extent();
            s.touch = [touch(r, c + 32, ext), touch(r, c + 36, ext)];
        }
        _ => return None,
    }
    Some(s)
}

/// Build a USB output report that sets rumble. Bluetooth output needs a CRC'd
/// report and is not implemented yet (returns `None`).
pub fn rumble_report(model: SonyModel, bluetooth: bool, rumble: Rumble) -> Option<Vec<u8>> {
    if bluetooth {
        return None;
    }
    let low = (rumble.low.clamp(0.0, 1.0) * 255.0) as u8;
    let high = (rumble.high.clamp(0.0, 1.0) * 255.0) as u8;
    Some(match model {
        SonyModel::DualShock4 => {
            let mut r = vec![0u8; 32];
            r[0] = 0x05;
            r[1] = 0x01; // rumble valid
            r[4] = high;
            r[5] = low;
            r
        }
        SonyModel::DualSense | SonyModel::DualSenseEdge => {
            let mut r = vec![0u8; 48];
            r[0] = 0x02;
            r[1] = 0x03; // compatible vibration + haptics select
            r[3] = high;
            r[4] = low;
            r
        }
    })
}

/// Driver for the raw-HID backends: asks for full reports and forwards rumble.
pub struct SonyDriver {
    model: SonyModel,
    bluetooth: bool,
    feature_len: usize,
    last_rumble: Rumble,
}

impl SonyDriver {
    /// `feature_len` is the device's feature report length.
    pub fn new(model: SonyModel, bluetooth: bool, feature_len: usize) -> Self {
        SonyDriver {
            model,
            bluetooth,
            feature_len,
            last_rumble: Rumble::default(),
        }
    }
}

impl PadDriver for SonyDriver {
    fn init(&mut self, io: &mut dyn ReportIo) -> Result<Identity, InitError> {
        let (id, len) = self.model.enable_full_reports_feature();
        let mut feature = vec![0u8; len.max(self.feature_len)];
        feature[0] = id;
        if let Err(e) = io.get_feature(&mut feature) {
            tracing::debug!("enabling full reports failed: {e}");
        }
        Ok(Identity {
            name: self.model.name().to_owned(),
            family: match self.model {
                SonyModel::DualShock4 => PadFamily::DualShock4,
                _ => PadFamily::DualSense,
            },
            joycon: None,
            has_motion: true,
            has_touchpad: true,
            has_rumble: !self.bluetooth,
            notice: None,
        })
    }

    fn parse(&mut self, report: &[u8], _now: Instant) -> Option<PadState> {
        parse_report(self.model, report)
    }

    fn service(&mut self, io: &mut dyn ReportIo, rumble: Rumble) -> std::io::Result<()> {
        if rumble == self.last_rumble {
            return Ok(());
        }
        self.last_rumble = rumble;
        match rumble_report(self.model, self.bluetooth, rumble) {
            Some(report) => io.write(&report),
            None => Ok(()),
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_forwards_rumble_changes_only() {
        let mut io = crate::padio::testing::ScriptIo::silent();
        let mut d = SonyDriver::new(SonyModel::DualSense, false, 64);
        let id = d.init(&mut io).unwrap();
        assert_eq!(id.family, PadFamily::DualSense);
        assert_eq!(io.features[0][0], 0x05);
        let r = Rumble {
            low: 1.0,
            high: 0.0,
        };
        d.service(&mut io, r).unwrap();
        d.service(&mut io, r).unwrap();
        assert_eq!(io.writes().len(), 1);
        assert_eq!(io.writes()[0][0], 0x02);
    }

    fn ds4_usb() -> Vec<u8> {
        let mut r = vec![0u8; 64];
        r[0] = 0x01;
        r[1..5].copy_from_slice(&[0x80, 0x80, 0x80, 0x80]);
        r[5] = 0x08; // hat released
        r[35] = 0x80; // touch 0 inactive
        r[39] = 0x80; // touch 1 inactive
        r
    }

    #[test]
    fn ds4_idle_is_neutral() {
        let s = parse_report(SonyModel::DualShock4, &ds4_usb()).unwrap();
        assert_eq!(s.buttons, 0);
        assert!(s.axis(Axis::LeftX).abs() < 0.01);
        assert_eq!(s.touch, [None, None]);
        assert!(s.motion.is_some());
    }

    #[test]
    fn ds4_buttons_hat_triggers_touch() {
        let mut r = ds4_usb();
        r[2] = 0x00; // left stick fully up
        r[5] = 0x20 | 0x02; // cross + hat east
        r[6] = 0x01 | 0x20; // L1 + options
        r[7] = 0x01; // PS
        r[8] = 0xFF; // L2
        r[35] = 0x05; // touch 0 active, id 5
        r[36] = 0x80; // x lo
        r[37] = 0x27; // y lo 2, x hi 7 => x = 0x780 = 1920
        r[38] = 0x00;
        let s = parse_report(SonyModel::DualShock4, &r).unwrap();
        assert!(s.pressed(PadButton::A));
        assert!(s.pressed(PadButton::DpadRight) && !s.pressed(PadButton::DpadUp));
        assert!(
            s.pressed(PadButton::LB) && s.pressed(PadButton::Start) && s.pressed(PadButton::Guide)
        );
        assert!(
            s.axis(Axis::LeftY) > 0.99,
            "HID 0 means up, which is +1 for us"
        );
        assert_eq!(s.axis(Axis::LeftTrigger), 1.0);
        let t = s.touch[0].unwrap();
        assert!((t.x - 1.0).abs() < 1e-3);
    }

    #[test]
    fn ds4_bluetooth_offsets() {
        let usb = ds4_usb();
        let mut bt = vec![0u8; 78];
        bt[0] = 0x11;
        bt[3..3 + 63].copy_from_slice(&usb[1..64]);
        bt[3 + 4] = 0x80; // triangle
        let s = parse_report(SonyModel::DualShock4, &bt).unwrap();
        assert!(s.pressed(PadButton::Y));
    }

    #[test]
    fn dualsense_usb() {
        let mut r = vec![0u8; 64];
        r[0] = 0x01;
        r[1..5].copy_from_slice(&[0xFF, 0x80, 0x80, 0x80]);
        r[5] = 0x00;
        r[6] = 0xFF; // R2
        r[8] = 0x08 | 0x40; // hat released + circle
        r[9] = 0x02; // R1
        r[10] = 0x04; // mute
        r[33] = 0x80;
        r[37] = 0x80;
        let s = parse_report(SonyModel::DualSense, &r).unwrap();
        assert!(s.pressed(PadButton::B) && s.pressed(PadButton::RB) && s.pressed(PadButton::Misc));
        assert!(s.axis(Axis::LeftX) > 0.99);
        assert_eq!(s.axis(Axis::RightTrigger), 1.0);
        assert_eq!(s.touch, [None, None]);
    }

    #[test]
    fn reduced_bt_report() {
        let r = [0x01, 0x80, 0x80, 0x80, 0x80, 0x18, 0x00, 0x00, 0x00, 0x00];
        let s = parse_report(SonyModel::DualSense, &r).unwrap();
        assert!(s.pressed(PadButton::X));
        assert!(s.motion.is_none());
    }

    #[test]
    fn models() {
        assert_eq!(model_for(0x054C, 0x09CC), Some(SonyModel::DualShock4));
        assert_eq!(model_for(0x054C, 0x0CE6), Some(SonyModel::DualSense));
        assert_eq!(model_for(0x045E, 0x0CE6), None);
    }
}
