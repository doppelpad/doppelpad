//! Sony DualShock 4 and DualSense controllers: input reports, output reports (rumble, light
//! bar, lights, trigger effects) and the reports of a virtual DualShock 4.
//!
//! The report layouts are the publicly documented ones (Linux `hid-playstation`,
//! SDL `SDL_hidapi_ps4/ps5`). Pure functions, so they are unit-tested on every OS.

mod output;
mod virtual_ds4;

pub use self::virtual_ds4::{DS4_REPORT_LEN, Ds4Calibration, Ds4ReportBuilder};

use std::time::Instant;

use dp_core::pad::{Axis, Motion, PadButton, PadState, Rgb, Rumble, TouchPoint, unit_from_u8};

use self::output::{SonyOutput, output_report};
use crate::padio::{Identity, InitError, PadDriver, ReportIo, Seq4};
use crate::report::{apply_hat, apply_u8_sticks, le_i16, unpack12};
use crate::{Battery, Leds, PadFamily, TriggerEffects};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SonyModel {
    DualShock4,
    DualSense,
    DualSenseEdge,
}

pub const SONY_VID: u16 = 0x054C;
/// The first DualShock 4 (CUH-ZCT1), which a virtual DualShock 4 claims to be.
pub const DS4_V1_PID: u16 = 0x05C4;

