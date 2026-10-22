//! First-generation Switch protocol: Pro Controller, Joy-Con, charging grip and the
//! Nintendo Switch Online pads, over USB and Bluetooth.
//!
//! Output report 0x01 carries rumble plus a subcommand, 0x10 rumble only; the pad answers
//! subcommands with input report 0x21 and streams its state as 0x30 once switched to full
//! mode. Over USB a short handshake (proprietary report 0x80) must come first.

mod calibration;
mod protocol;
mod rumble;
#[cfg(test)]
mod tests;

use std::io;
use std::time::{Duration, Instant};

use dp_core::pad::{PadState, Rumble};

use self::calibration::{Calibration, ImuCal, left_stick_cal, right_stick_cal};
use self::protocol::{
    is_usb_ack, parse_battery, parse_full, rumble_report, spi_data, spi_read_args,
    subcommand_reply, subcommand_report, usb_command,
};
use self::rumble::{NEUTRAL_RUMBLE8, rumble_data};
use super::{CAL_MAGIC, FIRST_PLAYER, NINTENDO_VID, PlayerLights, pid};
use crate::padio::{
    Every, Identity, InitError, PadDriver, ReportIo, RumblePacer, Seq4, earliest, pause, read_until,
};
use crate::{Battery, JoyConSide, Leds, PadFamily};

/// Rumble data for "no vibration" (320 Hz / 160 Hz at amplitude 0).
const NEUTRAL_RUMBLE: [u8; 4] = [0x00, 0x01, 0x40, 0x40];

const REPLY_TIMEOUT: Duration = Duration::from_millis(100);
/// Without input for this long a USB pad has been reset (e.g. by Steam): force USB again.
const USB_KEEPALIVE: Duration = Duration::from_millis(100);
/// Still silent this long despite the keep-alives: another program reset the pad or set
/// simple reports (which a wired pad answers with silence), so start it again.
const USB_RESTART: Duration = Duration::from_secs(1);
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

/// Controller type byte (device-info reply / USB status reply).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ControllerType {
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
    fn from_byte(b: u8) -> Option<Self> {
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
            (NINTENDO_VID, pid::JOYCON_L) => ControllerType::JoyConLeft,
            (NINTENDO_VID, pid::CHARGING_GRIP) if grip_slot == Some(JoyConSide::Left) => {
                ControllerType::JoyConLeft
            }
            (NINTENDO_VID, pid::JOYCON_R | pid::CHARGING_GRIP) => ControllerType::JoyConRight,
            (NINTENDO_VID, pid::SNES) => ControllerType::Snes,
            (NINTENDO_VID, pid::N64) => ControllerType::N64,
            (NINTENDO_VID, pid::GENESIS) => ControllerType::Genesis,
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

    fn has_imu(self) -> bool {
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
        (self.vid, self.pid) == (NINTENDO_VID, pid::CHARGING_GRIP)
    }
}

pub struct Switch1Driver {
    link: Link,
    ty: ControllerType,
    cal: Calibration,
    counter: Seq4,
    rumble: [u8; 8],
    pacer: RumblePacer,
    last_command: Option<Instant>,
    /// First input report ID seen, so shutdown can restore it for other programs.
    initial_mode: Option<u8>,
    last_input: Option<Instant>,
    last_keepalive: Option<Instant>,
    last_restart: Option<Instant>,
    /// Started again after a silence and no full report since.
    stalled: bool,
    mode_lost: bool,
    mode_fix: Every,
    imu_seen: bool,
    imu_lost: bool,
    imu_fix: Every,
    now: Option<Instant>,
    battery: Option<Battery>,
    lights: PlayerLights,
}

