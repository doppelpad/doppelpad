//! First-generation Switch protocol: Pro Controller, Joy-Con, charging grip and the
//! Nintendo Switch Online pads, over USB and Bluetooth.
//!
//! Output report 0x01 carries rumble plus a subcommand, 0x10 rumble only; the pad answers
//! subcommands with input report 0x21 and streams its state as 0x30 once switched to full
//! mode. Over USB a short handshake (proprietary report 0x80) must come first.

use std::io;
use std::time::{Duration, Instant};

use dp_core::pad::{Axis, Motion, PadState, Rumble};

use super::{NINTENDO_VID, StickCal, apply_buttons, imu_to_pad, unpack12};
use crate::nintendo::player_mask;
use crate::padio::{Identity, InitError, PadDriver, ReportIo, RumblePacer, pause};
use crate::{Battery, JoyConSide, Leds, PadFamily};

/// Rumble data for "no vibration" (320 Hz / 160 Hz at amplitude 0).
pub const NEUTRAL_RUMBLE: [u8; 4] = [0x00, 0x01, 0x40, 0x40];

const REPLY_TIMEOUT: Duration = Duration::from_millis(100);
/// Without input for this long a USB pad has been reset (e.g. by Steam): force USB again.
const USB_KEEPALIVE: Duration = Duration::from_millis(100);
const MODE_RETRY: Duration = Duration::from_secs(1);
const IMU_RETRY: Duration = Duration::from_secs(3);

const SUB_DEVICE_INFO: u8 = 0x02;
const SUB_REPORT_MODE: u8 = 0x03;
const SUB_SPI_READ: u8 = 0x10;
const SUB_HCI_STATE: u8 = 0x06;
const SUB_PLAYER_LEDS: u8 = 0x30;
const SUB_HOME_LED: u8 = 0x38;
const SUB_IMU: u8 = 0x40;
const SUB_VIBRATION: u8 = 0x48;

const MODE_FULL: u8 = 0x30;
const MODE_SIMPLE: u8 = 0x3F;

const USB_STATUS: u8 = 0x01;
const USB_HANDSHAKE: u8 = 0x02;
const USB_HIGH_SPEED: u8 = 0x03;
const USB_FORCE: u8 = 0x04;

const SPI_FACTORY_STICKS: u32 = 0x603D;
const SPI_USER_STICKS: u32 = 0x8010;
const SPI_FACTORY_IMU: u32 = 0x6020;
const SPI_USER_IMU: u32 = 0x8026;
const CAL_MAGIC: [u8; 2] = [0xB2, 0xA1];

/// Controller type byte (device-info reply / USB status reply).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ControllerType {
    JoyConLeft,
    JoyConRight,
    Pro,
    LicensedPro,
    FamicomLeft,
    FamicomRight,
    NesLeft,
    NesRight,
    Snes,
    N64,
    Genesis,
}

impl ControllerType {
    pub fn from_byte(b: u8) -> Option<Self> {
        use ControllerType::*;
        Some(match b {
            0x01 => JoyConLeft,
            0x02 => JoyConRight,
            0x03 => Pro,
            0x06 => LicensedPro,
            0x07 => FamicomLeft,
            0x08 => FamicomRight,
            0x09 => NesLeft,
            0x0A => NesRight,
            0x0B => Snes,
            0x0C => N64,
            0x0D => Genesis,
            _ => return None,
        })
    }

    /// Type to assume when the pad does not tell (or for the slot of a charging grip).
    pub fn fallback(vid: u16, pid: u16, grip_slot: Option<JoyConSide>) -> Self {
        match (vid, pid) {
            (NINTENDO_VID, 0x2006) => ControllerType::JoyConLeft,
            (NINTENDO_VID, 0x2007) => ControllerType::JoyConRight,
            (NINTENDO_VID, 0x200E) if grip_slot == Some(JoyConSide::Left) => {
                ControllerType::JoyConLeft
            }
            (NINTENDO_VID, 0x200E) => ControllerType::JoyConRight,
            (NINTENDO_VID, 0x2017) => ControllerType::Snes,
            (NINTENDO_VID, 0x2019) => ControllerType::N64,
            (NINTENDO_VID, 0x201E) => ControllerType::Genesis,
            (NINTENDO_VID, _) => ControllerType::Pro,
            _ => ControllerType::LicensedPro,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ControllerType::JoyConLeft => "Joy-Con (L)",
            ControllerType::JoyConRight => "Joy-Con (R)",
            ControllerType::Pro => "Switch Pro Controller",
            ControllerType::LicensedPro => "Switch-compatible controller",
            ControllerType::FamicomLeft => "Famicom Controller (1)",
            ControllerType::FamicomRight => "Famicom Controller (2)",
            ControllerType::NesLeft => "NES Controller (L)",
            ControllerType::NesRight => "NES Controller (R)",
            ControllerType::Snes => "SNES Controller",
            ControllerType::N64 => "N64 Controller",
            ControllerType::Genesis => "Genesis Controller",
        }
    }

    pub fn family(self) -> PadFamily {
        match self {
            ControllerType::JoyConLeft | ControllerType::JoyConRight => PadFamily::JoyCon,
            _ => PadFamily::SwitchPro,
        }
    }

