//! Report parsing for Sony DualShock 4 and DualSense controllers.
//!
//! The report layouts are the publicly documented ones (Linux `hid-playstation`,
//! SDL `SDL_hidapi_ps4/ps5`). Pure functions, so they are unit-tested on every OS.

use std::time::Instant;

use dp_core::pad::{
    Axis, Motion, PadButton, PadState, Rgb, Rumble, TouchPoint, axis_from_u8, axis_to_u8,
};

use crate::padio::{Identity, InitError, PadDriver, ReportIo};
use crate::{Battery, PadFamily};

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

/// The battery from a full input report (including its report-ID byte).
pub fn parse_battery(model: SonyModel, r: &[u8]) -> Option<Battery> {
    let status = match (model, r.first()?) {
        (_, 0x01) if r.len() < 32 => return None,
        // Low nibble 0–10 (11 on the cable when full), bit 4 the cable.
        (SonyModel::DualShock4, &id @ (0x01 | 0x11)) => {
            let s = *r.get(if id == 0x11 { 32 } else { 30 })?;
            let (n, cable) = (s & 0x0F, s & 0x10 != 0);
            return Some(Battery {
                level: (n * 10 + 5).min(100),
                charging: cable && n < 10,
                wired: false,
            });
        }
        (_, &id @ (0x01 | 0x31)) => *r.get(if id == 0x31 { 54 } else { 53 })?,
        _ => return None,
    };
    // Low nibble 0–10; high nibble 0 discharging, 1 charging, 2 full, others errors.
    Some(Battery {
        level: ((status & 0x0F) * 10 + 5).min(100),
        charging: status >> 4 == 1,
        wired: false,
    })
}

/// Build a USB output report that sets rumble and, if given, the light bar colour.
/// Bluetooth output needs a CRC'd report and is not implemented yet (returns `None`).
pub fn output_report(
    model: SonyModel,
    bluetooth: bool,
    rumble: Rumble,
    lightbar: Option<Rgb>,
) -> Option<Vec<u8>> {
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
            if let Some(c) = lightbar {
                r[1] |= 0x02; // light bar valid
                r[6..9].copy_from_slice(&[c.r, c.g, c.b]);
            }
            r
        }
        SonyModel::DualSense | SonyModel::DualSenseEdge => {
            let mut r = vec![0u8; 48];
            r[0] = 0x02;
            r[1] = 0x03; // compatible vibration + haptics select
            r[3] = high;
            r[4] = low;
            if let Some(c) = lightbar {
                r[2] |= 0x04; // light bar control
                r[45..48].copy_from_slice(&[c.r, c.g, c.b]);
            }
            r
        }
    })
}

/// Driver for the raw-HID backends: asks for full reports and forwards rumble and the
/// light bar colour.
pub struct SonyDriver {
    model: SonyModel,
    bluetooth: bool,
    feature_len: usize,
    lightbar: Option<Rgb>,
    sent: (Rumble, Option<Rgb>),
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
            sent: (Rumble::default(), None),
            battery: None,
        }
    }
}

/// Size of a DualShock 4 USB input report without its report-ID byte.
pub const DS4_REPORT_LEN: usize = 63;

/// How raw DualShock 4 motion values relate to physical units, as a game reads it from
/// the controller's calibration feature report (0x02 over USB). Raw = value × scale + bias.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Ds4Calibration {
    /// Counts per °/s, pitch/yaw/roll.
    pub gyro_scale: [f32; 3],
    pub gyro_bias: [f32; 3],
    /// Counts per g, x/y/z.
    pub accel_scale: [f32; 3],
    pub accel_bias: [f32; 3],
}

impl Default for Ds4Calibration {
    /// The nominal scale [`parse_report`] assumes, without bias.
    fn default() -> Self {
        Ds4Calibration {
            gyro_scale: [1.0 / GYRO_DPS_PER_LSB; 3],
            gyro_bias: [0.0; 3],
            accel_scale: [1.0 / ACCEL_G_PER_LSB; 3],
            accel_bias: [0.0; 3],
        }
    }
}

impl Ds4Calibration {
    /// Decode a USB calibration feature report (report ID first, at least 35 bytes), the
    /// way the public drivers interpret it: the gyro range between the plus and minus
    /// readings spans `speed_plus + speed_minus` °/s, the accelerometer range spans 2 g.
    pub fn from_usb_feature(r: &[u8]) -> Option<Self> {
        if r.len() < 35 || r[0] != 0x02 {
            return None;
        }
        let v = |at: usize| i16::from_le_bytes([r[at], r[at + 1]]) as f32;
        let speed = v(19) + v(21);
        let mut c = Ds4Calibration::default();
        for i in 0..3 {
            let bias = v(1 + 2 * i);
            let (plus, minus) = (v(7 + 4 * i), v(9 + 4 * i));
            let range = (plus - bias).abs() + (minus - bias).abs();
            if speed <= 0.0 || range <= 0.0 {
                return None;
            }
            c.gyro_scale[i] = range / speed;
            c.gyro_bias[i] = bias;
            let (plus, minus) = (v(23 + 4 * i), v(25 + 4 * i));
            let range = plus - minus;
            if range <= 0.0 {
                return None;
            }
            c.accel_scale[i] = range / 2.0;
            c.accel_bias[i] = plus - (range / 2.0).trunc();
        }
        Some(c)
    }

