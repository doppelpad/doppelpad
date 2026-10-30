//! The input reports of a virtual DualShock 4 (served by the ViGEm backend, not a physical
//! pad).

use dp_core::pad::{Axis, Motion, PadButton, PadState, Stick, stick_to_hid_u8, unit_to_u8_round};

use super::{ACCEL_G_PER_LSB, DS4_TOUCH, GYRO_DPS_PER_LSB};
use crate::report::{hat_from_dpad, le_i16, pack12};

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
        let v = |at: usize| f32::from(le_i16(r, at));
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
            .clamp(f32::from(i16::MIN), f32::from(i16::MAX));
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
        r[0..2].copy_from_slice(&stick_to_hid_u8(s.stick(Stick::Left)));
        r[2..4].copy_from_slice(&stick_to_hid_u8(s.stick(Stick::Right)));
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
        r[7] = unit_to_u8_round(s.axis(Axis::LeftTrigger));
        r[8] = unit_to_u8_round(s.axis(Axis::RightTrigger));
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
            p[1..4].copy_from_slice(&pack12(f.x, f.y));
        }
    }
}

/// Hat in the low nibble, then the buttons as the controller reports them ([`super::BUTTONS`]),
/// and L2 and R2 from the triggers.
fn ds4_buttons(s: &PadState) -> u16 {
    let mut buttons = u16::from(hat_from_dpad(s));
    for (b, bit) in super::BUTTONS {
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