    pub fn joycon(self) -> Option<JoyConSide> {
        match self {
            ControllerType::JoyConLeft => Some(JoyConSide::Left),
            ControllerType::JoyConRight => Some(JoyConSide::Right),
            _ => None,
        }
    }

    pub fn has_imu(self) -> bool {
        matches!(
            self,
            ControllerType::JoyConLeft
                | ControllerType::JoyConRight
                | ControllerType::Pro
                | ControllerType::LicensedPro
        )
    }

    pub fn has_rumble(self) -> bool {
        self.has_imu() || self == ControllerType::N64
    }

    /// Only genuine pads with a Home button have its LED; third-party pads may even
    /// switch off when asked to set it.
    fn has_home_led(self) -> bool {
        matches!(self, ControllerType::Pro | ControllerType::JoyConRight)
    }
}

// ---------------------------------------------------------------------------------------
// Report building and matching
// ---------------------------------------------------------------------------------------

pub fn usb_command(cmd: u8) -> [u8; 2] {
    [0x80, cmd]
}

pub fn is_usb_ack(r: &[u8], cmd: u8) -> bool {
    r.len() >= 2 && r[0] == 0x81 && r[1] == cmd
}

/// Output report 0x01: rumble for both actuators plus one subcommand.
pub fn subcommand_report(counter: u8, rumble: &[u8; 8], sub: u8, args: &[u8]) -> Vec<u8> {
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
pub fn subcommand_reply(r: &[u8], sub: u8) -> Option<&[u8]> {
    (r.len() > 15 && r[0] == 0x21 && r[13] & 0x80 != 0 && r[14] == sub).then(|| &r[15..])
}

pub fn spi_read_args(addr: u32, len: u8) -> [u8; 5] {
    let a = addr.to_le_bytes();
    [a[0], a[1], a[2], a[3], len]
}

/// The flash bytes of an SPI-read reply, if it answers this exact read.
pub fn spi_data(reply: &[u8], addr: u32, len: u8) -> Option<&[u8]> {
    let end = 5 + len as usize;
    (reply.len() >= end && reply[..5] == spi_read_args(addr, len)).then(|| &reply[5..end])
}

// ---------------------------------------------------------------------------------------
// Calibration
// ---------------------------------------------------------------------------------------

fn six(b: &[u8]) -> [u16; 6] {
    let (a, b2, c) = (unpack12(&b[0..3]), unpack12(&b[3..6]), unpack12(&b[6..9]));
    [a.0, a.1, b2.0, b2.1, c.0, c.1]
}

/// Left stick: maximum above centre, centre, minimum below centre (x, y each).
pub fn left_stick_cal(b: &[u8]) -> StickCal {
    let v = six(b);
    StickCal::switch1((v[2], v[3]), (v[0], v[1]), (v[4], v[5]))
}

/// Right stick: centre, minimum below centre, maximum above centre (x, y each).
pub fn right_stick_cal(b: &[u8]) -> StickCal {
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
    pub fn from_spi(factory: &[u8], user: Option<&[u8]>) -> ImuCal {
        let word = |b: &[u8], i: usize| i16::from_le_bytes([b[2 * i], b[2 * i + 1]]) as f32;
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

    pub fn accel_g(&self, raw: [i16; 3]) -> [f32; 3] {
        std::array::from_fn(|i| raw[i] as f32 * self.accel_scale[i])
    }

    pub fn gyro_dps(&self, raw: [i16; 3]) -> [f32; 3] {
        std::array::from_fn(|i| (raw[i] as f32 - self.gyro_offset[i]) * self.gyro_scale[i])
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
        let unset = StickCal::switch1((0xFFF, 0xFFF), (0xFFF, 0xFFF), (0xFFF, 0xFFF));
        Calibration {
            left: unset,
            right: unset,
            imu: ImuCal::DEFAULT,
        }
    }
}

// ---------------------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------------------

fn i16le(r: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([r[at], r[at + 1]])
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
        let a = cal.accel_g([i16le(r, at), i16le(r, at + 2), i16le(r, at + 4)]);
        let g = cal.gyro_dps([i16le(r, at + 6), i16le(r, at + 8), i16le(r, at + 10)]);
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
pub fn parse_full(ty: ControllerType, cal: &mut Calibration, r: &[u8]) -> Option<PadState> {
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

// ---------------------------------------------------------------------------------------
// Rumble
// ---------------------------------------------------------------------------------------

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
pub fn hd_rumble_at(high_hz: f32, low_hz: f32, amp: f32) -> [u8; 4] {
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
pub fn hd_rumble(amp: f32) -> [u8; 4] {
    hd_rumble_at(320.0, 160.0, amp)
}

/// Rumble data for both actuators. A Pro Controller gets the heavy motor on its left
/// actuator and the light one on the right; a Joy-Con has a single actuator.
pub fn rumble_data(ty: ControllerType, r: Rumble) -> [u8; 8] {
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

fn neutral8() -> [u8; 8] {
    let mut r = [0u8; 8];
    r[..4].copy_from_slice(&NEUTRAL_RUMBLE);
    r[4..].copy_from_slice(&NEUTRAL_RUMBLE);
    r
}

// ---------------------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------------------

/// How the pad is connected and what the host knows about it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub bluetooth: bool,
    pub vid: u16,
    pub pid: u16,
    /// Charging-grip slot of this HID interface.
    pub grip_slot: Option<JoyConSide>,
}

impl Link {
    fn is_grip(&self) -> bool {
        (self.vid, self.pid) == (NINTENDO_VID, 0x200E)
    }
}

pub struct Switch1Driver {
    link: Link,
    ty: ControllerType,
    cal: Calibration,
    counter: u8,
    rumble: [u8; 8],
    pacer: RumblePacer,
    last_command: Option<Instant>,
    /// First input report ID seen, so shutdown can restore it for other programs.
    initial_mode: Option<u8>,
    last_input: Option<Instant>,
    last_keepalive: Option<Instant>,
    mode_lost: bool,
    last_mode_fix: Option<Instant>,
    imu_seen: bool,
    imu_lost: bool,
    last_imu_fix: Option<Instant>,
    now: Option<Instant>,
    battery: Option<Battery>,
    /// Player lights wanted and last sent.
    player_want: Option<u8>,
    player_sent: Option<u8>,
}

impl Switch1Driver {
    pub fn new(link: Link) -> Self {
        Switch1Driver {
            link,
            ty: ControllerType::fallback(link.vid, link.pid, link.grip_slot),
            cal: Calibration::default(),
            counter: 0,
            rumble: neutral8(),
            pacer: RumblePacer::new(
                Duration::from_millis(30),
                Some(Duration::from_millis(50)),
                3,
            ),
            last_command: None,
            initial_mode: None,
            last_input: None,
            last_keepalive: None,
            mode_lost: false,
            last_mode_fix: None,
            imu_seen: false,
            battery: None,
            player_want: None,
            player_sent: Some(0x01),
            imu_lost: false,
            last_imu_fix: None,
            now: None,
        }
    }

    pub fn controller_type(&self) -> ControllerType {
        self.ty
    }

    fn attempts(&self) -> usize {
        if self.link.is_grip() { 20 } else { 5 }
    }

    /// Minimum spacing between subcommands.
    fn gap(&self) -> Duration {
        Duration::from_millis(if self.link.bluetooth { 60 } else { 20 })
    }

    fn next_counter(&mut self) -> u8 {
        let c = self.counter;
        self.counter = (self.counter + 1) & 0x0F;
        c
    }

    /// Read reports until `want` accepts one or the reply timeout passes.
    fn wait_for<T>(
        &mut self,
        io: &mut dyn ReportIo,
        mut want: impl FnMut(&[u8]) -> Option<T>,
    ) -> io::Result<Option<T>> {
        let deadline = io.now() + REPLY_TIMEOUT;
        let mut buf = [0u8; 64];
        loop {
            let now = io.now();
            if now >= deadline {
                return Ok(None);
            }
            let Some(n) = io.read(&mut buf, deadline - now)? else {
                return Ok(None);
            };
            let r = &buf[..n];
            if self.initial_mode.is_none() && matches!(r.first(), Some(0x30 | 0x31 | 0x3F)) {
                self.initial_mode = Some(r[0]);
            }
            if let Some(v) = want(r) {
                return Ok(Some(v));
            }
        }
    }

    fn usb(&mut self, io: &mut dyn ReportIo, cmd: u8, wait: bool) -> io::Result<bool> {
        for _ in 0..self.attempts() {
            io.write(&usb_command(cmd))?;
            if !wait {
                return Ok(true);
            }
            if self
                .wait_for(io, |r| is_usb_ack(r, cmd).then_some(()))?
                .is_some()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The USB status reply's controller type byte.
    fn usb_status(&mut self, io: &mut dyn ReportIo) -> io::Result<Option<u8>> {
        for _ in 0..self.attempts() {
            io.write(&usb_command(USB_STATUS))?;
            let ty = self.wait_for(io, |r| {
                (is_usb_ack(r, USB_STATUS) && r.len() > 3).then(|| r[3])
            })?;
            if ty.is_some() {
                return Ok(ty);
            }
        }
        Ok(None)
    }

    /// Send a subcommand and return its reply data (`None` if it never came).
    fn subcommand(
        &mut self,
        io: &mut dyn ReportIo,
        sub: u8,
        args: &[u8],
    ) -> io::Result<Option<Vec<u8>>> {
        self.subcommand_matching(io, sub, args, |d| Some(d.to_vec()))
    }

    fn subcommand_matching<T>(
        &mut self,
        io: &mut dyn ReportIo,
        sub: u8,
        args: &[u8],
        mut accept: impl FnMut(&[u8]) -> Option<T>,
    ) -> io::Result<Option<T>> {
        for _ in 0..self.attempts() {
            if let Some(t) = self.last_command {
                pause(io, t + self.gap())?;
            }
            let report = subcommand_report(self.next_counter(), &self.rumble, sub, args);
            io.write(&report)?;
            self.last_command = Some(io.now());
            let reply = self.wait_for(io, |r| subcommand_reply(r, sub).and_then(&mut accept))?;
            if reply.is_some() {
                return Ok(reply);
            }
        }
        Ok(None)
    }

    fn spi_read(
        &mut self,
        io: &mut dyn ReportIo,
        addr: u32,
        len: u8,
    ) -> io::Result<Option<Vec<u8>>> {
        self.subcommand_matching(io, SUB_SPI_READ, &spi_read_args(addr, len), |d| {
            spi_data(d, addr, len).map(<[u8]>::to_vec)
        })
    }

    fn read_type(&mut self, io: &mut dyn ReportIo) -> io::Result<()> {
        let byte = if self.link.bluetooth {
            self.subcommand(io, SUB_DEVICE_INFO, &[])?
                .and_then(|d| d.get(2).copied())
        } else {
            self.usb_status(io)?
        };
        if let Some(ty) = byte.and_then(ControllerType::from_byte) {
            // The N64 controller reports itself as a Pro Controller over USB.
            self.ty = if ty == ControllerType::Pro && self.link.pid == 0x2019 {
                ControllerType::N64
            } else {
                ty
            };
        }
        Ok(())
    }

    fn load_stick_calibration(&mut self, io: &mut dyn ReportIo) -> io::Result<()> {
        let user = self.spi_read(io, SPI_USER_STICKS, 22)?;
        let user_left = user
            .as_ref()
            .filter(|u| u[..2] == CAL_MAGIC)
            .map(|u| u[2..11].to_vec());
        let user_right = user
            .as_ref()
            .filter(|u| u[11..13] == CAL_MAGIC)
            .map(|u| u[13..22].to_vec());
        let factory = if user_left.is_none() || user_right.is_none() {
            self.spi_read(io, SPI_FACTORY_STICKS, 18)?
        } else {
            None
        };
        if let Some(l) = user_left.or_else(|| factory.as_ref().map(|f| f[..9].to_vec())) {
            self.cal.left = left_stick_cal(&l);
        }
        if let Some(r) = user_right.or_else(|| factory.as_ref().map(|f| f[9..18].to_vec())) {
            self.cal.right = right_stick_cal(&r);
        }
        Ok(())
    }

    fn load_imu_calibration(&mut self, io: &mut dyn ReportIo) -> io::Result<()> {
        if let Some(factory) = self.spi_read(io, SPI_FACTORY_IMU, 24)? {
            let user = self.spi_read(io, SPI_USER_IMU, 20)?;
            self.cal.imu = ImuCal::from_spi(&factory, user.as_deref());
        }
        Ok(())
    }

    /// Fire-and-forget subcommand while running (no reply wait).
    fn send_subcommand(&mut self, io: &mut dyn ReportIo, sub: u8, args: &[u8]) -> io::Result<()> {
        let report = subcommand_report(self.next_counter(), &self.rumble, sub, args);
        io.write(&report)
    }
}

impl PadDriver for Switch1Driver {
    fn init(&mut self, io: &mut dyn ReportIo) -> Result<Identity, InitError> {
        self.rumble = neutral8();
        if !self.link.bluetooth {
            if !self.usb(io, USB_HANDSHAKE, true)? {
                return Err(InitError::NoResponse);
            }
            // Some clones ignore the speed change, and the right Joy-Con in a grip fails
            // the second handshake; neither is fatal.
            self.usb(io, USB_HIGH_SPEED, true)?;
            self.usb(io, USB_HANDSHAKE, true)?;
            self.usb(io, USB_FORCE, false)?;
        }
        self.read_type(io)?;
        let ty = self.ty;
        tracing::debug!(
            ?ty,
            bluetooth = self.link.bluetooth,
            "Switch controller type"
        );
        self.load_stick_calibration(io)?;
        if ty.has_imu() {
            self.load_imu_calibration(io)?;
            // Licensed pads may refuse; that is not fatal.
            self.subcommand(io, SUB_IMU, &[0x01])?;
        }
        if ty.has_rumble() {
            self.subcommand(io, SUB_VIBRATION, &[0x01])?;
        }
        if self
            .subcommand(io, SUB_REPORT_MODE, &[MODE_FULL])?
            .is_none()
        {
            tracing::debug!("no reply to the full-report request");
        }
        if !self.link.bluetooth {
            self.usb(io, USB_FORCE, false)?;
        }
        self.subcommand(io, SUB_PLAYER_LEDS, &[0x01])?;
        if ty.has_home_led() {
            self.subcommand(io, SUB_HOME_LED, &[0x01, 0xF0, 0xF0, 0x00])?;
        }
        let now = io.now();
        self.last_input = Some(now);
        self.last_keepalive = Some(now);
        Ok(Identity {
            name: ty.name().to_owned(),
            family: ty.family(),
            joycon: ty.joycon(),
            has_motion: ty.has_imu(),
            has_touchpad: false,
            has_rumble: ty.has_rumble(),
            notice: None,
        })
    }

    fn parse(&mut self, report: &[u8], now: Instant) -> Option<PadState> {
        self.now = Some(now);
        if let Some(b) = parse_battery(report) {
            self.battery = Some(b);
        }
        match report.first()? {
            0x30 | 0x31 => {
                self.last_input = Some(now);
                self.mode_lost = false;
                let state = parse_full(self.ty, &mut self.cal, report)?;
                if self.ty.has_imu() {
                    match state.motion {
                        Some(_) => {
                            self.imu_seen = true;
                            self.imu_lost = false;
                        }
                        None => self.imu_lost = self.imu_seen,
                    }
                }
                Some(state)
            }
            0x3F => {
                // Someone switched the pad back to simple reports.
                self.last_input = Some(now);
                self.mode_lost = true;
                None
            }
            _ => {
                self.last_input = Some(now);
                None
            }
        }
    }

    fn battery(&self) -> Option<Battery> {
        self.battery
    }

    fn set_leds(&mut self, leds: Leds) {
        self.player_want = leds.player.map(player_mask);
    }

    fn power_off(&mut self, io: &mut dyn ReportIo) -> io::Result<bool> {
        if !self.link.bluetooth {
            return Ok(false);
        }
        // Set HCI state: disconnect. The controller drops the link and sleeps.
        self.send_subcommand(io, SUB_HCI_STATE, &[0x00])?;
        Ok(true)
    }

    fn service(&mut self, io: &mut dyn ReportIo, rumble: Rumble) -> io::Result<()> {
        let now = io.now();
        if !self.link.bluetooth
            && !self.link.is_grip()
            && self.last_input.is_some_and(|t| now >= t + USB_KEEPALIVE)
            && self.last_keepalive.is_none_or(|t| now >= t + USB_KEEPALIVE)
        {
            io.write(&usb_command(USB_FORCE))?;
            self.last_keepalive = Some(now);
        }
        if self.mode_lost && self.last_mode_fix.is_none_or(|t| now >= t + MODE_RETRY) {
            self.send_subcommand(io, SUB_REPORT_MODE, &[MODE_FULL])?;
            self.last_mode_fix = Some(now);
        }
        if self.imu_lost && self.last_imu_fix.is_none_or(|t| now >= t + IMU_RETRY) {
            self.send_subcommand(io, SUB_IMU, &[0x01])?;
            self.last_imu_fix = Some(now);
        }
        if let Some(mask) = self.player_want.filter(|m| Some(*m) != self.player_sent) {
            self.player_sent = Some(mask);
            self.send_subcommand(io, SUB_PLAYER_LEDS, &[mask])?;
        }
        if self.ty.has_rumble()
            && let Some(r) = self.pacer.poll(now, rumble)
        {
            self.rumble = rumble_data(self.ty, r);
            let report = rumble_report(self.next_counter(), &self.rumble);
            io.write(&report)?;
        }
        Ok(())
    }

    fn next_deadline(&self) -> Option<Instant> {
        let mut next = self.pacer.next_deadline();
        let mut consider = |t: Option<Instant>| {
            if let Some(t) = t {
                next = Some(next.map_or(t, |n| n.min(t)));
            }
        };
        if !self.link.bluetooth && !self.link.is_grip() {
            consider(self.last_input.map(|t| t + USB_KEEPALIVE));
        }
        if self.mode_lost {
            consider(self.last_mode_fix.map(|t| t + MODE_RETRY).or(self.now));
        }
        if self.imu_lost {
            consider(self.last_imu_fix.map(|t| t + IMU_RETRY).or(self.now));
        }
        next
    }

    fn shutdown(&mut self, io: &mut dyn ReportIo) {
        self.rumble = neutral8();
        if self.ty.has_rumble() {
            let report = rumble_report(self.next_counter(), &self.rumble);
            let _ = io.write(&report);
        }
        // Wired pads stop reporting in simple mode, so only Bluetooth goes back to it.
        if self.link.bluetooth && self.initial_mode.is_none_or(|m| m == MODE_SIMPLE) {
            let _ = self.send_subcommand(io, SUB_REPORT_MODE, &[MODE_SIMPLE]);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use dp_core::pad::PadButton;

    use super::*;
    use crate::padio::testing::ScriptIo;

    const MS: Duration = Duration::from_millis(1);

    /// A scripted Switch controller of type `ty` with the given flash contents.
    fn fake_pad(ty: u8, spi: HashMap<u32, Vec<u8>>) -> impl FnMut(&[u8]) -> Vec<Vec<u8>> {
        move |w: &[u8]| match w[0] {
            0x80 => match w[1] {
                0x01 => vec![vec![0x81, 0x01, 0x00, ty, 1, 2, 3, 4, 5, 6]],
                0x04 => Vec::new(),
                cmd => vec![vec![0x81, cmd]],
            },
            0x01 => {
                let sub = w[10];
                let data: Vec<u8> = match sub {
                    0x02 => vec![0x04, 0x33, ty, 0x02, 1, 2, 3, 4, 5, 6, 0x01, 0x01],
                    0x10 => {
                        let addr = u32::from_le_bytes([w[11], w[12], w[13], w[14]]);
                        let len = w[15];
                        let Some(bytes) = spi.get(&addr) else {
                            return Vec::new();
                        };
                        let mut d = w[11..16].to_vec();
                        d.extend_from_slice(&bytes[..len as usize]);
                        d
                    }
                    _ => Vec::new(),
                };
                let mut r = vec![0u8; 49];
                r[0] = 0x21;
                r[13] = 0x80 | if sub == 0x10 { 0x10 } else { 0x00 };
                r[14] = sub;
                r[15..15 + data.len()].copy_from_slice(&data);
                vec![r]
            }
            _ => Vec::new(),
        }
    }

    fn link(bluetooth: bool, pid: u16) -> Link {
        Link {
            bluetooth,
            vid: NINTENDO_VID,
            pid,
            grip_slot: None,
        }
    }

    /// Subcommand IDs (or 0x80xx for USB commands) in write order.
    fn sequence(io: &ScriptIo) -> Vec<u16> {
        io.writes()
            .iter()
            .map(|w| match w[0] {
                0x80 => 0x8000 | w[1] as u16,
                0x01 => w[10] as u16,
                id => 0x1000 | id as u16,
            })
            .collect()
    }

    fn pack(a: u16, b: u16) -> [u8; 3] {
        [
            a as u8,
            ((a >> 8) as u8 & 0x0F) | ((b as u8 & 0x0F) << 4),
            (b >> 4) as u8,
        ]
    }

    #[test]
    fn usb_init_sequence() {
        let mut io = ScriptIo::new(fake_pad(0x03, HashMap::new()));
        let mut d = Switch1Driver::new(link(false, 0x2009));
        let id = d.init(&mut io).unwrap();
        assert_eq!(id.name, "Switch Pro Controller");
        assert_eq!(id.family, PadFamily::SwitchPro);
        assert!(id.has_motion && id.has_rumble && id.joycon.is_none());
        let seq = sequence(&io);
        assert_eq!(&seq[..5], &[0x8002, 0x8003, 0x8002, 0x8004, 0x8001]);
        let pos = |s: u16| seq.iter().position(|&x| x == s).unwrap();
        assert!(pos(0x40) < pos(0x48) && pos(0x48) < pos(0x03) && pos(0x03) < pos(0x30));
        assert!(seq.contains(&0x38), "Pro Controller has a Home LED");
        assert_eq!(seq.iter().filter(|&&s| s == 0x8004).count(), 2);
    }

    #[test]
    fn bluetooth_init_asks_device_info() {
        let mut io = ScriptIo::new(fake_pad(0x02, HashMap::new()));
        let mut d = Switch1Driver::new(link(true, 0x2009));
        let id = d.init(&mut io).unwrap();
        assert_eq!(id.name, "Joy-Con (R)", "type byte beats the PID");
        assert_eq!(id.joycon, Some(JoyConSide::Right));
        let seq = sequence(&io);
        assert_eq!(seq[0], 0x02);
        assert!(
            !seq.iter().any(|s| s & 0x8000 != 0),
            "no USB commands over Bluetooth"
        );
        // Subcommands are spaced at least 60 ms apart over Bluetooth.
        let times: Vec<Duration> = io
            .written
            .iter()
            .filter(|(_, w)| w[0] == 0x01)
            .map(|(t, _)| *t)
            .collect();
        assert!(times.windows(2).all(|w| w[1] - w[0] >= 60 * MS));
    }

    #[test]
    fn usb_without_answer_fails() {
        let mut io = ScriptIo::silent();
        let mut d = Switch1Driver::new(link(false, 0x2009));
        assert!(matches!(d.init(&mut io), Err(InitError::NoResponse)));
    }

    #[test]
    fn user_calibration_wins_over_factory() {
        let mut user = vec![0u8; 22];
        user[..2].copy_from_slice(&CAL_MAGIC);
        // Left: max above (1000, 1000), centre (2000, 2000), min below (1000, 1000).
        user[2..5].copy_from_slice(&pack(1000, 1000));
        user[5..8].copy_from_slice(&pack(2000, 2000));
        user[8..11].copy_from_slice(&pack(1000, 1000));
        let mut spi = HashMap::new();
        spi.insert(SPI_USER_STICKS, user);
        let mut factory = vec![0u8; 18];
        factory[9..12].copy_from_slice(&pack(1800, 1900)); // right centre
        factory[12..15].copy_from_slice(&pack(500, 500)); // right min below
        factory[15..18].copy_from_slice(&pack(600, 600)); // right max above
        spi.insert(SPI_FACTORY_STICKS, factory);
        let mut io = ScriptIo::new(fake_pad(0x03, spi));
        let mut d = Switch1Driver::new(link(true, 0x2009));
        d.init(&mut io).unwrap();
        assert_eq!(d.cal.left.x.center, 2000.0);
        assert!((d.cal.left.x.above - 700.0).abs() < 1e-3);
        // The right stick had no user calibration, so the factory block applies.
        assert_eq!(d.cal.right.x.center, 1800.0);
        assert_eq!(d.cal.right.y.center, 1900.0);
        assert!((d.cal.right.x.above - 420.0).abs() < 1e-3);
        assert!((d.cal.right.x.below - 350.0).abs() < 1e-3);
    }

    #[test]
    fn missing_flash_uses_defaults() {
        let mut io = ScriptIo::new(fake_pad(0x03, HashMap::new()));
        let mut d = Switch1Driver::new(link(true, 0x2009));
        d.init(&mut io).unwrap();
        assert_eq!(d.cal.left.x.center, 2048.0);
        assert_eq!(d.cal.imu, ImuCal::DEFAULT);
    }

    #[test]
    fn counter_wraps() {
        let mut io = ScriptIo::new(fake_pad(0x03, HashMap::new()));
        let mut d = Switch1Driver::new(link(true, 0x2009));
        d.init(&mut io).unwrap();
        let counters: Vec<u8> = io.writes().iter().map(|w| w[1]).collect();
        for (i, c) in counters.iter().enumerate() {
            assert_eq!(*c as usize, i % 16);
        }
        assert!(counters.len() > 16);
    }

    #[test]
    fn classic_pads_skip_motion() {
        let mut io = ScriptIo::new(fake_pad(0x0B, HashMap::new()));
        let mut d = Switch1Driver::new(link(true, 0x2017));
        let id = d.init(&mut io).unwrap();
        assert_eq!(id.name, "SNES Controller");
        assert!(!id.has_motion && !id.has_rumble);
        assert!(!sequence(&io).contains(&0x40));
    }

    fn full_report() -> Vec<u8> {
        let mut r = vec![0u8; 49];
        r[0] = 0x30;
        r[6..9].copy_from_slice(&pack(2048, 2048));
        r[9..12].copy_from_slice(&pack(2048, 2048));
        r
    }

    #[test]
    fn parse_buttons_sticks_and_motion() {
        let mut cal = Calibration::default();
        let mut r = full_report();
        r[3] = 0x08; // A
        r[5] = 0x80; // ZL
        r[6..9].copy_from_slice(&pack(4095, 2048)); // left stick right
        // All frames: accel z = +1 g (device up), gyro y = +14.2842 raw/°/s × 10.
        for f in 0..3 {
            let at = 13 + f * 12;
            r[at + 4..at + 6].copy_from_slice(&4096i16.to_le_bytes());
            r[at + 8..at + 10].copy_from_slice(&143i16.to_le_bytes());
        }
        let s = parse_full(ControllerType::Pro, &mut cal, &r).unwrap();
        assert!(s.pressed(PadButton::B), "Nintendo A is east");
        assert_eq!(s.axis(Axis::LeftTrigger), 1.0);
        assert_eq!(s.axis(Axis::LeftX), 1.0);
        assert_eq!(s.axis(Axis::LeftY), 0.0);
        let m = s.motion.unwrap();
        assert!(
            (m.accel[1] - 1.0).abs() < 1e-3,
            "gravity on our y: {:?}",
            m.accel
        );
        assert!(
            (m.gyro[0] + 10.0).abs() < 0.05,
            "device y is our -x: {:?}",
            m.gyro
        );

        let s = parse_full(ControllerType::JoyConRight, &mut cal, &r).unwrap();
        let m = s.motion.unwrap();
        assert!((m.accel[1] + 1.0).abs() < 1e-3, "right Joy-Con is mirrored");
        assert!((m.gyro[0] - 10.0).abs() < 0.05);
    }

    #[test]
    fn idle_imu_gives_no_motion() {
        let s = parse_full(
            ControllerType::Pro,
            &mut Calibration::default(),
            &full_report(),
        );
        assert!(s.unwrap().motion.is_none());
    }

    #[test]
    fn imu_calibration_from_flash() {
        let mut f = Vec::new();
        for v in [
            0i16, 0, 0, 16384, 16384, 8192, 10, 20, 30, 13381, 13391, 13401,
        ] {
            f.extend_from_slice(&v.to_le_bytes());
        }
        let cal = ImuCal::from_spi(&f, None);
        assert!((cal.accel_g([4096, 4096, 2048])[0] - 1.0).abs() < 1e-4);
        assert!((cal.accel_g([4096, 4096, 2048])[2] - 1.0).abs() < 1e-4);
        assert_eq!(cal.gyro_dps([10, 20, 30]), [0.0, 0.0, 0.0]);
        let g = cal.gyro_dps([10 + 13371, 20, 30])[0];
        assert!((g - 936.0).abs() < 1e-2, "{g}");
        // Erased flash keeps the defaults.
        assert_eq!(ImuCal::from_spi(&[0xFF; 24], None), ImuCal::DEFAULT);
    }

    #[test]
    fn rumble_encoding() {
        assert_eq!(hd_rumble(0.0), NEUTRAL_RUMBLE);
        let full = hd_rumble(1.0);
        assert_eq!(full, [0x00, 0xC9, 0x40, 0x72]);
        let mut last = 0u8;
        for i in 1..=20 {
            let a = hd_rumble(i as f32 / 20.0);
            let index = a[1] >> 1;
            assert!(index >= last, "amplitude rises monotonically");
            last = index;
            // The low band carries the same index: bit 7 of b2 is its low bit.
            assert_eq!(((a[3] - 0x40) << 1) | (a[2] >> 7), index);
        }
        let jc = rumble_data(
            ControllerType::JoyConLeft,
            Rumble {
                low: 0.0,
                high: 1.0,
            },
        );
        assert_eq!(jc[..4], full);
        let pro = rumble_data(
            ControllerType::Pro,
            Rumble {
                low: 1.0,
                high: 0.0,
            },
        );
        assert_eq!(pro[..4], full);
        assert_eq!(pro[4..], NEUTRAL_RUMBLE);
    }

    fn ready_driver(bluetooth: bool) -> (Switch1Driver, ScriptIo) {
        let mut io = ScriptIo::new(fake_pad(0x03, HashMap::new()));
        let mut d = Switch1Driver::new(link(bluetooth, 0x2009));
        d.init(&mut io).unwrap();
        io.written.clear();
        (d, io)
    }

    #[test]
    fn rumble_is_paced() {
        let (mut d, mut io) = ready_driver(true);
        let on = Rumble {
            low: 1.0,
            high: 0.5,
        };
        d.service(&mut io, on).unwrap();
        assert_eq!(io.writes().len(), 1);
        assert_eq!(io.writes()[0][0], 0x10);
        io.advance(10 * MS);
        d.service(
            &mut io,
            Rumble {
                low: 0.2,
                high: 0.2,
            },
        )
        .unwrap();
        assert_eq!(io.writes().len(), 1, "too soon after the last write");
        io.advance(25 * MS);
        d.service(
            &mut io,
            Rumble {
                low: 0.2,
                high: 0.2,
            },
        )
        .unwrap();
        assert_eq!(io.writes().len(), 2);
        io.advance(55 * MS);
        d.service(
            &mut io,
            Rumble {
                low: 0.2,
                high: 0.2,
            },
        )
        .unwrap();
        assert_eq!(io.writes().len(), 3, "refreshed after 50 ms");
    }

    #[test]
    fn usb_keepalive_after_silence() {
        let (mut d, mut io) = ready_driver(false);
        let t = io.now();
        d.parse(&full_report(), t);
        io.advance(50 * MS);
        d.service(&mut io, Rumble::default()).unwrap();
        assert!(io.writes().is_empty());
        io.advance(60 * MS);
        d.service(&mut io, Rumble::default()).unwrap();
        assert_eq!(io.writes(), vec![&[0x80u8, 0x04][..]]);
        assert!(d.next_deadline().is_some());

        let (mut d, mut io) = ready_driver(true);
        io.advance(Duration::from_secs(1));
        d.service(&mut io, Rumble::default()).unwrap();
        assert!(io.writes().is_empty(), "no keep-alive over Bluetooth");
    }

    #[test]
    fn simple_mode_is_undone() {
        let (mut d, mut io) = ready_driver(true);
        assert_eq!(d.parse(&[0x3F, 0, 0, 8], io.now()), None);
        d.service(&mut io, Rumble::default()).unwrap();
        let w = io.writes();
        assert_eq!((w[0][0], w[0][10], w[0][11]), (0x01, 0x03, 0x30));
        io.advance(100 * MS);
        d.service(&mut io, Rumble::default()).unwrap();
        assert_eq!(io.writes().len(), 1, "retried at most once a second");
        d.parse(&full_report(), io.now());
        io.advance(Duration::from_secs(2));
        d.service(&mut io, Rumble::default()).unwrap();
        assert_eq!(io.writes().len(), 1, "full reports are back");
    }

    #[test]
    fn shutdown_restores_simple_mode_over_bluetooth_only() {
        let mut io = ScriptIo::new(fake_pad(0x03, HashMap::new()));
        io.push_input(Duration::ZERO, vec![0x3F, 0, 0, 8]);
        let mut d = Switch1Driver::new(link(true, 0x2009));
        d.init(&mut io).unwrap();
        io.written.clear();
        d.shutdown(&mut io);
        assert!(
            io.writes()
                .iter()
                .any(|w| w[0] == 0x01 && w[10] == 0x03 && w[11] == 0x3F)
        );

        let (mut d, mut io) = ready_driver(false);
        d.shutdown(&mut io);
        assert!(!io.writes().iter().any(|w| w[0] == 0x01 && w[10] == 0x03));
    }

    #[test]
    fn power_off_only_over_bluetooth() {
        let off = |w: &[u8]| w[0] == 0x01 && w[10] == SUB_HCI_STATE && w[11] == 0x00;
        let (mut d, mut io) = ready_driver(true);
        assert!(d.power_off(&mut io).unwrap());
        assert!(io.writes().iter().any(|w| off(w)));
        let (mut d, mut io) = ready_driver(false);
        assert!(!d.power_off(&mut io).unwrap());
        assert!(!io.writes().iter().any(|w| off(w)));
    }

    #[test]
    fn fallback_types() {
        assert_eq!(
            ControllerType::fallback(NINTENDO_VID, 0x2006, None),
            ControllerType::JoyConLeft
        );
        assert_eq!(
            ControllerType::fallback(NINTENDO_VID, 0x200E, Some(JoyConSide::Left)),
            ControllerType::JoyConLeft
        );
        assert_eq!(
            ControllerType::fallback(0x0E6F, 0x0186, None),
            ControllerType::LicensedPro
        );
        assert_eq!(ControllerType::from_byte(0x0C), Some(ControllerType::N64));
        assert_eq!(ControllerType::from_byte(0x00), None);
    }

    #[test]
    fn battery_levels_and_charging() {
        let report = |b: u8| {
            let mut r = vec![0u8; 49];
            r[0] = 0x30;
            r[2] = b;
            r
        };
        let b = parse_battery(&report(0x8E)).unwrap(); // full, connection bits set
        assert_eq!((b.level, b.charging), (100, false));
        let b = parse_battery(&report(0x50)).unwrap(); // low, charging
        assert_eq!((b.level, b.charging), (30, true));
        assert_eq!(parse_battery(&report(0x20)).unwrap().level, 10); // critical
        assert_eq!(parse_battery(&[0x3F, 0, 0x80]), None);
    }
}