impl Switch1Driver {
    pub fn new(link: Link) -> Self {
        Switch1Driver {
            link,
            ty: ControllerType::fallback(link.vid, link.pid, link.grip_slot),
            cal: Calibration::default(),
            counter: Seq4::default(),
            rumble: NEUTRAL_RUMBLE8,
            pacer: RumblePacer::new(
                Duration::from_millis(30),
                Some(Duration::from_millis(50)),
                3,
            ),
            last_command: None,
            initial_mode: None,
            last_input: None,
            last_keepalive: None,
            last_restart: None,
            stalled: false,
            mode_lost: false,
            mode_fix: Every::new(MODE_RETRY),
            imu_seen: false,
            battery: None,
            lights: PlayerLights::new(),
            imu_lost: false,
            imu_fix: Every::new(IMU_RETRY),
            now: None,
        }
    }

    fn attempts(&self) -> usize {
        if self.link.is_grip() { 20 } else { 5 }
    }

    /// Minimum spacing between subcommands.
    fn gap(&self) -> Duration {
        Duration::from_millis(if self.link.bluetooth { 60 } else { 20 })
    }

    /// Wired pads other than the charging grip need keep-alives and, when silent, a restart.
    fn usb_watchdog(&self) -> bool {
        !self.link.bluetooth && !self.link.is_grip()
    }

