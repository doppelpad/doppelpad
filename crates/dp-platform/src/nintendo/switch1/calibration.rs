//! Stick and IMU calibration from the controller's flash.

use crate::nintendo::{CAL_MAGIC, StickCal};
use crate::report::{le_i16, unpack12};

fn six(b: &[u8]) -> [u16; 6] {
    let (a, b2, c) = (unpack12(&b[0..3]), unpack12(&b[3..6]), unpack12(&b[6..9]));
    [a.0, a.1, b2.0, b2.1, c.0, c.1]
}

/// Left stick: maximum above centre, centre, minimum below centre (x, y each).
pub(super) fn left_stick_cal(b: &[u8]) -> StickCal {
    let v = six(b);
    StickCal::switch1((v[2], v[3]), (v[0], v[1]), (v[4], v[5]))
}

/// Right stick: centre, minimum below centre, maximum above centre (x, y each).
pub(super) fn right_stick_cal(b: &[u8]) -> StickCal {
    let v = six(b);
    StickCal::switch1((v[0], v[1]), (v[4], v[5]), (v[2], v[3]))
}

/// Accelerometer and gyroscope scaling.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ImuCal {
    /// g per raw unit.
    pub accel_scale: [f32; 3],
    pub gyro_offset: [f32; 3],
    /// Degrees per second per raw unit.
    pub gyro_scale: [f32; 3],
}

impl ImuCal {
    pub const DEFAULT: ImuCal = ImuCal {
        accel_scale: [1.0 / 4096.0; 3],
        gyro_offset: [0.0; 3],
        gyro_scale: [1.0 / 14.2842; 3],
    };

    /// From the 24-byte factory block (accel origin, accel sensitivity, gyro origin, gyro
    /// sensitivity) and the optional user block (magic, accel origin, accel sensitivity,
    /// gyro origin), whose origins win when its magic is set.
    pub(super) fn from_spi(factory: &[u8], user: Option<&[u8]>) -> ImuCal {
        let word = |b: &[u8], i: usize| f32::from(le_i16(b, 2 * i));
        if factory.len() < 24 {
            return Self::DEFAULT;
        }
        let mut accel_origin = [word(factory, 0), word(factory, 1), word(factory, 2)];
        let accel_sens = [word(factory, 3), word(factory, 4), word(factory, 5)];
        let mut gyro_origin = [word(factory, 6), word(factory, 7), word(factory, 8)];
        let gyro_sens = [word(factory, 9), word(factory, 10), word(factory, 11)];
        if let Some(u) = user.filter(|u| u.len() >= 20 && u[..2] == CAL_MAGIC) {
            let u = &u[2..];
            accel_origin = [word(u, 0), word(u, 1), word(u, 2)];
            gyro_origin = [word(u, 6), word(u, 7), word(u, 8)];
        }
        let mut cal = Self::DEFAULT;
        for i in 0..3 {
            // Garbage (erased flash, licensed pads) would give absurd scales.
            let a = accel_sens[i] - accel_origin[i];
            if (1000.0..=40000.0).contains(&a) {
                cal.accel_scale[i] = 4.0 / a;
            }
            let g = gyro_sens[i] - gyro_origin[i];
            if (1000.0..=40000.0).contains(&g) {
                cal.gyro_scale[i] = 936.0 / g;
                cal.gyro_offset[i] = gyro_origin[i];
            }
        }
        cal
    }

    pub(super) fn accel_g(&self, raw: [i16; 3]) -> [f32; 3] {
        std::array::from_fn(|i| f32::from(raw[i]) * self.accel_scale[i])
    }

    pub(super) fn gyro_dps(&self, raw: [i16; 3]) -> [f32; 3] {
        std::array::from_fn(|i| (f32::from(raw[i]) - self.gyro_offset[i]) * self.gyro_scale[i])
    }
}

#[derive(Clone, Debug)]
pub struct Calibration {
    pub left: StickCal,
    pub right: StickCal,
    pub imu: ImuCal,
}

impl Default for Calibration {
    fn default() -> Self {
        let unset = StickCal::switch1_unset();
        Calibration {
            left: unset,
            right: unset,
            imu: ImuCal::DEFAULT,
        }
    }
}