    fn encode(value: f32, scale: f32, bias: f32) -> [u8; 2] {
        let raw = (value * scale + bias)
            .round()
            .clamp(i16::MIN as f32, i16::MAX as f32);
        (raw as i16).to_le_bytes()
    }
}

/// Last state of one touch point in the virtual report.
#[derive(Copy, Clone, Debug, Default)]
struct Finger {
    id: u8,
    down: bool,
    x: u16,
    y: u16,
}

/// Builds DualShock 4 USB input reports (the 63 bytes after the report ID) for a virtual
/// controller, keeping the report counter, the sensor timestamp and touch tracking IDs.
#[derive(Clone, Debug)]
pub struct Ds4ReportBuilder {
    calibration: Ds4Calibration,
    counter: u8,
    touch_counter: u8,
    next_id: u8,
    fingers: [Finger; 2],
}

/// The pad at rest on a table: no rotation, gravity along +y.
const RESTING: Motion = Motion {
    gyro: [0.0; 3],
    accel: [0.0, 1.0, 0.0],
};

impl Ds4ReportBuilder {
    /// `calibration` is what the virtual device tells games in its calibration report.
    pub fn new(calibration: Ds4Calibration) -> Self {
        Ds4ReportBuilder {
            calibration,
            counter: 0,
            touch_counter: 0,
            next_id: 0,
            fingers: [Finger::default(); 2],
        }
    }

    /// One report for `s`; `elapsed_us` is the time since the device was created.
    pub fn build(&mut self, s: &PadState, elapsed_us: u64) -> [u8; DS4_REPORT_LEN] {
        let mut r = [0u8; DS4_REPORT_LEN];
        r[0] = axis_to_u8(s.axis(Axis::LeftX), false);
        r[1] = axis_to_u8(s.axis(Axis::LeftY), true);
        r[2] = axis_to_u8(s.axis(Axis::RightX), false);
        r[3] = axis_to_u8(s.axis(Axis::RightY), true);
        let buttons = ds4_buttons(s);
        r[4..6].copy_from_slice(&buttons.to_le_bytes());
        let mut special = self.counter << 2;
        self.counter = (self.counter + 1) & 0x3F;
        if s.pressed(PadButton::Guide) {
            special |= 0x01;
        }
        if s.pressed(PadButton::Touchpad) {
            special |= 0x02;
        }
        r[6] = special;
        r[7] = (s.axis(Axis::LeftTrigger).clamp(0.0, 1.0) * 255.0).round() as u8;
        r[8] = (s.axis(Axis::RightTrigger).clamp(0.0, 1.0) * 255.0).round() as u8;
        // Sensor timestamp in units of 16/3 µs.
        let stamp = (elapsed_us * 3 / 16) as u16;
        r[9..11].copy_from_slice(&stamp.to_le_bytes());
        let m = s.motion.unwrap_or(RESTING);
        let c = &self.calibration;
        for i in 0..3 {
            let g = Ds4Calibration::encode(m.gyro[i], c.gyro_scale[i], c.gyro_bias[i]);
            r[12 + 2 * i..14 + 2 * i].copy_from_slice(&g);
            let a = Ds4Calibration::encode(m.accel[i], c.accel_scale[i], c.accel_bias[i]);
            r[18 + 2 * i..20 + 2 * i].copy_from_slice(&a);
        }
        r[29] = 0x1B; // cable connected, battery full
        r[32] = 1; // one touch record
        self.touch(s, &mut r[33..42]);
        r
    }

    /// Touch record: packet counter, then two points of
    /// `[inactive:1 id:7] [x lo] [y lo:4 | x hi:4] [y hi]`.
    fn touch(&mut self, s: &PadState, out: &mut [u8]) {
        for (i, point) in s.touch.iter().enumerate() {
            let f = &mut self.fingers[i];
            match point {
                Some(p) => {
                    if !f.down {
                        f.id = self.next_id;
                        self.next_id = (self.next_id + 1) & 0x7F;
                        f.down = true;
                    }
                    f.x = (p.x.clamp(0.0, 1.0) * DS4_TOUCH.0)
                        .round()
                        .min(DS4_TOUCH.0 - 1.0) as u16;
                    f.y = (p.y.clamp(0.0, 1.0) * DS4_TOUCH.1)
                        .round()
                        .min(DS4_TOUCH.1 - 1.0) as u16;
                }
                None => f.down = false,
            }
        }
        if self.fingers.iter().any(|f| f.down) {
            self.touch_counter = self.touch_counter.wrapping_add(1);
        }
        out[0] = self.touch_counter;
        for (i, f) in self.fingers.iter().enumerate() {
            let p = &mut out[1 + 4 * i..5 + 4 * i];
            p[0] = f.id | if f.down { 0 } else { 0x80 };
            p[1] = f.x as u8;
            p[2] = ((f.x >> 8) as u8 & 0x0F) | ((f.y as u8 & 0x0F) << 4);
            p[3] = (f.y >> 4) as u8;
        }
    }
}

