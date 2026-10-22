//! Switch 2 controllers over USB: Pro Controller 2, GameCube controller, Joy-Con 2.
//!
//! The pads are composite USB devices. Interface 0 is an ordinary HID game pad; interface 1
//! is a vendor interface with two bulk endpoints that takes 8-byte-header commands. The
//! HID interface stays silent until the vendor interface has been told to start output,
//! after which it streams input report 0x05 at about 250 Hz. Rumble goes out as HID
//! output reports.

use std::io;
use std::time::{Duration, Instant};

use dp_core::pad::{Axis, Motion, PadButton, PadState, Rumble};

use super::{StickCal, Switch2Model, apply_buttons, gc_trigger};
use crate::nintendo::player_mask;
use crate::padio::{BulkIo, Identity, InitError, LateBulk, PadDriver, ReportIo, RumblePacer};
use crate::report::{le_f32, le_i16, le_u32, unpack12};
use crate::{JoyConSide, Leds, PadFamily};

/// Feature mask: buttons, sticks, IMU and rumble.
pub const FEATURES: u8 = 0x27;
pub const INPUT_REPORT: u8 = 0x05;

const COMMAND_TIMEOUT: Duration = Duration::from_millis(200);
/// How long to wait for input when another program started the pad.
const FOREIGN_INIT_WAIT: Duration = Duration::from_millis(1500);
const CAL_MAGIC: [u8; 2] = [0xB2, 0xA1];
/// Full strength is capped below the actuators' maximum, as the console does.
const RUMBLE_MAX: f32 = 29000.0;
const HIGH_FREQ: u16 = 0x187;
const LOW_FREQ: u16 = 0x112;

/// One vendor command: `[cmd, 0x91 (host to device), 0x00 (USB), sub, 0, len, 0, 0] + payload`.
pub fn command(cmd: u8, sub: u8, payload: &[u8]) -> Vec<u8> {
    let mut c = vec![cmd, 0x91, 0x00, sub, 0x00, payload.len() as u8, 0x00, 0x00];
    c.extend_from_slice(payload);
    c
}

/// The start-up sequence the console sends. Only the last command ("start output") is
/// strictly needed; the second-to-last selects input report 0x05.
pub fn init_commands(features: u8) -> Vec<Vec<u8>> {
    let mut vibration = vec![0x01];
    vibration.extend_from_slice(&[0xFF; 8]);
    vibration.extend_from_slice(&[0x35, 0x00, 0x46, 0x00]);
    vibration.resize(20, 0);
    vec![
        command(0x07, 0x01, &[]),
        command(0x0C, 0x02, &[features, 0, 0, 0]),
        command(0x11, 0x01, &[]),
        command(0x0A, 0x08, &vibration),
        command(0x0C, 0x04, &[features, 0, 0, 0]),
        command(0x01, 0x0C, &[]),
        command(0x01, 0x01, &[]),
        // Enables GL/GR on the charging grip.
        command(0x08, 0x02, &[0x01, 0, 0, 0]),
        select_report_command(),
        start_output_command(),
    ]
}

pub fn select_report_command() -> Vec<u8> {
    command(0x03, 0x0A, &[INPUT_REPORT, 0, 0, 0])
}