    /// Read reports until `want` accepts one or the reply timeout passes.
    fn wait_for<T>(
        &mut self,
        io: &mut dyn ReportIo,
        mut want: impl FnMut(&[u8]) -> Option<T>,
    ) -> io::Result<Option<T>> {
        let until = io.now() + REPLY_TIMEOUT;
        let initial_mode = &mut self.initial_mode;
        read_until(io, until, |r| {
            if initial_mode.is_none() && matches!(r.first(), Some(0x30 | 0x31 | 0x3F)) {
                *initial_mode = Some(r[0]);
            }
            want(r)
        })
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
            let report = subcommand_report(self.counter.step(), &self.rumble, sub, args);
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
            self.ty = if ty == ControllerType::Pro && self.link.pid == pid::N64 {
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
        let report = subcommand_report(self.counter.step(), &self.rumble, sub, args);
        io.write(&report)
    }

    /// The USB handshake; `false` if the pad never acknowledges it.
    fn usb_handshake(&mut self, io: &mut dyn ReportIo) -> io::Result<bool> {
        if !self.usb(io, USB_HANDSHAKE, true)? {
            return Ok(false);
        }
        // Some clones ignore the speed change, and the right Joy-Con in a grip fails
        // the second handshake; neither is fatal.
        self.usb(io, USB_HIGH_SPEED, true)?;
        self.usb(io, USB_HANDSHAKE, true)?;
        self.usb(io, USB_FORCE, false)?;
        Ok(true)
    }

    /// Switch on motion and rumble where the pad has them, then full reports.
    fn start_reports(&mut self, io: &mut dyn ReportIo) -> io::Result<()> {
        if self.ty.has_imu() {
            // Licensed pads may refuse; that is not fatal.
            self.subcommand(io, SUB_IMU, &[0x01])?;
        }
        if self.ty.has_rumble() {
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
        Ok(())
    }

    /// Start a wired pad again that stays silent despite the keep-alives.
    fn restart_usb(&mut self, io: &mut dyn ReportIo) -> io::Result<()> {
        if !self.stalled {
            self.stalled = true;
            tracing::info!(ty = ?self.ty, "Switch controller silent over USB, starting it again");
        }
        self.last_restart = Some(io.now());
        if self.usb_handshake(io)? {
            self.start_reports(io)?;
            // A reset also put out the player lights.
            let mask = self.lights.wanted_or_first();
            self.subcommand(io, SUB_PLAYER_LEDS, &[mask])?;
            self.lights.mark_sent(mask);
        }
        Ok(())
    }
}

impl PadDriver for Switch1Driver {
    fn init(&mut self, io: &mut dyn ReportIo) -> Result<Identity, InitError> {
        self.rumble = NEUTRAL_RUMBLE8;
        if !self.link.bluetooth && !self.usb_handshake(io)? {
            return Err(InitError::NoResponse);
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
        }
        self.start_reports(io)?;
        self.subcommand(io, SUB_PLAYER_LEDS, &[FIRST_PLAYER])?;
        if ty.has_home_led() {
            self.subcommand(io, SUB_HOME_LED, &[0x01, 0xF0, 0xF0, 0x00])?;
        }
        let now = io.now();
        self.last_input = Some(now);
        self.last_keepalive = Some(now);
        Ok(Identity {
            joycon: ty.joycon(),
            has_motion: ty.has_imu(),
            has_rumble: ty.has_rumble(),
            ..Identity::new(ty.name(), ty.family())
        })
    }

    fn parse(&mut self, report: &[u8], now: Instant) -> Option<PadState> {
        self.now = Some(now);
        if let Some(b) = parse_battery(report) {
            self.battery = Some(b);
        }
        let id = *report.first()?;
        self.last_input = Some(now);
        match id {
            0x30 | 0x31 => {
                self.mode_lost = false;
                if self.stalled {
                    self.stalled = false;
                    tracing::info!(ty = ?self.ty, "Switch controller reporting again");
                }
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
                self.mode_lost = true;
                None
            }
            _ => None,
        }
    }

    fn battery(&self) -> Option<Battery> {
        self.battery
    }

    fn set_leds(&mut self, leds: Leds) {
        self.lights.set(leds.player);
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
        if self.usb_watchdog()
            && self.last_input.is_some_and(|t| now >= t + USB_KEEPALIVE)
            && self.last_keepalive.is_none_or(|t| now >= t + USB_KEEPALIVE)
        {
            io.write(&usb_command(USB_FORCE))?;
            self.last_keepalive = Some(now);
        }
        if self.usb_watchdog()
            && self
                .last_input
                .max(self.last_restart)
                .is_some_and(|t| now >= t + USB_RESTART)
        {
            self.restart_usb(io)?;
        }
        if self.mode_lost && self.mode_fix.due(now) {
            self.send_subcommand(io, SUB_REPORT_MODE, &[MODE_FULL])?;
            self.mode_fix.ran(now);
        }
        if self.imu_lost && self.imu_fix.due(now) {
            self.send_subcommand(io, SUB_IMU, &[0x01])?;
            self.imu_fix.ran(now);
        }
        if let Some(mask) = self.lights.take_change() {
            self.send_subcommand(io, SUB_PLAYER_LEDS, &[mask])?;
        }
        if self.ty.has_rumble()
            && let Some(r) = self.pacer.poll(now, rumble)
        {
            self.rumble = rumble_data(self.ty, r);
            let report = rumble_report(self.counter.step(), &self.rumble);
            io.write(&report)?;
        }
        Ok(())
    }

    fn next_deadline(&self) -> Option<Instant> {
        // Counted from the last attempt too, so a silent pad does not make the caller spin
        // on a deadline in the past.
        let watch = |attempt: Option<Instant>, wait: Duration| {
            self.usb_watchdog()
                .then(|| self.last_input.max(attempt).map(|t| t + wait))
                .flatten()
        };
        let retry = |lost: bool, fix: &Every| lost.then(|| fix.next().or(self.now)).flatten();
        earliest([
            self.pacer.next_deadline(),
            watch(self.last_keepalive, USB_KEEPALIVE),
            watch(self.last_restart, USB_RESTART),
            retry(self.mode_lost, &self.mode_fix),
            retry(self.imu_lost, &self.imu_fix),
        ])
    }

    fn shutdown(&mut self, io: &mut dyn ReportIo) {
        self.rumble = NEUTRAL_RUMBLE8;
        if self.ty.has_rumble() {
            let report = rumble_report(self.counter.step(), &self.rumble);
            let _ = io.write(&report);
        }
        // Wired pads stop reporting in simple mode, so only Bluetooth goes back to it.
        if self.link.bluetooth && self.initial_mode.is_none_or(|m| m == MODE_SIMPLE) {
            let _ = self.send_subcommand(io, SUB_REPORT_MODE, &[MODE_SIMPLE]);
        }
    }
}