/// Touchpad resolution of the DualShock 4.
const DS4_TOUCH: (f32, f32) = (1920.0, 943.0);

/// Hat in the low nibble, then square, cross, circle, triangle, L1, R1, L2, R2, share,
/// options, L3, R3.
fn ds4_buttons(s: &PadState) -> u16 {
    let hat = match (
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
    };
    let mut buttons = hat as u16;
    let bits = [
        (PadButton::X, 1 << 4),  // square
        (PadButton::A, 1 << 5),  // cross
        (PadButton::B, 1 << 6),  // circle
        (PadButton::Y, 1 << 7),  // triangle
        (PadButton::LB, 1 << 8), // L1
        (PadButton::RB, 1 << 9), // R1
        (PadButton::Back, 1 << 12),
        (PadButton::Start, 1 << 13),
        (PadButton::LS, 1 << 14),
        (PadButton::RS, 1 << 15),
    ];
    for (b, bit) in bits {
        if s.pressed(b) {
            buttons |= bit;
        }
    }
    if s.axis(Axis::LeftTrigger) > 0.0 {
        buttons |= 1 << 10; // L2 digital
    }
    if s.axis(Axis::RightTrigger) > 0.0 {
        buttons |= 1 << 11; // R2 digital
    }
    buttons
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

    fn service(&mut self, io: &mut dyn ReportIo, rumble: Rumble) -> std::io::Result<()> {
        let want = (rumble, self.lightbar);
        if want == self.sent {
            return Ok(());
        }
        self.sent = want;
        match output_report(self.model, self.bluetooth, rumble, self.lightbar) {
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
        assert_eq!(io.writes()[0][2] & 0x04, 0, "light bar left alone");

        d.set_lightbar(Some(Rgb { r: 1, g: 2, b: 3 }));
        d.service(&mut io, r).unwrap();
        let w = io.writes()[1];
        assert_eq!((w[2] & 0x04, &w[45..48]), (0x04, &[1, 2, 3][..]));
    }

    #[test]
    fn ds4_output_report_carries_lightbar() {
        let red = Some(Rgb { r: 255, g: 0, b: 0 });
        let r = output_report(SonyModel::DualShock4, false, Rumble::default(), red).unwrap();
        assert_eq!((r[0], r[1], &r[6..9]), (0x05, 0x03, &[255, 0, 0][..]));
        let r = output_report(SonyModel::DualShock4, false, Rumble::default(), None).unwrap();
        assert_eq!(r[1], 0x01);
        assert!(output_report(SonyModel::DualShock4, true, Rumble::default(), red).is_none());
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
        assert_eq!(
            u16::from_le_bytes([r1[9], r1[10]]),
            562,
            "3 ms in 16/3 µs units"
        );
        assert_eq!((r0[34], r0[38]), (0x80, 0x80), "no finger down");
        assert_eq!(
            i16::from_le_bytes([r0[20], r0[21]]),
            8192,
            "resting pad feels 1 g up"
        );

        s.touch[0] = Some(TouchPoint { x: 1.0, y: 1.0 });
        let r = b.build(&s, 4000);
        assert_eq!(r[34], 0, "first contact gets id 0");
        let x = r[35] as u16 | ((r[36] as u16 & 0x0F) << 8);
        let y = (r[36] as u16 >> 4) | ((r[37] as u16) << 4);
        assert_eq!((x, y), (1919, 942));
        let counter = r[33];
        s.touch[0] = None;
        let r = b.build(&s, 5000);
        assert_eq!(r[34], 0x80, "lifted finger keeps its id");
        assert_eq!(r[33], counter, "touch counter only runs while touched");
        s.touch[0] = Some(TouchPoint { x: 0.0, y: 0.0 });
        assert_eq!(b.build(&s, 6000)[34], 1, "a new contact gets a new id");
    }

    #[test]
    fn calibration_report_is_inverted() {
        // A calibration report as a real pad sends it over USB.
        let feature = [
            0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x87, 0x22, 0x7B, 0xDD, 0xB2, 0x22, 0x47,
            0xDD, 0xBD, 0x22, 0x43, 0xDD, 0x1C, 0x02, 0x1C, 0x02, 0x7F, 0x1E, 0x2E, 0xDF, 0x60,
            0x1F, 0x4C, 0xE0, 0x3A, 0x1D, 0xC6, 0xDE, 0x08, 0x00,
        ];
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
        let raw = |at: usize| i16::from_le_bytes([r[at], r[at + 1]]) as f32;
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
}