/// The trailing six bytes are a host address; any value works.
pub fn start_output_command() -> Vec<u8> {
    command(
        0x03,
        0x0D,
        &[0x01, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
    )
}

/// Read a 0x40-byte flash block.
pub fn flash_read_command(addr: u32) -> Vec<u8> {
    let a = addr.to_le_bytes();
    command(0x02, 0x01, &[0, 0, 0, 0, a[0], a[1], a[2], a[3]])
}

/// Length of a flash-read reply; the block starts at offset 0x10.
pub const FLASH_REPLY_LEN: usize = 0x50;

pub fn flash_block(reply: &[u8]) -> Option<&[u8]> {
    reply.get(0x10..FLASH_REPLY_LEN)
}

/// Player LEDs: bits 0..3 are LEDs 1..4.
pub fn player_led_command(mask: u8) -> Vec<u8> {
    command(0x09, 0x07, &[mask, 0, 0, 0, 0, 0, 0, 0])
}

/// Nine bytes: neutral x/y, distance to the maximum x/y, distance to the minimum x/y.
pub fn stick_cal(b: &[u8]) -> StickCal {
    let (neutral, max, min) = (unpack12(&b[0..3]), unpack12(&b[3..6]), unpack12(&b[6..9]));
    StickCal::fixed(neutral, max, min)
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Calibration {
    pub left: StickCal,
    pub right: StickCal,
    /// Gyro zero-rate bias in degrees per second, device axes.
    pub gyro_bias: [f32; 3],
    /// Raw GameCube trigger values at rest (left, right).
    pub trigger_zero: [u8; 2],
}

impl Default for Calibration {
    fn default() -> Self {
        Calibration {
            left: StickCal::DEFAULT,
            right: StickCal::DEFAULT,
            gyro_bias: [0.0; 3],
            trigger_zero: [0; 2],
        }
    }
}

fn apply_gamecube(s: &mut PadState, r: &[u8], cal: &Calibration) {
    let (face, mid, left) = (r[5], r[6], r[7]);
    // Mapped by the pad's geometry: big A south, B left of it, X right, Y on top.
    s.set(PadButton::Y, face & 0x01 != 0);
    s.set(PadButton::B, face & 0x02 != 0); // X
    s.set(PadButton::X, face & 0x04 != 0); // B
    s.set(PadButton::A, face & 0x08 != 0);
    s.set(PadButton::RB, face & 0x80 != 0); // Z
    s.set(PadButton::Start, mid & 0x02 != 0);
    s.set(PadButton::Guide, mid & 0x10 != 0);
    s.set(PadButton::Misc, mid & 0x20 != 0);
    s.set(PadButton::Misc2, mid & 0x40 != 0);
    s.set(PadButton::DpadDown, left & 0x01 != 0);
    s.set(PadButton::DpadUp, left & 0x02 != 0);
    s.set(PadButton::DpadRight, left & 0x04 != 0);
    s.set(PadButton::DpadLeft, left & 0x08 != 0);
    s.set(PadButton::LB, left & 0x80 != 0); // ZL
    s.set_axis(Axis::LeftTrigger, gc_trigger(r[0x3D], cal.trigger_zero[0]));
    s.set_axis(Axis::RightTrigger, gc_trigger(r[0x3E], cal.trigger_zero[1]));
}

/// IMU timestamp of an input report (0 while the IMU is off).
pub fn imu_timestamp(r: &[u8]) -> u32 {
    le_u32(r, 0x2B)
}

/// Parse input report 0x05. `gyro_scale` is degrees per second per raw unit; motion is
/// left out while it is unknown.
pub fn parse_report(
    model: Switch2Model,
    cal: &mut Calibration,
    gyro_scale: Option<f32>,
    r: &[u8],
) -> Option<PadState> {
    if r.len() < 0x3F || r[0] != INPUT_REPORT {
        return None;
    }
    let mut s = PadState::default();
    if model == Switch2Model::GameCube {
        apply_gamecube(&mut s, r, cal);
    } else {
        apply_buttons(&mut s, r[5], r[6], r[7]);
        if r[8] & 0x01 != 0 {
            s.set(PadButton::PaddleUR, true); // GR
        }
        if r[8] & 0x02 != 0 {
            s.set(PadButton::PaddleUL, true); // GL
        }
    }
    let (lx, ly) = cal.left.normalize(unpack12(&r[11..14]));
    let (rx, ry) = cal.right.normalize(unpack12(&r[14..17]));
    s.set_axis(Axis::LeftX, lx);
    s.set_axis(Axis::LeftY, ly);
    s.set_axis(Axis::RightX, rx);
    s.set_axis(Axis::RightY, ry);
    if let Some(scale) = gyro_scale
        && imu_timestamp(r) != 0
    {
        let v = |at: usize| f32::from(le_i16(r, at));
        let a = [v(0x31), v(0x33), v(0x35)].map(|v| v / 4096.0);
        let g: [f32; 3] = std::array::from_fn(|i| v(0x37 + 2 * i) * scale - cal.gyro_bias[i]);
        // Device axes to ours: x stays, z is up (our y), y points backward (our -z).
        s.motion = Some(Motion {
            gyro: [g[0], g[2], -g[1]],
            accel: [a[0], a[2], -a[1]],
        });
    }
    Some(s)
}

/// One HD rumble sample: four packed 10-bit little-endian fields (high frequency, high
/// amplitude, low frequency, low amplitude); amplitudes are 16-bit values shifted by 6.
pub fn hd_sample(high_freq: u16, high_amp: u16, low_freq: u16, low_amp: u16) -> [u8; 5] {
    let bits = (u64::from(high_freq) & 0x3FF)
        | ((u64::from(high_amp) >> 6) & 0x3FF) << 10
        | (u64::from(low_freq) & 0x3FF) << 20
        | ((u64::from(low_amp) >> 6) & 0x3FF) << 30;
    let b = bits.to_le_bytes();
    [b[0], b[1], b[2], b[3], b[4]]
}

fn report_id(model: Switch2Model) -> u8 {
    match model {
        Switch2Model::JoyConLeft | Switch2Model::JoyConRight => 0x01,
        Switch2Model::Pro => 0x02,
        Switch2Model::GameCube => 0x03,
    }
}

/// Rumble output report for the HD-rumble pads (everything but the GameCube controller).
pub fn rumble_report(model: Switch2Model, seq: u8, r: Rumble) -> [u8; 64] {
    let amp = |v: f32| (v.clamp(0.0, 1.0) * RUMBLE_MAX) as u16;
    let mut out = [0u8; 64];
    out[0] = report_id(model);
    out[1] = 0x50 | (seq & 0x0F);
    out[2..7].copy_from_slice(&hd_sample(HIGH_FREQ, amp(r.high), LOW_FREQ, amp(r.low)));
    if model == Switch2Model::Pro {
        // Second motor.
        out.copy_within(1..7, 0x11);
    }
    out
}

/// GameCube rumble (a plain motor): 1 = on, 0 = off, 2 = stop.
pub fn gamecube_rumble_report(seq: u8, state: u8) -> [u8; 64] {
    let mut out = [0u8; 64];
    out[0] = report_id(Switch2Model::GameCube);
    out[1] = 0x50 | (seq & 0x0F);
    out[2] = state;
    out
}

/// Works out the gyro scale. The IMU timestamps normally count microseconds, and the
/// gyro's full scale is then 34.8 rad/s; some units run a different timebase and a
/// 40.0 rad/s full scale.
#[derive(Clone, Debug, Default)]
struct ImuClock {
    samples: u32,
    first: Option<(u32, Instant)>,
    scale: Option<f32>,
}

impl ImuClock {
    const SKIP: u32 = 5;
    const MEASURE: Duration = Duration::from_millis(400);

    fn update(&mut self, ts: u32, now: Instant) {
        if self.scale.is_some() || ts == 0 {
            return;
        }
        self.samples += 1;
        if self.samples < Self::SKIP {
            return;
        }
        let Some((ts0, t0)) = self.first else {
            self.first = Some((ts, now));
            return;
        };
        let host = now.duration_since(t0);
        if host < Self::MEASURE {
            return;
        }
        let ratio = f64::from(ts.wrapping_sub(ts0)) / host.as_micros() as f64;
        let full_scale_rad = if (0.9..=1.1).contains(&ratio) {
            34.8
        } else {
            40.0
        };
        self.scale = Some((full_scale_rad as f32).to_degrees() / 32767.0);
    }
}

pub struct Switch2Driver {
    model: Switch2Model,
    /// The vendor interface, or why it could not be opened.
    bulk: Result<Box<dyn BulkIo>, String>,
    /// The vendor interface once the program that held it at the start lets go.
    late: Option<LateBulk>,
    /// The vendor interface arrived late and the limitation notice is gone.
    notice_cleared: bool,
    cal: Calibration,
    clock: ImuClock,
    seq: u8,
    pacer: RumblePacer,
    gc_error: u32,
    /// Player lights wanted and last sent (bits 0..3: LEDs 1..4).
    player_want: Option<u8>,
    player_sent: Option<u8>,
}

impl Switch2Driver {
    pub fn new(model: Switch2Model, bulk: Result<Box<dyn BulkIo>, String>) -> Self {
        Switch2Driver {
            model,
            bulk,
            late: None,
            notice_cleared: false,
            cal: Calibration::default(),
            clock: ImuClock::default(),
            seq: 0,
            pacer: RumblePacer::new(
                Duration::from_millis(12),
                Some(Duration::from_millis(12)),
                1,
            ),
            gc_error: 0,
            player_want: None,
            player_sent: Some(0x01),
        }
    }

    /// Take over the vendor interface when it arrives through `late` (it was busy at the
    /// start); until then the pad runs on another program's initialisation.
    pub fn with_retry(mut self, late: LateBulk) -> Self {
        self.late = Some(late);
        self
    }

    pub fn calibration(&self) -> &Calibration {
        &self.cal
    }

    /// Read the calibration and (re)start output through a vendor interface that became
    /// free while the pad was running. HID input continues meanwhile.
    fn adopt_late_bulk(&mut self) {
        let Some(mut bulk) = self.late.as_ref().and_then(LateBulk::try_take) else {
            return;
        };
        self.late = None;
        match self.start(bulk.as_mut()) {
            Ok(()) => {
                tracing::info!("Switch 2 controller: took over its USB command interface");
                self.bulk = Ok(bulk);
                self.notice_cleared = true;
            }
            Err(e) => {
                tracing::warn!("Switch 2 controller: cannot use its USB command interface: {e}");
            }
        }
    }

    fn identity(&self, notice: Option<String>) -> Identity {
        let (name, family, joycon) = match self.model {
            Switch2Model::Pro => ("Switch 2 Pro Controller", PadFamily::SwitchPro, None),
            Switch2Model::GameCube => ("GameCube Controller", PadFamily::GameCube, None),
            Switch2Model::JoyConLeft => {
                ("Joy-Con 2 (L)", PadFamily::JoyCon, Some(JoyConSide::Left))
            }
            Switch2Model::JoyConRight => {
                ("Joy-Con 2 (R)", PadFamily::JoyCon, Some(JoyConSide::Right))
            }
        };
        Identity {
            name: name.to_owned(),
            family,
            joycon,
            has_motion: self.model != Switch2Model::GameCube,
            has_touchpad: false,
            has_rumble: true,
            notice,
        }
    }

    fn read_calibration(&mut self, bulk: &mut dyn BulkIo) {
        let mut read = |addr: u32| -> Option<Vec<u8>> {
            let reply = bulk
                .command(&flash_read_command(addr), FLASH_REPLY_LEN, COMMAND_TIMEOUT)
                .map_err(|e| tracing::debug!(addr, "flash read failed: {e}"))
                .ok()?;
            flash_block(&reply).map(<[u8]>::to_vec)
        };
        if let Some(b) = read(0x13040) {
            self.cal.gyro_bias = [4, 8, 12].map(|at| le_f32(&b, at).to_degrees());
            if !self
                .cal
                .gyro_bias
                .iter()
                .all(|v| v.is_finite() && v.abs() < 50.0)
            {
                self.cal.gyro_bias = [0.0; 3];
            }
        }
        if let Some(b) = read(0x13080) {
            self.cal.left = stick_cal(&b[0x28..]);
        }
        if let Some(b) = read(0x130C0) {
            self.cal.right = stick_cal(&b[0x28..]);
        }
        if self.model == Switch2Model::GameCube
            && let Some(b) = read(0x13140)
        {
            self.cal.trigger_zero = [b[0], b[1]];
        }
        // User calibration. The left stick lives at 0x1FC040; for the right stick, sources
        // disagree between 0x1FC080 and 0x1FC060 (offset 0x20 of the first block).
        let user = read(0x1FC040);
        if let Some(b) = &user
            && b[..2] == CAL_MAGIC
        {
            self.cal.left = stick_cal(&b[2..]);
        }
        let right = read(0x1FC080)
            .filter(|b| b[..2] == CAL_MAGIC)
            .map(|b| b[2..11].to_vec())
            .or_else(|| {
                user.filter(|b| b[0x20..0x22] == CAL_MAGIC)
                    .map(|b| b[0x22..0x2B].to_vec())
            });
        if let Some(b) = right {
            self.cal.right = stick_cal(&b);
        }
    }

    fn send(bulk: &mut dyn BulkIo, cmd: &[u8]) -> io::Result<Vec<u8>> {
        bulk.command(cmd, 64, COMMAND_TIMEOUT)
    }

    fn start(&mut self, bulk: &mut dyn BulkIo) -> Result<(), InitError> {
        self.read_calibration(bulk);
        let mut ok = true;
        for cmd in init_commands(FEATURES) {
            if let Err(e) = Self::send(bulk, &cmd) {
                tracing::debug!(cmd = cmd[0], sub = cmd[3], "init command failed: {e}");
                ok = false;
                break;
            }
        }
        if !ok {
            // Fall back to the two commands that matter.
            let _ = Self::send(bulk, &select_report_command());
            Self::send(bulk, &start_output_command())?;
        }
        if let Err(e) = Self::send(bulk, &player_led_command(0x01)) {
            tracing::debug!("player LED: {e}");
        }
        Ok(())
    }

    /// Wait for input that another program's initialisation already started.
    fn wait_for_input(io: &mut dyn ReportIo) -> io::Result<bool> {
        let deadline = io.now() + FOREIGN_INIT_WAIT;
        let mut buf = [0u8; 64];
        loop {
            let now = io.now();
            if now >= deadline {
                return Ok(false);
            }
            if let Some(n) = io.read(&mut buf, deadline - now)?
                && buf[..n].first() == Some(&INPUT_REPORT)
            {
                return Ok(true);
            }
        }
    }
}

impl PadDriver for Switch2Driver {
    fn init(&mut self, io: &mut dyn ReportIo) -> Result<Identity, InitError> {
        let mut bulk = std::mem::replace(&mut self.bulk, Err(String::new()));
        let result = match &mut bulk {
            Ok(b) => self.start(b.as_mut()).map(|()| self.identity(None)),
            Err(reason) => {
                if Self::wait_for_input(io)? {
                    let until = if self.late.is_some() {
                        " until that interface is free"
                    } else {
                        ""
                    };
                    Ok(self.identity(Some(format!(
                        "{reason}; it works, but with default stick calibration{until}"
                    ))))
                } else {
                    Err(InitError::Blocked(reason.clone()))
                }
            }
        };
        self.bulk = bulk;
        result
    }

    fn parse(&mut self, report: &[u8], now: Instant) -> Option<PadState> {
        if report.len() >= 0x3F && report[0] == INPUT_REPORT {
            self.clock.update(imu_timestamp(report), now);
        }
        let scale = self
            .clock
            .scale
            .filter(|_| self.model != Switch2Model::GameCube);
        parse_report(self.model, &mut self.cal, scale, report)
    }

    fn set_leds(&mut self, leds: Leds) {
        self.player_want = leds.player.map(player_mask);
    }

    fn service(&mut self, io: &mut dyn ReportIo, rumble: Rumble) -> io::Result<()> {
        self.adopt_late_bulk();
        if let Some(mask) = self.player_want.filter(|m| Some(*m) != self.player_sent)
            && let Ok(bulk) = &mut self.bulk
        {
            if let Err(e) = Self::send(bulk.as_mut(), &player_led_command(mask)) {
                tracing::debug!("player LED: {e}");
            }
            self.player_sent = Some(mask);
        }
        let Some(r) = self.pacer.poll(io.now(), rumble) else {
            return Ok(());
        };
        let seq = self.seq;
        self.seq = (self.seq + 1) & 0x0F;
        let report = if self.model == Switch2Model::GameCube {
            // Partial strength by pulsing the motor (error diffusion at the refresh rate).
            let strength = (r.low.max(r.high).clamp(0.0, 1.0) * 65535.0) as u32;
            let state = if strength == 0 {
                self.gc_error = 0;
                2
            } else if self.gc_error < strength {
                self.gc_error += 65535 - strength;
                1
            } else {
                self.gc_error -= strength;
                0
            };
            gamecube_rumble_report(seq, state)
        } else {
            rumble_report(self.model, seq, r)
        };
        io.write(&report)
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.pacer.next_deadline()
    }

    fn take_notice_change(&mut self) -> Option<Option<String>> {
        std::mem::take(&mut self.notice_cleared).then_some(None)
    }

    fn shutdown(&mut self, io: &mut dyn ReportIo) {
        let report = if self.model == Switch2Model::GameCube {
            gamecube_rumble_report(self.seq, 2)
        } else {
            rumble_report(self.model, self.seq, Rumble::default())
        };
        let _ = io.write(&report);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::padio::testing::{ScriptBulk, ScriptIo};
    use crate::report::pack12;

    const MS: Duration = Duration::from_millis(1);

    /// Flash image as address -> 0x40-byte block.
    fn fake_bulk(blocks: Vec<(u32, Vec<u8>)>) -> (ScriptBulk, Arc<Mutex<Vec<Vec<u8>>>>) {
        let bulk = ScriptBulk::new(move |cmd: &[u8]| {
            let mut reply = vec![0u8; 64];
            reply[0] = cmd[0];
            reply[1] = 0x01;
            reply[3] = cmd[3];
            if cmd[0] == 0x02 {
                let addr = le_u32(cmd, 12);
                reply.resize(FLASH_REPLY_LEN, 0);
                if let Some((_, b)) = blocks.iter().find(|(a, _)| *a == addr) {
                    reply[0x10..0x50].copy_from_slice(b);
                }
            }
            Ok(reply)
        });
        let sent = bulk.sent.clone();
        (bulk, sent)
    }

    fn report() -> Vec<u8> {
        let mut r = vec![0u8; 64];
        r[0] = INPUT_REPORT;
        r[11..14].copy_from_slice(&pack12(2048, 2048));
        r[14..17].copy_from_slice(&pack12(2048, 2048));
        r
    }

    #[test]
    fn command_header() {
        assert_eq!(
            start_output_command(),
            [
                0x03, 0x91, 0x00, 0x0D, 0x00, 0x08, 0x00, 0x00, 0x01, 0x00, 0xFF, 0xFF, 0xFF, 0xFF,
                0xFF, 0xFF
            ]
        );
        assert_eq!(
            flash_read_command(0x0013_0080),
            [
                0x02, 0x91, 0x00, 0x01, 0x00, 0x08, 0x00, 0x00, 0, 0, 0, 0, 0x80, 0x00, 0x13, 0x00
            ]
        );
        let seq = init_commands(FEATURES);
        assert_eq!(seq.len(), 10);
        for c in &seq {
            assert_eq!(
                c.len(),
                8 + c[5] as usize,
                "length byte matches the payload"
            );
        }
        assert_eq!(seq[3].len(), 28);
        assert_eq!(seq[1][8], 0x27);
    }

    #[test]
    fn init_reads_calibration_then_starts() {
        let mut left = vec![0u8; 0x40];
        left[0x28..0x2B].copy_from_slice(&pack12(2000, 2100));
        left[0x2B..0x2E].copy_from_slice(&pack12(1500, 1500));
        left[0x2E..0x31].copy_from_slice(&pack12(1400, 1400));
        let mut user = vec![0u8; 0x40];
        user[0x20..0x22].copy_from_slice(&CAL_MAGIC); // right stick at 0x1FC060
        user[0x22..0x25].copy_from_slice(&pack12(1900, 1950));
        user[0x25..0x28].copy_from_slice(&pack12(1200, 1200));
        user[0x28..0x2B].copy_from_slice(&pack12(1100, 1100));
        let (bulk, sent) = fake_bulk(vec![(0x13080, left), (0x1FC040, user)]);
        let mut d = Switch2Driver::new(Switch2Model::Pro, Ok(Box::new(bulk)));
        let id = d.init(&mut ScriptIo::silent()).unwrap();
        assert_eq!(id.name, "Switch 2 Pro Controller");
        assert!(id.has_motion && id.has_rumble && id.notice.is_none());
        assert_eq!(d.cal.left.x.center, 2000.0);
        assert_eq!(d.cal.left.y.center, 2100.0);
        assert_eq!(d.cal.right.x.center, 1900.0, "falls back to 0x1FC060");
        let sent = sent.lock().unwrap();
        let start = sent
            .iter()
            .position(|c| c[..4] == [0x03, 0x91, 0x00, 0x0D])
            .unwrap();
        assert!(
            sent[..start].iter().any(|c| c[0] == 0x02),
            "flash read before start"
        );
        assert_eq!(sent.last().unwrap()[..4], [0x09, 0x91, 0x00, 0x07]);
    }

    #[test]
    fn failed_sequence_falls_back_to_start() {
        let bulk = ScriptBulk::new(|cmd: &[u8]| {
            if cmd[0] == 0x0A {
                Err(io::ErrorKind::TimedOut.into())
            } else {
                Ok(vec![0; 64])
            }
        });
        let sent = bulk.sent.clone();
        let mut d = Switch2Driver::new(Switch2Model::Pro, Ok(Box::new(bulk)));
        d.init(&mut ScriptIo::silent()).unwrap();
        let sent = sent.lock().unwrap();
        assert!(sent.iter().any(|c| *c == start_output_command()));
        assert!(sent.iter().any(|c| *c == select_report_command()));
    }

    #[test]
    fn foreign_init_is_accepted_with_notice() {
        let mut io = ScriptIo::silent();
        io.push_input(20 * MS, report());
        let mut d = Switch2Driver::new(Switch2Model::GameCube, Err("busy".into()));
        let id = d.init(&mut io).unwrap();
        assert!(id.notice.unwrap().starts_with("busy"));
        assert_eq!(id.family, PadFamily::GameCube);

        let mut d = Switch2Driver::new(Switch2Model::Pro, Err("needs WinUSB".into()));
        match d.init(&mut ScriptIo::silent()) {
            Err(InitError::Blocked(m)) => assert_eq!(m, "needs WinUSB"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn busy_interface_is_taken_over_once_free() {
        let mut io = ScriptIo::silent();
        io.push_input(20 * MS, report());
        let (late, retry) = crate::padio::late_bulk();
        let mut d =
            Switch2Driver::new(Switch2Model::Pro, Err("Steam is using it".into())).with_retry(late);
        let id = d.init(&mut io).unwrap();
        assert_eq!(
            id.notice.unwrap(),
            "Steam is using it; it works, but with default stick calibration until that \
             interface is free"
        );
        d.service(&mut io, Rumble::default()).unwrap();
        assert_eq!(d.take_notice_change(), None, "still busy");

        let mut left = vec![0u8; 0x40];
        left[0x28..0x2B].copy_from_slice(&pack12(2000, 2100));
        left[0x2B..0x2E].copy_from_slice(&pack12(1500, 1500));
        left[0x2E..0x31].copy_from_slice(&pack12(1400, 1400));
        let (bulk, sent) = fake_bulk(vec![(0x13080, left)]);
        retry.hand_over(Box::new(bulk));
        d.service(&mut io, Rumble::default()).unwrap();
        assert_eq!(d.take_notice_change(), Some(None));
        assert_eq!(d.take_notice_change(), None, "reported once");
        assert_eq!(d.cal.left.x.center, 2000.0);
        assert!(
            sent.lock()
                .unwrap()
                .iter()
                .any(|c| *c == start_output_command())
        );
    }

    #[test]
    fn parse_buttons_and_sticks() {
        let mut cal = Calibration::default();
        let mut r = report();
        r[5] = 0x08 | 0x80; // A + ZR
        r[6] = 0x40; // C
        r[8] = 0x03; // GR + GL
        r[11..14].copy_from_slice(&pack12(4095, 0)); // left stick right + down
        let s = parse_report(Switch2Model::Pro, &mut cal, None, &r).unwrap();
        assert!(s.pressed(PadButton::B) && s.pressed(PadButton::Misc2));
        assert!(s.pressed(PadButton::PaddleUL) && s.pressed(PadButton::PaddleUR));
        assert_eq!(s.axis(Axis::RightTrigger), 1.0);
        assert!(s.axis(Axis::LeftX) > 0.99 && s.axis(Axis::LeftY) < -0.99);
        assert!(
            s.motion.is_none(),
            "no motion before the gyro scale is known"
        );
        assert!(parse_report(Switch2Model::Pro, &mut cal, None, &r[..40]).is_none());
    }

    #[test]
    fn gamecube_layout() {
        let mut cal = Calibration {
            trigger_zero: [30, 30],
            ..Default::default()
        };
        let mut r = report();
        r[5] = 0x08 | 0x04 | 0x80; // A, B, Z
        r[6] = 0x02; // Start
        r[7] = 0x80 | 0x02; // ZL, up
        r[0x3D] = 232;
        r[0x3E] = 30;
        let s = parse_report(Switch2Model::GameCube, &mut cal, None, &r).unwrap();
        assert!(
            s.pressed(PadButton::A) && s.pressed(PadButton::X),
            "B sits west of A"
        );
        assert!(s.pressed(PadButton::RB) && s.pressed(PadButton::LB));
        assert!(s.pressed(PadButton::Start) && s.pressed(PadButton::DpadUp));
        assert_eq!(s.axis(Axis::LeftTrigger), 1.0);
        assert_eq!(s.axis(Axis::RightTrigger), 0.0);
    }

    #[test]
    fn motion_after_timebase_is_known() {
        let mut d = Switch2Driver::new(Switch2Model::Pro, Err(String::new()));
        let t0 = Instant::now();
        let mut r = report();
        // Accel: +1 g on device z; gyro: device x raw 1000.
        r[0x35..0x37].copy_from_slice(&4096i16.to_le_bytes());
        r[0x37..0x39].copy_from_slice(&1000i16.to_le_bytes());
        let mut last = None;
        for i in 0..200u32 {
            let ts = 1_000_000 + i * 4000; // microseconds, 4 ms apart
            r[0x2B..0x2F].copy_from_slice(&ts.to_le_bytes());
            last = d.parse(&r, t0 + Duration::from_micros(4000 * u64::from(i)));
        }
        let m = last
            .unwrap()
            .motion
            .expect("motion once the timebase is measured");
        assert!((m.accel[1] - 1.0).abs() < 1e-4);
        let expected = 1000.0 * 34.8f32.to_degrees() / 32767.0;
        assert!(
            (m.gyro[0] - expected).abs() < 0.01,
            "{} vs {expected}",
            m.gyro[0]
        );

        let mut d = Switch2Driver::new(Switch2Model::Pro, Err(String::new()));
        for i in 0..200u32 {
            let ts = i * 1000; // not microseconds
            r[0x2B..0x2F].copy_from_slice(&(ts + 1).to_le_bytes());
            last = d.parse(&r, t0 + Duration::from_micros(4000 * u64::from(i)));
        }
        let m = last.unwrap().motion.unwrap();
        let expected = 1000.0 * 40.0f32.to_degrees() / 32767.0;
        assert!((m.gyro[0] - expected).abs() < 0.01);
    }

    #[test]
    fn rumble_packing() {
        assert_eq!(
            hd_sample(0x187, 0, 0x112, 0),
            [0x87, 0x01, 0x20, 0x11, 0x00]
        );
        let s = hd_sample(0x3FF, 0xFFFF, 0x3FF, 0xFFFF);
        assert_eq!(s, [0xFF; 5]);
        let r = rumble_report(
            Switch2Model::Pro,
            3,
            Rumble {
                low: 1.0,
                high: 0.0,
            },
        );
        assert_eq!(r[0], 0x02);
        assert_eq!(r[1], 0x53);
        assert_eq!(r[0x11..0x17], r[1..7], "second motor mirrors the first");
        let la = u32::from((u16::from(r[5]) >> 6) | u16::from(r[6]) << 2);
        assert_eq!(la, 29000 >> 6);
        let jc = rumble_report(Switch2Model::JoyConLeft, 0, Rumble::default());
        assert_eq!(jc[0], 0x01);
        assert!(jc[0x11..].iter().all(|&b| b == 0));
    }

    #[test]
    fn gamecube_rumble_pulses() {
        let mut d = Switch2Driver::new(Switch2Model::GameCube, Err(String::new()));
        let mut io = ScriptIo::silent();
        let half = Rumble {
            low: 0.5,
            high: 0.0,
        };
        for _ in 0..20 {
            d.service(&mut io, half).unwrap();
            io.advance(12 * MS);
        }
        let on = io.writes().iter().filter(|w| w[2] == 1).count();
        assert!((8..=12).contains(&on), "about half the pulses are on: {on}");
        d.service(&mut io, Rumble::default()).unwrap();
        assert_eq!(io.writes().last().unwrap()[2], 2, "stop");
    }

    #[test]
    fn truncated_reports_do_not_panic() {
        let mut r = report();
        r[5..9].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0x03]);
        r[0x2B..0x2F].copy_from_slice(&1u32.to_le_bytes());
        r[0x31..0x3F].fill(0x11);
        let models = [
            Switch2Model::Pro,
            Switch2Model::GameCube,
            Switch2Model::JoyConLeft,
        ];
        let now = Instant::now();
        for model in models {
            let s = parse_report(model, &mut Calibration::default(), Some(0.07), &r);
            assert!(s.unwrap().motion.is_some(), "the whole report parses");
            let mut d = Switch2Driver::new(model, Err(String::new()));
            for n in 0..=r.len() {
                for scale in [None, Some(0.07)] {
                    parse_report(model, &mut Calibration::default(), scale, &r[..n]);
                }
                d.parse(&r[..n], now);
            }
        }
    }

    /// The player-light masks sent through the vendor interface.
    fn lights(sent: &Mutex<Vec<Vec<u8>>>) -> Vec<u8> {
        sent.lock()
            .unwrap()
            .iter()
            .filter(|c| c[..4] == [0x09, 0x91, 0x00, 0x07])
            .map(|c| c[8])
            .collect()
    }

    #[test]
    fn player_lights_follow_the_slot() {
        let player2 = Leds {
            player: Some(2),
            mic: None,
        };
        let (bulk, sent) = fake_bulk(Vec::new());
        let mut d = Switch2Driver::new(Switch2Model::Pro, Ok(Box::new(bulk)));
        let mut io = ScriptIo::silent();
        d.init(&mut io).unwrap();
        d.set_leds(player2);
        d.service(&mut io, Rumble::default()).unwrap();
        d.service(&mut io, Rumble::default()).unwrap();
        assert_eq!(
            lights(&sent),
            [0x01, 0x03],
            "player 1 at the start, then 2 once"
        );

        // The interface was busy at the start: the lights wait for the hand-over.
        let mut io = ScriptIo::silent();
        io.push_input(20 * MS, report());
        let (late, retry) = crate::padio::late_bulk();
        let mut d = Switch2Driver::new(Switch2Model::Pro, Err("busy".into())).with_retry(late);
        d.init(&mut io).unwrap();
        d.set_leds(player2);
        d.service(&mut io, Rumble::default()).unwrap();
        let (bulk, sent) = fake_bulk(Vec::new());
        retry.hand_over(Box::new(bulk));
        d.service(&mut io, Rumble::default()).unwrap();
        d.service(&mut io, Rumble::default()).unwrap();
        assert_eq!(lights(&sent), [0x01, 0x03]);
    }
}