pub fn model_for(vid: u16, pid: u16) -> Option<SonyModel> {
    if vid != SONY_VID {
        return None;
    }
    match pid {
        DS4_V1_PID | 0x09CC | 0x0BA0 => Some(SonyModel::DualShock4),
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
    fn enable_full_reports_feature(self) -> (u8, usize) {
        match self {
            SonyModel::DualShock4 => (0x02, 37),
            SonyModel::DualSense | SonyModel::DualSenseEdge => (0x05, 41),
        }
    }

    fn touch_extent(self) -> (f32, f32) {
        match self {
            SonyModel::DualShock4 => DS4_TOUCH,
            _ => DUALSENSE_TOUCH,
        }
    }
}

const GYRO_DPS_PER_LSB: f32 = 1.0 / 16.0;
const ACCEL_G_PER_LSB: f32 = 1.0 / 8192.0;

fn face_buttons(state: &mut PadState, b0: u8, b1: u8) {
    apply_hat(state, b0);
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

fn motion(r: &[u8], gyro_at: usize, accel_at: usize) -> Motion {
    let v = |at: usize| f32::from(le_i16(r, at));
    Motion {
        gyro: [
            v(gyro_at) * GYRO_DPS_PER_LSB,
            v(gyro_at + 2) * GYRO_DPS_PER_LSB,
            v(gyro_at + 4) * GYRO_DPS_PER_LSB,
        ],
        accel: [
            v(accel_at) * ACCEL_G_PER_LSB,
            v(accel_at + 2) * ACCEL_G_PER_LSB,
            v(accel_at + 4) * ACCEL_G_PER_LSB,
        ],
    }
}

/// 4-byte touch record: `[inactive:1 id:7] [x lo] [y lo:4 | x hi:4] [y hi]`.
fn touch(r: &[u8], at: usize, (w, h): (f32, f32)) -> Option<TouchPoint> {
    if r[at] & 0x80 != 0 {
        return None;
    }
    let (x, y) = unpack12(&r[at + 1..at + 4]);
    Some(TouchPoint {
        x: (f32::from(x) / w).clamp(0.0, 1.0),
        y: (f32::from(y) / h).clamp(0.0, 1.0),
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
            apply_u8_sticks(&mut s, &r[1..5]);
            face_buttons(&mut s, r[5], r[6]);
            s.set(PadButton::Guide, r[7] & 0x01 != 0);
            s.set(PadButton::Touchpad, r[7] & 0x02 != 0);
            s.set_axis(Axis::LeftTrigger, unit_from_u8(r[8]));
            s.set_axis(Axis::RightTrigger, unit_from_u8(r[9]));
        }
        (SonyModel::DualShock4, &id @ (0x01 | 0x11)) => {
            let c = if id == 0x11 { 3 } else { 1 };
            if r.len() < c + 42 {
                return None;
            }
            apply_u8_sticks(&mut s, &r[c..c + 4]);
            face_buttons(&mut s, r[c + 4], r[c + 5]);
            s.set(PadButton::Guide, r[c + 6] & 0x01 != 0);
            s.set(PadButton::Touchpad, r[c + 6] & 0x02 != 0);
            s.set_axis(Axis::LeftTrigger, unit_from_u8(r[c + 7]));
            s.set_axis(Axis::RightTrigger, unit_from_u8(r[c + 8]));
            s.motion = Some(motion(r, c + 12, c + 18));
            let ext = model.touch_extent();
            s.touch = [touch(r, c + 34, ext), touch(r, c + 38, ext)];
        }
        (_, &id @ (0x01 | 0x31)) => {
            let c = if id == 0x31 { 2 } else { 1 };
            if r.len() < c + 40 {
                return None;
            }
            apply_u8_sticks(&mut s, &r[c..c + 4]);
            s.set_axis(Axis::LeftTrigger, unit_from_u8(r[c + 4]));
            s.set_axis(Axis::RightTrigger, unit_from_u8(r[c + 5]));
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

/// The battery level (percent) from a 0–10 nibble: the middle of its tenth, at most 100.
fn level_from_nibble(n: u8) -> u8 {
    (n * 10 + 5).min(100)
}

/// The battery from a full input report (including its report-ID byte).
pub fn parse_battery(model: SonyModel, r: &[u8]) -> Option<Battery> {
    let status = match (model, r.first()?) {
        (_, 0x01) if r.len() < 32 => return None,
        // Low nibble 0–10 (11 on the cable when full), bit 4 the cable.
        (SonyModel::DualShock4, &id @ (0x01 | 0x11)) => {
            let s = *r.get(if id == 0x11 { 32 } else { 30 })?;
            let (n, cable) = (s & 0x0F, s & 0x10 != 0);
            return Some(Battery {
                level: level_from_nibble(n),
                charging: cable && n < 10,
                wired: false,
            });
        }
        (_, &id @ (0x01 | 0x31)) => *r.get(if id == 0x31 { 54 } else { 53 })?,
        _ => return None,
    };
    // Low nibble 0–10; high nibble 0 discharging, 1 charging, 2 full, others errors.
    Some(Battery {
        level: level_from_nibble(status & 0x0F),
        charging: status >> 4 == 1,
        wired: false,
    })
}

/// Driver for the raw-HID backends: asks for full reports and forwards rumble, the light
/// bar colour and the player and microphone lights.
pub struct SonyDriver {
    model: SonyModel,
    bluetooth: bool,
    feature_len: usize,
    lightbar: Option<Rgb>,
    leds: Leds,
    triggers: TriggerEffects,
    sent: Option<SonyOutput>,
    /// The DualSense light bar has been taken over from the firmware.
    set_up: bool,
    seq: Seq4,
    battery: Option<Battery>,
}

impl SonyDriver {
    /// `feature_len` is the device's feature report length.
    pub fn new(model: SonyModel, bluetooth: bool, feature_len: usize) -> Self {
        SonyDriver {
            model,
            bluetooth,
            feature_len,
            lightbar: None,
            leds: Leds::default(),
            triggers: [None; 2],
            sent: None,
            set_up: false,
            seq: Seq4::default(),
            battery: None,
        }
    }
}

/// Touchpad resolution of the DualShock 4.
/// Touchpad resolutions.
const DS4_TOUCH: (f32, f32) = (1920.0, 943.0);
const DUALSENSE_TOUCH: (f32, f32) = (1920.0, 1080.0);

impl PadDriver for SonyDriver {
    fn init(&mut self, io: &mut dyn ReportIo) -> Result<Identity, InitError> {
        let (id, len) = self.model.enable_full_reports_feature();
        let mut feature = vec![0u8; len.max(self.feature_len)];
        feature[0] = id;
        if let Err(e) = io.get_feature(&mut feature) {
            tracing::debug!("enabling full reports failed: {e}");
        }
        let family = match self.model {
            SonyModel::DualShock4 => PadFamily::DualShock4,
            _ => PadFamily::DualSense,
        };
        Ok(Identity {
            has_motion: true,
            has_touchpad: true,
            has_rumble: true,
            ..Identity::new(self.model.name(), family)
        })
    }

    fn parse(&mut self, report: &[u8], _now: Instant) -> Option<PadState> {
        let state = parse_report(self.model, report)?;
        if let Some(b) = parse_battery(self.model, report) {
            self.battery = Some(b);
        }
        Some(state)
    }

    fn battery(&self) -> Option<Battery> {
        self.battery
    }

    fn set_lightbar(&mut self, color: Option<Rgb>) {
        self.lightbar = color;
    }

    fn set_leds(&mut self, leds: Leds) {
        self.leds = leds;
    }

    fn set_trigger_effects(&mut self, effects: TriggerEffects) {
        // The DualShock 4 has no trigger motors.
        if self.model != SonyModel::DualShock4 {
            self.triggers = effects;
        }
    }

    fn service(&mut self, io: &mut dyn ReportIo, rumble: Rumble) -> std::io::Result<()> {
        let want = SonyOutput {
            rumble,
            lightbar: self.lightbar,
            leds: self.leds,
            triggers: self.triggers,
            setup: false,
        };
        // Nothing to say yet: leave the controller as it is.
        if self.sent.is_none() && want == SonyOutput::default() {
            return Ok(());
        }
        if self.sent == Some(want) {
            return Ok(());
        }
        self.sent = Some(want);
        let setup = self.model != SonyModel::DualShock4 && !self.set_up && want.lightbar.is_some();
        self.set_up |= setup;
        let report = output_report(
            self.model,
            self.bluetooth,
            &SonyOutput { setup, ..want },
            self.seq.step(),
        );
        io.write(&report)
    }

    fn next_deadline(&self) -> Option<Instant> {
        None
    }
}

#[cfg(test)]
mod tests {
    use dp_core::profile::TriggerEffect;

    use super::output::{crc32, effect_bytes};
    use super::*;
    use crate::report::{le_u16, le_u32};

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
        assert_eq!(io.writes()[0][2] & 0x04, 0, "light bar left alone");

        d.set_lightbar(Some(Rgb { r: 1, g: 2, b: 3 }));
        d.service(&mut io, r).unwrap();
        let w = io.writes()[1];
        assert_eq!((w[2] & 0x04, &w[45..48]), (0x04, &[1, 2, 3][..]));
    }

    #[test]
    fn ds4_output_report_carries_lightbar() {
        let red = SonyOutput {
            lightbar: Some(Rgb { r: 255, g: 0, b: 0 }),
            ..Default::default()
        };
        let r = output_report(SonyModel::DualShock4, false, &red, 0);
        assert_eq!((r[0], r[1], &r[6..9]), (0x05, 0x03, &[255, 0, 0][..]));
        let r = output_report(SonyModel::DualShock4, false, &SonyOutput::default(), 0);
        assert_eq!(r[1], 0x01);
        // Bluetooth: the same fields three bytes later, sealed with a CRC.
        let bt = output_report(SonyModel::DualShock4, true, &red, 0);
        assert_eq!(
            (bt.len(), bt[0], bt[1], bt[3], &bt[8..11]),
            (78, 0x11, 0xC0, 0x03, &[255, 0, 0][..])
        );
        let crc = crc32(std::iter::once(0xA2).chain(bt[..74].iter().copied()));
        assert_eq!(bt[74..], crc.to_le_bytes());
    }

    #[test]
    fn trigger_effects_encode_their_zones() {
        let r = effect_bytes(TriggerEffect::Resistance {
            start: 3,
            strength: 5,
        });
        let zones = le_u16(&r, 1);
        let force = le_u32(&r, 3);
        assert_eq!((r[0], zones), (0x21, 0b11_1111_1000));
        assert_eq!(force >> 9 & 7, 4, "strength 5 in zone 3");
        assert_eq!(force & 0x1FF, 0, "nothing below the start");
        let w = effect_bytes(TriggerEffect::Weapon {
            start: 4,
            end: 6,
            strength: 8,
        });
        assert_eq!(w[..4], [0x25, 0x50, 0x00, 7]);
        let v = effect_bytes(TriggerEffect::Vibration {
            start: 0,
            amplitude: 1,
            frequency: 30,
        });
        assert_eq!((v[0], v[1], v[2], v[9]), (0x26, 0xFF, 0x03, 30));
        assert_eq!(effect_bytes(TriggerEffect::Off)[0], 0x05);
        let raw = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
        assert_eq!(effect_bytes(TriggerEffect::Raw { bytes: raw }), raw);
        // In the report: right trigger at 11, left at 22 (USB, after the report ID).
        let o = SonyOutput {
            triggers: [
                Some(TriggerEffect::Off),
                Some(TriggerEffect::Raw { bytes: raw }),
            ],
            ..Default::default()
        };
        let r = output_report(SonyModel::DualSense, false, &o, 0);
        assert_eq!(r[1] & 0x0C, 0x0C);
        assert_eq!((&r[11..22], r[22]), (&raw[..], 0x05));
    }

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(*b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn dualsense_lights() {
        let o = SonyOutput {
            lightbar: Some(Rgb { r: 9, g: 8, b: 7 }),
            leds: Leds {
                player: Some(2),
                mic: Some(true),
            },
            setup: true,
            ..Default::default()
        };
        let usb = output_report(SonyModel::DualSense, false, &o, 0);
        assert_eq!(
            usb[2],
            0x01 | 0x04 | 0x10,
            "mic, light bar and player control"
        );
        assert_eq!((usb[9], usb[39], usb[42], usb[44]), (1, 0x02, 0x02, 0x0A));
        assert_eq!(usb[45..48], [9, 8, 7]);
        let bt = output_report(SonyModel::DualSense, true, &o, 5);
        assert_eq!((bt.len(), bt[0], bt[1], bt[2]), (78, 0x31, 0x50, 0x10));
        assert_eq!(bt[3..50], usb[1..48], "the same common part");
        let off = Leds {
            player: Some(0),
            mic: Some(false),
        };
        let r = output_report(
            SonyModel::DualSense,
            false,
            &SonyOutput {
                leds: off,
                ..Default::default()
            },
            0,
        );
        assert_eq!((r[2], r[9], r[44]), (0x11, 0, 0));
    }

    /// A report as the virtual DS4 sends it, prefixed with its report ID, parses back.
    fn reparse(b: &mut Ds4ReportBuilder, s: &PadState, us: u64) -> (Vec<u8>, PadState) {
        let mut raw = vec![0x01];
        raw.extend_from_slice(&b.build(s, us));
        let back = parse_report(SonyModel::DualShock4, &raw).unwrap();
        (raw, back)
    }

    #[test]
    fn virtual_ds4_report_round_trips() {
        let mut b = Ds4ReportBuilder::new(Ds4Calibration::default());
        let mut s = PadState::default();
        s.set(PadButton::A, true);
        s.set(PadButton::DpadUp, true);
        s.set(PadButton::DpadLeft, true);
        s.set(PadButton::Guide, true);
        s.set_axis(Axis::LeftX, 1.0);
        s.set_axis(Axis::RightY, -1.0);
        s.set_axis(Axis::RightTrigger, 0.5);
        s.motion = Some(Motion {
            gyro: [100.0, -50.0, 2.0],
            accel: [0.5, -1.0, 0.25],
        });
        s.touch[1] = Some(TouchPoint { x: 0.5, y: 0.25 });
        let (raw, back) = reparse(&mut b, &s, 0);
        assert_eq!(raw.len(), 64);
        assert_eq!(back.buttons, s.buttons);
        assert!(back.axis(Axis::LeftX) > 0.99 && back.axis(Axis::RightY) < -0.99);
        assert!((back.axis(Axis::RightTrigger) - 0.5).abs() < 0.01);
        assert_eq!(
            back.motion, s.motion,
            "nominal scale is exact for these values"
        );
        assert_eq!(back.touch[0], None);
        let t = back.touch[1].unwrap();
        assert!((t.x - 0.5).abs() < 1e-3 && (t.y - 0.25).abs() < 2e-3);
    }

    #[test]
    fn virtual_ds4_counters_and_touch_ids() {
        let mut b = Ds4ReportBuilder::new(Ds4Calibration::default());
        let mut s = PadState::default();
        let r0 = b.build(&s, 0);
        let r1 = b.build(&s, 3000);
        assert_eq!((r0[6] >> 2, r1[6] >> 2), (0, 1), "report counter");
        assert_eq!(r0[4] & 0x0F, 8, "hat released");
        assert_eq!(le_u16(&r1, 9), 562, "3 ms in 16/3 µs units");
        assert_eq!((r0[34], r0[38]), (0x80, 0x80), "no finger down");
        assert_eq!(le_i16(&r0, 20), 8192, "resting pad feels 1 g up");

        s.touch[0] = Some(TouchPoint { x: 1.0, y: 1.0 });
        let r = b.build(&s, 4000);
        assert_eq!(r[34], 0, "first contact gets id 0");
        assert_eq!(unpack12(&r[35..38]), (1919, 942));
        let counter = r[33];
        s.touch[0] = None;
        let r = b.build(&s, 5000);
        assert_eq!(r[34], 0x80, "lifted finger keeps its id");
        assert_eq!(r[33], counter, "touch counter only runs while touched");
        s.touch[0] = Some(TouchPoint { x: 0.0, y: 0.0 });
        assert_eq!(b.build(&s, 6000)[34], 1, "a new contact gets a new id");
    }

    /// A calibration report as a real pad sends it over USB.
    const DS4_CALIBRATION: [u8; 37] = [
        0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x87, 0x22, 0x7B, 0xDD, 0xB2, 0x22, 0x47, 0xDD,
        0xBD, 0x22, 0x43, 0xDD, 0x1C, 0x02, 0x1C, 0x02, 0x7F, 0x1E, 0x2E, 0xDF, 0x60, 0x1F, 0x4C,
        0xE0, 0x3A, 0x1D, 0xC6, 0xDE, 0x08, 0x00,
    ];

    #[test]
    fn calibration_report_is_inverted() {
        let feature = DS4_CALIBRATION;
        let c = Ds4Calibration::from_usb_feature(&feature).unwrap();
        // Pitch: 8838 + 8838 counts span 1080 °/s, bias 1.
        assert!((c.gyro_scale[0] - 17676.0 / 1080.0).abs() < 1e-3);
        assert_eq!(c.gyro_bias, [1.0, 0.0, 0.0]);
        // Accel x: 7807 - (-8402) = 16209 counts span 2 g, bias 7807 - 8104.
        assert!((c.accel_scale[0] - 8104.5).abs() < 1e-3);
        assert_eq!(c.accel_bias[0], -297.0);

        let mut b = Ds4ReportBuilder::new(c);
        let s = PadState {
            motion: Some(Motion {
                gyro: [90.0, 0.0, 0.0],
                accel: [0.0, 0.0, 1.0],
            }),
            ..Default::default()
        };
        let r = b.build(&s, 0);
        let raw = |at: usize| f32::from(le_i16(&r, at));
        // What a reader that applies this calibration gets back.
        let pitch = (raw(12) - 1.0) * 1080.0 / 17676.0;
        let z = (raw(22) - c.accel_bias[2]) / c.accel_scale[2];
        assert!((pitch - 90.0).abs() < 0.1, "{pitch}");
        assert!((z - 1.0).abs() < 1e-3, "{z}");
        assert!(Ds4Calibration::from_usb_feature(&feature[..20]).is_none());
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

    #[test]
    fn battery_from_status_bytes() {
        // DualShock 4 over USB: level 6, no cable; over Bluetooth on the cable at level 4.
        let mut usb = vec![0u8; 64];
        usb[0] = 0x01;
        usb[30] = 0x06;
        let b = parse_battery(SonyModel::DualShock4, &usb).unwrap();
        assert_eq!((b.level, b.charging), (65, false));
        let mut bt = vec![0u8; 78];
        bt[0] = 0x11;
        bt[32] = 0x14;
        let b = parse_battery(SonyModel::DualShock4, &bt).unwrap();
        assert_eq!((b.level, b.charging), (45, true));
        bt[32] = 0x1B; // full on the cable
        let b = parse_battery(SonyModel::DualShock4, &bt).unwrap();
        assert_eq!((b.level, b.charging), (100, false));
        // DualSense over Bluetooth: level 2, charging.
        let mut ds = vec![0u8; 78];
        ds[0] = 0x31;
        ds[54] = 0x12;
        let b = parse_battery(SonyModel::DualSense, &ds).unwrap();
        assert_eq!((b.level, b.charging), (25, true));
        // The reduced Bluetooth report has none.
        assert_eq!(parse_battery(SonyModel::DualSense, &[0x01; 10]), None);
    }

    /// A DualSense input report over USB (0x01) or Bluetooth (0x31) with sticks centred,
    /// the hat released and both touch points lifted.
    fn dualsense(id: u8) -> Vec<u8> {
        let (mut r, c) = if id == 0x31 {
            (vec![0u8; 78], 2)
        } else {
            (vec![0u8; 64], 1)
        };
        r[0] = id;
        r[c..c + 4].copy_from_slice(&[0x80; 4]);
        r[c + 7] = 0x08;
        r[c + 32] = 0x80;
        r[c + 36] = 0x80;
        r
    }

    #[test]
    fn dualsense_bluetooth() {
        let mut r = dualsense(0x31);
        r[2] = 0x00; // left stick fully left
        r[7] = 0xFF; // R2
        r[9] = 0x08 | 0x80; // hat released + triangle
        r[10] = 0x01; // L1
        r[11] = 0x01; // PS
        r[34] = 0x07; // touch 0 active
        r[54] = 0x23; // full, level 3
        let s = parse_report(SonyModel::DualSense, &r).unwrap();
        assert!(s.pressed(PadButton::Y) && s.pressed(PadButton::LB) && s.pressed(PadButton::Guide));
        assert!(s.axis(Axis::LeftX) < -0.99);
        assert_eq!(s.axis(Axis::RightTrigger), 1.0);
        assert!(s.motion.is_some());
        assert!(s.touch[0].is_some() && s.touch[1].is_none());
        let b = parse_battery(SonyModel::DualSense, &r).unwrap();
        assert_eq!((b.level, b.charging), (35, false));
    }

    #[test]
    fn dualsense_edge_paddles() {
        let mut r = dualsense(0x01);
        r[10] = 0x10 | 0x80; // left Fn + right back paddle
        let s = parse_report(SonyModel::DualSenseEdge, &r).unwrap();
        assert!(s.pressed(PadButton::PaddleUL) && s.pressed(PadButton::PaddleLR));
        assert!(!s.pressed(PadButton::PaddleUR) && !s.pressed(PadButton::PaddleLL));
        r[10] = 0x20 | 0x40; // right Fn + left back paddle
        let s = parse_report(SonyModel::DualSenseEdge, &r).unwrap();
        assert!(s.pressed(PadButton::PaddleUR) && s.pressed(PadButton::PaddleLL));
        let plain = parse_report(SonyModel::DualSense, &r).unwrap();
        assert_eq!(plain.buttons, 0, "a plain DualSense has no paddles");
    }

    #[test]
    fn truncated_reports_do_not_panic() {
        let mut ds4_bt = vec![0u8; 78];
        ds4_bt[0] = 0x11;
        ds4_bt[3..66].copy_from_slice(&ds4_usb()[1..]);
        let reports = [
            ds4_usb(),
            ds4_bt,
            dualsense(0x01),
            dualsense(0x31),
            vec![0x01, 0x80, 0x80, 0x80, 0x80, 0x08, 0, 0, 0, 0],
        ];
        let models = [
            SonyModel::DualShock4,
            SonyModel::DualSense,
            SonyModel::DualSenseEdge,
        ];
        for r in &reports {
            for model in models {
                for n in 0..=r.len() {
                    parse_report(model, &r[..n]);
                    parse_battery(model, &r[..n]);
                }
            }
        }
        let feature = DS4_CALIBRATION;
        assert!(Ds4Calibration::from_usb_feature(&feature).is_some());
        for n in 0..=feature.len() {
            Ds4Calibration::from_usb_feature(&feature[..n]);
        }
    }
}
