//! The report formats: building output reports, matching replies, parsing input.

use dp_core::pad::{Axis, Motion, PadState};

use super::ControllerType;
use super::calibration::{Calibration, ImuCal};
use crate::Battery;
use crate::nintendo::{apply_buttons, imu_to_pad};
use crate::report::{le_i16, unpack12};

pub(super) fn usb_command(cmd: u8) -> [u8; 2] {
    [0x80, cmd]
}

pub(super) fn is_usb_ack(r: &[u8], cmd: u8) -> bool {
    r.len() >= 2 && r[0] == 0x81 && r[1] == cmd
}

/// Output report 0x01: rumble for both actuators plus one subcommand.
pub(super) fn subcommand_report(counter: u8, rumble: &[u8; 8], sub: u8, args: &[u8]) -> Vec<u8> {
    let mut r = Vec::with_capacity(11 + args.len());
    r.extend_from_slice(&[0x01, counter & 0x0F]);
    r.extend_from_slice(rumble);
    r.push(sub);
    r.extend_from_slice(args);
    r
}

/// Output report 0x10: rumble only.
pub fn rumble_report(counter: u8, rumble: &[u8; 8]) -> Vec<u8> {
    let mut r = vec![0x10, counter & 0x0F];
    r.extend_from_slice(rumble);
    r
}

/// Data of an acknowledged reply (input report 0x21) to subcommand `sub`.
pub(super) fn subcommand_reply(r: &[u8], sub: u8) -> Option<&[u8]> {
    (r.len() > 15 && r[0] == 0x21 && r[13] & 0x80 != 0 && r[14] == sub).then(|| &r[15..])
}

pub(super) fn spi_read_args(addr: u32, len: u8) -> [u8; 5] {
    let a = addr.to_le_bytes();
    [a[0], a[1], a[2], a[3], len]
}

/// The flash bytes of an SPI-read reply, if it answers this exact read.
pub(super) fn spi_data(reply: &[u8], addr: u32, len: u8) -> Option<&[u8]> {
    let end = 5 + len as usize;
    (reply.len() >= end && reply[..5] == spi_read_args(addr, len)).then(|| &reply[5..end])
}

/// Average of the three IMU frames of a full report, in our axes. `None` while the IMU
/// sends nothing (all-zero accelerometer).
fn parse_motion(ty: ControllerType, cal: &ImuCal, r: &[u8]) -> Option<Motion> {
    if r[13..19].iter().all(|&b| b == 0) {
        return None;
    }
    let mut accel = [0.0f32; 3];
    let mut gyro = [0.0f32; 3];
    for frame in 0..3 {
        let at = 13 + frame * 12;
        let a = cal.accel_g([le_i16(r, at), le_i16(r, at + 2), le_i16(r, at + 4)]);
        let g = cal.gyro_dps([le_i16(r, at + 6), le_i16(r, at + 8), le_i16(r, at + 10)]);
        for i in 0..3 {
            accel[i] += a[i] / 3.0;
            gyro[i] += g[i] / 3.0;
        }
    }
    let (mut accel, mut gyro) = (imu_to_pad(accel), imu_to_pad(gyro));
    // The right Joy-Con's sensor is mounted mirrored.
    if ty == ControllerType::JoyConRight {
        for v in [&mut accel, &mut gyro] {
            v[0] = -v[0];
            v[1] = -v[1];
        }
    }
    Some(Motion { gyro, accel })
}

/// The battery from byte 2 of a 0x21, 0x30 or 0x31 report: bits 5–7 the level (empty,
/// critical, low, medium, full), bit 4 charging.
pub fn parse_battery(r: &[u8]) -> Option<Battery> {
    if r.len() < 3 || !matches!(r[0], 0x21 | 0x30 | 0x31) {
        return None;
    }
    Some(Battery {
        level: [0, 10, 30, 60, 100][(r[2] >> 5).min(4) as usize],
        charging: r[2] & 0x10 != 0,
        wired: false,
    })
}

/// Parse a full report (0x30 / 0x31) into a pad state, with sticks calibrated.
pub(super) fn parse_full(ty: ControllerType, cal: &mut Calibration, r: &[u8]) -> Option<PadState> {
    if r.len() < 49 || !matches!(r[0], 0x30 | 0x31) {
        return None;
    }
    let mut s = PadState::default();
    apply_buttons(&mut s, r[3], r[4], r[5]);
    let (lx, ly) = cal.left.normalize(unpack12(&r[6..9]));
    let (rx, ry) = cal.right.normalize(unpack12(&r[9..12]));
    s.set_axis(Axis::LeftX, lx);
    s.set_axis(Axis::LeftY, ly);
    s.set_axis(Axis::RightX, rx);
    s.set_axis(Axis::RightY, ry);
    if ty.has_imu() {
        s.motion = parse_motion(ty, &cal.imu, r);
    }
    Some(s)
}
