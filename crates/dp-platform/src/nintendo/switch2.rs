//! Switch 2 controllers: Pro Controller 2, GameCube controller, Joy-Con 2.
//!
//! Over USB the pads are composite devices. Interface 0 is an ordinary HID game pad;
//! interface 1 is a vendor interface with two bulk endpoints that takes 8-byte-header
//! commands. The HID interface stays silent until the vendor interface has been told to
//! start output, after which it streams input report 0x05 at about 250 Hz. Rumble goes out
//! as HID output reports.
//!
//! Over Bluetooth LE the same reports and commands travel through Nintendo's own GATT
//! characteristics ([`super::switch2ble`], carried by [`crate::switch2gatt`]). Input starts as
//! soon as the host subscribes to it, as often as the host's connection interval allows (about
//! 33 Hz on macOS).

use std::io;
use std::time::{Duration, Instant};

use dp_core::pad::{Axis, Motion, PadButton, PadState, Rumble};

use super::{CAL_MAGIC, FIRST_PLAYER, StickCal, Switch2Model, apply_buttons, gc_trigger};
use crate::nintendo::PlayerLights;
use crate::padio::{
    BulkIo, Identity, InitError, LateBulk, PadDriver, ReportIo, RumblePacer, Seq4, read_until,
};
use crate::report::{le_f32, le_i16, le_u16, le_u32, unpack12};
use crate::{Battery, JoyConSide, Leds, PadFamily};

/// Feature mask: buttons, sticks, IMU and rumble.
const FEATURES: u8 = 0x27;
pub(crate) const INPUT_REPORT: u8 = 0x05;

const COMMAND_TIMEOUT: Duration = Duration::from_millis(200);
/// Over Bluetooth a reply waits for the next connection event, and some take 200 ms.
const BLUETOOTH_COMMAND_TIMEOUT: Duration = Duration::from_millis(600);
/// How long to wait for input when another program started the pad.
const FOREIGN_INIT_WAIT: Duration = Duration::from_millis(1500);
/// Full strength is capped below the actuators' maximum, as the console does.
const RUMBLE_MAX: f32 = 29000.0;
const HIGH_FREQ: u16 = 0x187;
const LOW_FREQ: u16 = 0x112;

/// How a pad is connected, which its command headers name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    Usb,
    Bluetooth,
}

impl Transport {
    /// How long a command may take to answer.
    fn timeout(self) -> Duration {
        match self {
            Transport::Usb => COMMAND_TIMEOUT,
            Transport::Bluetooth => BLUETOOTH_COMMAND_TIMEOUT,
        }
    }
}

/// One vendor command: `[cmd, 0x91 (host to device), transport, sub, 0, len, 0, 0] + payload`.
pub fn command(transport: Transport, cmd: u8, sub: u8, payload: &[u8]) -> Vec<u8> {
    let link = match transport {
        Transport::Usb => 0x00,
        Transport::Bluetooth => 0x01,
    };
    let mut c = vec![cmd, 0x91, link, sub, 0x00, payload.len() as u8, 0x00, 0x00];
    c.extend_from_slice(payload);
    c
}

/// Rumble settings the console sends at the start.
fn vibration_config() -> Vec<u8> {
    let mut vibration = vec![0x01];
    vibration.extend_from_slice(&[0xFF; 8]);
    vibration.extend_from_slice(&[0x35, 0x00, 0x46, 0x00]);
    vibration.resize(20, 0);
    vibration
}

/// The start-up sequence the console sends over USB. Only the last command ("start
/// output") is strictly needed; the second-to-last selects input report 0x05.
fn init_commands(features: u8) -> Vec<Vec<u8>> {
    let usb = |cmd, sub, payload: &[u8]| command(Transport::Usb, cmd, sub, payload);
    vec![
        usb(0x07, 0x01, &[]),
        usb(0x0C, 0x02, &[features, 0, 0, 0]),
        usb(0x11, 0x01, &[]),
        usb(0x0A, 0x08, &vibration_config()),
        usb(0x0C, 0x04, &[features, 0, 0, 0]),
        usb(0x01, 0x0C, &[]),
        usb(0x01, 0x01, &[]),
        // Enables GL/GR on the charging grip.
        usb(0x08, 0x02, &[0x01, 0, 0, 0]),
        select_report_command(),
        start_output_command(),
    ]
}

/// Over Bluetooth input already flows; the features (motion) and rumble are switched on as
/// the console does it.
fn bluetooth_init_commands(features: u8) -> Vec<Vec<u8>> {
    let bt = |cmd, sub, payload: &[u8]| command(Transport::Bluetooth, cmd, sub, payload);
    vec![
        bt(0x0C, 0x02, &[features, 0, 0, 0]),
        bt(0x0A, 0x08, &vibration_config()),
        bt(0x0C, 0x04, &[features, 0, 0, 0]),
    ]
}

fn select_report_command() -> Vec<u8> {
    command(Transport::Usb, 0x03, 0x0A, &[INPUT_REPORT, 0, 0, 0])
}

/// The trailing six bytes are a host address; any value works.
fn start_output_command() -> Vec<u8> {
    command(
        Transport::Usb,
        0x03,
        0x0D,
        &[0x01, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
    )
}

/// Read a 0x40-byte flash block.
fn flash_read_command(transport: Transport, addr: u32) -> Vec<u8> {
    let a = addr.to_le_bytes();
    command(transport, 0x02, 0x01, &[0, 0, 0, 0, a[0], a[1], a[2], a[3]])
}

/// Length of a flash-read reply; the block starts at offset 0x10.
const FLASH_REPLY_LEN: usize = 0x50;

// Flash blocks (0x40 bytes each).
/// Factory IMU calibration: the gyro bias as three f32 (rad/s) from offset 4.
const FLASH_IMU: u32 = 0x13040;
/// Factory stick calibration: nine bytes at offset 0x28 (see [`stick_cal`]).
const FLASH_LEFT_STICK: u32 = 0x13080;
const FLASH_RIGHT_STICK: u32 = 0x130C0;
/// Factory rest values of the GameCube triggers: left, right.
const FLASH_GC_TRIGGERS: u32 = 0x13140;
/// User stick calibration, each behind the calibration magic.
const FLASH_USER_LEFT: u32 = 0x1FC040;
const FLASH_USER_RIGHT: u32 = 0x1FC080;

// Input report 0x05, offsets from the report ID.
/// Buttons: right, middle and left byte, as on the first generation.
const IN_BUTTONS: usize = 5;
/// GR and GL (Pro Controller).
const IN_PADDLES: usize = 8;
/// Sticks: x and y packed in 12 bits each.
const IN_LEFT_STICK: usize = 11;
const IN_RIGHT_STICK: usize = 14;
/// Battery voltage in mV (u16).
const IN_BATTERY: usize = 0x20;
/// 0 on the battery; on external power it rises to 0x34 while charging and is 0x20 once charged.
const IN_CHARGE: usize = 0x22;
const CHARGED: u8 = 0x20;
/// IMU timestamp (u32), 0 while the IMU is off.
const IN_IMU_TIME: usize = 0x2B;
/// Accelerometer, then gyroscope: three i16 each.
const IN_ACCEL: usize = 0x31;
const IN_GYRO: usize = 0x37;
/// Analog GameCube triggers: left, right.
const IN_GC_TRIGGERS: usize = 0x3D;
/// The input report up to its last field read here.
const IN_LEN: usize = 0x3F;

fn flash_block(reply: &[u8]) -> Option<&[u8]> {
    reply.get(0x10..FLASH_REPLY_LEN)
}

/// Player LEDs: bits 0..3 are LEDs 1..4.
fn player_led_command(transport: Transport, mask: u8) -> Vec<u8> {
    command(transport, 0x09, 0x07, &[mask, 0, 0, 0, 0, 0, 0, 0])
}

/// Nine bytes: neutral x/y, distance to the maximum x/y, distance to the minimum x/y.
fn stick_cal(b: &[u8]) -> StickCal {
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
    let (face, mid, left) = (r[IN_BUTTONS], r[IN_BUTTONS + 1], r[IN_BUTTONS + 2]);
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
    let triggers = &r[IN_GC_TRIGGERS..IN_GC_TRIGGERS + 2];
    s.set_axis(
        Axis::LeftTrigger,
        gc_trigger(triggers[0], cal.trigger_zero[0]),
    );
    s.set_axis(
        Axis::RightTrigger,
        gc_trigger(triggers[1], cal.trigger_zero[1]),
    );
}

/// Charge by the voltage the pads report: (mV, percent), linear in between. The voltage is on a
/// scale of its own, well below a Li-ion cell's: a Pro Controller read 3.71–3.72 V just after
/// it called itself charged, and 3.69 V while its own coarse level (in report 0x09, which is
/// not read here) said 8 of 9; a GameCube controller cut out at 2.94 V (the
/// `switch2-controllers-linux` project). In between, the shape of a Li-ion discharge, steep at
/// both ends.
const CHARGE_CURVE: [(f32, f32); 11] = [
    (2950.0, 0.0),
    (3191.0, 8.0),
    (3258.0, 20.0),
    (3297.0, 30.0),
    (3335.0, 40.0),
    (3374.0, 50.0),
    (3422.0, 60.0),
    (3479.0, 70.0),
    (3547.0, 80.0),
    (3624.0, 90.0),
    (3720.0, 100.0),
];

fn charge_percent(mv: f32) -> f32 {
    let (first, last) = (CHARGE_CURVE[0], CHARGE_CURVE[CHARGE_CURVE.len() - 1]);
    if mv <= first.0 {
        return first.1;
    }
    CHARGE_CURVE
        .windows(2)
        .find(|w| mv <= w[1].0)
        .map_or(last.1, |w| {
            let ((v0, p0), (v1, p1)) = (w[0], w[1]);
            p0 + (p1 - p0) * (mv - v0) / (v1 - v0)
        })
}

/// The battery from the voltage the pad reports, smoothed so that rumble and radio bursts do
/// not move it, in steps of 5 %. Plugging in or out moves the voltage at once and starts over.
#[derive(Clone, Debug, Default)]
struct BatteryGauge {
    mv: Option<f32>,
    charge: u8,
    last: Option<Instant>,
}

impl BatteryGauge {
    /// Time constant of the smoothing.
    const SMOOTHING: Duration = Duration::from_secs(10);

    fn update(&mut self, r: &[u8], now: Instant) {
        let mv = f32::from(le_u16(r, IN_BATTERY));
        if mv == 0.0 {
            return;
        }
        let charge = r[IN_CHARGE];
        let same_power = (charge == 0) == (self.charge == 0);
        self.mv = match (self.mv, self.last) {
            (Some(prev), Some(t)) if same_power => {
                let dt = now.saturating_duration_since(t).as_secs_f32();
                let a = 1.0 - (-dt / Self::SMOOTHING.as_secs_f32()).exp();
                Some(prev + (mv - prev) * a)
            }
            _ => Some(mv),
        };
        self.charge = charge;
        self.last = Some(now);
    }

    fn battery(&self) -> Option<Battery> {
        let mv = self.mv?;
        Some(Battery {
            level: ((charge_percent(mv) / 5.0).round() * 5.0) as u8,
            charging: self.charge != 0 && self.charge != CHARGED,
            wired: false,
        })
    }
}

/// IMU timestamp of an input report (0 while the IMU is off).
fn imu_timestamp(r: &[u8]) -> u32 {
    le_u32(r, IN_IMU_TIME)
}

/// Parse input report 0x05. `gyro_scale` is degrees per second per raw unit; motion is
/// left out while it is unknown.
pub fn parse_report(
    model: Switch2Model,
    cal: &mut Calibration,
    gyro_scale: Option<f32>,
    r: &[u8],
) -> Option<PadState> {
    if r.len() < IN_LEN || r[0] != INPUT_REPORT {
        return None;
    }
    let mut s = PadState::default();
    if model == Switch2Model::GameCube {
        apply_gamecube(&mut s, r, cal);
    } else {
        apply_buttons(&mut s, r[IN_BUTTONS], r[IN_BUTTONS + 1], r[IN_BUTTONS + 2]);
        if r[IN_PADDLES] & 0x01 != 0 {
            s.set(PadButton::PaddleUR, true); // GR
        }
        if r[IN_PADDLES] & 0x02 != 0 {
            s.set(PadButton::PaddleUL, true); // GL
        }
    }
    let stick = |at: usize| unpack12(&r[at..at + 3]);
    let (lx, ly) = cal.left.normalize(stick(IN_LEFT_STICK));
    let (rx, ry) = cal.right.normalize(stick(IN_RIGHT_STICK));
    s.set_axis(Axis::LeftX, lx);
    s.set_axis(Axis::LeftY, ly);
    s.set_axis(Axis::RightX, rx);
    s.set_axis(Axis::RightY, ry);
    if let Some(scale) = gyro_scale
        && imu_timestamp(r) != 0
    {
        let v = |at: usize| f32::from(le_i16(r, at));
        let a: [f32; 3] = std::array::from_fn(|i| v(IN_ACCEL + 2 * i) / 4096.0);
        let g: [f32; 3] = std::array::from_fn(|i| v(IN_GYRO + 2 * i) * scale - cal.gyro_bias[i]);
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
fn hd_sample(high_freq: u16, high_amp: u16, low_freq: u16, low_amp: u16) -> [u8; 5] {
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

/// Rumble output report for the HD-rumble pads (everything but the GameCube controller):
/// report ID, then a 16-byte block per motor: `0x50 | seq` and up to three HD samples the
/// motor plays one after the other. The Pro Controller's second motor's block repeats the
/// first at 0x11. Over USB, with a report every 12 ms, one sample is enough; over Bluetooth
/// a report arrives only every 30 ms or so, and with fewer samples the motor runs in bursts.
pub fn rumble_report(model: Switch2Model, seq: u8, r: Rumble, samples: usize) -> [u8; 64] {
    let amp = |v: f32| (v.clamp(0.0, 1.0) * RUMBLE_MAX) as u16;
    let sample = hd_sample(HIGH_FREQ, amp(r.high), LOW_FREQ, amp(r.low));
    let mut out = [0u8; 64];
    out[0] = report_id(model);
    out[1] = 0x50 | (seq & 0x0F);
    for i in 0..samples.clamp(1, 3) {
        out[2 + 5 * i..7 + 5 * i].copy_from_slice(&sample);
    }
    if model == Switch2Model::Pro {
        // Second motor.
        out.copy_within(1..0x11, 0x11);
    }
    out
}

/// GameCube rumble (a plain motor): 1 = on, 0 = off, 2 = stop.
fn gamecube_rumble_report(seq: u8, state: u8) -> [u8; 64] {
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

/// The pad's USB vendor interface (calibration, start-up commands, player lights).
enum Vendor {
    Ready(Box<dyn BulkIo>),
    /// Not open: why, and where it arrives once the program that held it lets go.
    Missing {
        reason: String,
        late: Option<LateBulk>,
    },
}

pub struct Switch2Driver {
    model: Switch2Model,
    transport: Transport,
    vendor: Vendor,
    /// The vendor interface arrived late and the limitation notice is gone.
    notice_cleared: bool,
    cal: Calibration,
    clock: ImuClock,
    gauge: BatteryGauge,
    seq: Seq4,
    pacer: RumblePacer,
    gc_error: u32,
    lights: PlayerLights,
}

impl Switch2Driver {
    pub fn new(model: Switch2Model, bulk: Result<Box<dyn BulkIo>, String>) -> Self {
        let vendor = match bulk {
            Ok(bulk) => Vendor::Ready(bulk),
            Err(reason) => Vendor::Missing { reason, late: None },
        };
        Switch2Driver {
            model,
            transport: Transport::Usb,
            vendor,
            notice_cleared: false,
            cal: Calibration::default(),
            clock: ImuClock::default(),
            gauge: BatteryGauge::default(),
            seq: Seq4::default(),
            pacer: RumblePacer::new(
                Duration::from_millis(12),
                Some(Duration::from_millis(12)),
                1,
            ),
            gc_error: 0,
            lights: PlayerLights::new(),
        }
    }

    /// A pad connected over Bluetooth LE, with its command channel. Rumble goes out twice
    /// per connection interval, and the stop three times: a write the link has no room for
    /// is dropped.
    pub fn bluetooth(model: Switch2Model, commands: Box<dyn BulkIo>) -> Self {
        Switch2Driver {
            transport: Transport::Bluetooth,
            pacer: RumblePacer::new(
                Duration::from_millis(15),
                Some(Duration::from_millis(15)),
                3,
            ),
            ..Switch2Driver::new(model, Ok(commands))
        }
    }

    /// Take over the vendor interface when it arrives through `late` (it was busy at the
    /// start); until then the pad runs on another program's initialisation.
    pub fn with_retry(mut self, retry: LateBulk) -> Self {
        if let Vendor::Missing { late, .. } = &mut self.vendor {
            *late = Some(retry);
        }
        self
    }

    /// HD samples per rumble report (see [`rumble_report`]).
    fn rumble_samples(&self) -> usize {
        match self.transport {
            Transport::Usb => 1,
            Transport::Bluetooth => 3,
        }
    }

    /// Read the calibration and (re)start output through a vendor interface that became
    /// free while the pad was running. The device thread waits for the commands' answers
    /// meanwhile, so no input is read for that time (a few round trips).
    fn adopt_late_bulk(&mut self) {
        let Vendor::Missing { late, .. } = &mut self.vendor else {
            return;
        };
        let Some(mut bulk) = late.as_ref().and_then(LateBulk::try_take) else {
            return;
        };
        *late = None;
        match start(self.model, self.transport, &mut self.cal, bulk.as_mut()) {
            Ok(()) => {
                tracing::info!("Switch 2 controller: took over its USB command interface");
                self.vendor = Vendor::Ready(bulk);
                self.notice_cleared = true;
            }
            Err(e) => {
                tracing::warn!("Switch 2 controller: cannot use its USB command interface: {e}");
            }
        }
    }
}

fn identity(model: Switch2Model, notice: Option<String>) -> Identity {
    let (name, family, joycon) = match model {
        Switch2Model::Pro => ("Switch 2 Pro Controller", PadFamily::SwitchPro, None),
        Switch2Model::GameCube => ("GameCube Controller", PadFamily::GameCube, None),
        Switch2Model::JoyConLeft => ("Joy-Con 2 (L)", PadFamily::JoyCon, Some(JoyConSide::Left)),
        Switch2Model::JoyConRight => ("Joy-Con 2 (R)", PadFamily::JoyCon, Some(JoyConSide::Right)),
    };
    Identity {
        joycon,
        has_motion: model != Switch2Model::GameCube,
        has_rumble: true,
        notice,
        ..Identity::new(name, family)
    }
}

/// Read the calibration blocks that are there into `cal`.
fn read_calibration(
    model: Switch2Model,
    transport: Transport,
    cal: &mut Calibration,
    bulk: &mut dyn BulkIo,
) {
    let mut read = |addr: u32| -> Option<Vec<u8>> {
        let reply = bulk
            .command(
                &flash_read_command(transport, addr),
                FLASH_REPLY_LEN,
                transport.timeout(),
            )
            .map_err(|e| tracing::debug!(addr, "flash read failed: {e}"))
            .ok()?;
        flash_block(&reply).map(<[u8]>::to_vec)
    };
    if let Some(b) = read(FLASH_IMU) {
        cal.gyro_bias = [4, 8, 12].map(|at| le_f32(&b, at).to_degrees());
        if !cal
            .gyro_bias
            .iter()
            .all(|v| v.is_finite() && v.abs() < 50.0)
        {
            cal.gyro_bias = [0.0; 3];
        }
    }
    if let Some(b) = read(FLASH_LEFT_STICK) {
        cal.left = stick_cal(&b[0x28..]);
    }
    if let Some(b) = read(FLASH_RIGHT_STICK) {
        cal.right = stick_cal(&b[0x28..]);
    }
    if model == Switch2Model::GameCube
        && let Some(b) = read(FLASH_GC_TRIGGERS)
    {
        cal.trigger_zero = [b[0], b[1]];
    }
    // User calibration. For the right stick, sources disagree between FLASH_USER_RIGHT
    // and offset 0x20 of the left stick's block (0x1FC060).
    let user = read(FLASH_USER_LEFT);
    if let Some(b) = &user
        && b[..2] == CAL_MAGIC
    {
        cal.left = stick_cal(&b[2..]);
    }
    let right = read(FLASH_USER_RIGHT)
        .filter(|b| b[..2] == CAL_MAGIC)
        .map(|b| b[2..11].to_vec())
        .or_else(|| {
            user.filter(|b| b[0x20..0x22] == CAL_MAGIC)
                .map(|b| b[0x22..0x2B].to_vec())
        });
    if let Some(b) = right {
        cal.right = stick_cal(&b);
    }
}

fn send(bulk: &mut dyn BulkIo, transport: Transport, cmd: &[u8]) -> io::Result<Vec<u8>> {
    bulk.command(cmd, 64, transport.timeout())
}

/// Read the calibration, then switch the pad's input and output on.
fn start(
    model: Switch2Model,
    transport: Transport,
    cal: &mut Calibration,
    bulk: &mut dyn BulkIo,
) -> Result<(), InitError> {
    read_calibration(model, transport, cal, bulk);
    let commands = match transport {
        Transport::Usb => init_commands(FEATURES),
        Transport::Bluetooth => bluetooth_init_commands(FEATURES),
    };
    let mut ok = true;
    for cmd in commands {
        if let Err(e) = send(bulk, transport, &cmd) {
            tracing::debug!(cmd = cmd[0], sub = cmd[3], "init command failed: {e}");
            ok = false;
            break;
        }
    }
    match (ok, transport) {
        (true, _) => {}
        (false, Transport::Usb) => {
            // Fall back to the two commands that matter.
            let _ = send(bulk, transport, &select_report_command());
            send(bulk, transport, &start_output_command())?;
        }
        // Input flows without them, only motion and rumble may be missing.
        (false, Transport::Bluetooth) => {
            tracing::warn!("Switch 2 controller: switching on motion and rumble failed");
        }
    }
    if let Err(e) = send(
        bulk,
        transport,
        &player_led_command(transport, FIRST_PLAYER),
    ) {
        tracing::debug!("player LED: {e}");
    }
    Ok(())
}

/// Wait for input that another program's initialisation already started.
fn wait_for_input(io: &mut dyn ReportIo) -> io::Result<bool> {
    let until = io.now() + FOREIGN_INIT_WAIT;
    let input = read_until(io, until, |r| {
        (r.first() == Some(&INPUT_REPORT)).then_some(())
    })?;
    Ok(input.is_some())
}

impl PadDriver for Switch2Driver {
    fn init(&mut self, io: &mut dyn ReportIo) -> Result<Identity, InitError> {
        let model = self.model;
        match &mut self.vendor {
            Vendor::Ready(bulk) => {
                start(model, self.transport, &mut self.cal, bulk.as_mut())?;
                Ok(identity(model, None))
            }
            Vendor::Missing { reason, late } => {
                if !wait_for_input(io)? {
                    return Err(InitError::Blocked(reason.clone()));
                }
                let until = if late.is_some() {
                    " until that interface is free"
                } else {
                    ""
                };
                Ok(identity(
                    model,
                    Some(format!(
                        "{reason}; it works, but with default stick calibration{until}"
                    )),
                ))
            }
        }
    }

    fn parse(&mut self, report: &[u8], now: Instant) -> Option<PadState> {
        if report.len() >= IN_LEN && report[0] == INPUT_REPORT {
            self.clock.update(imu_timestamp(report), now);
            self.gauge.update(report, now);
        }
        let scale = self
            .clock
            .scale
            .filter(|_| self.model != Switch2Model::GameCube);
        parse_report(self.model, &mut self.cal, scale, report)
    }

    fn battery(&self) -> Option<Battery> {
        self.gauge.battery()
    }

    fn set_leds(&mut self, leds: Leds) {
        self.lights.set(leds.player);
    }

    fn service(&mut self, io: &mut dyn ReportIo, rumble: Rumble) -> io::Result<()> {
        self.adopt_late_bulk();
        // The change waits until the vendor interface is there.
        if let Vendor::Ready(bulk) = &mut self.vendor
            && let Some(mask) = self.lights.take_change()
            && let Err(e) = send(
                bulk.as_mut(),
                self.transport,
                &player_led_command(self.transport, mask),
            )
        {
            tracing::debug!("player LED: {e}");
        }
        let Some(r) = self.pacer.poll(io.now(), rumble) else {
            return Ok(());
        };
        let seq = self.seq.step();
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
            rumble_report(self.model, seq, r, self.rumble_samples())
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
            gamecube_rumble_report(self.seq.get(), 2)
        } else {
            rumble_report(self.model, self.seq.get(), Rumble::default(), 1)
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
            flash_read_command(Transport::Usb, 0x0013_0080),
            [
                0x02, 0x91, 0x00, 0x01, 0x00, 0x08, 0x00, 0x00, 0, 0, 0, 0, 0x80, 0x00, 0x13, 0x00
            ]
        );
        // As a Pro Controller answered it over Bluetooth.
        assert_eq!(
            flash_read_command(Transport::Bluetooth, 0x0001_3080),
            [
                0x02, 0x91, 0x01, 0x01, 0x00, 0x08, 0x00, 0x00, 0, 0, 0, 0, 0x80, 0x30, 0x01, 0x00
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
        let full_low = Rumble {
            low: 1.0,
            high: 0.0,
        };
        let r = rumble_report(Switch2Model::Pro, 3, full_low, 1);
        assert_eq!(r[0], 0x02);
        assert_eq!(r[1], 0x53);
        assert_eq!(r[0x11..0x17], r[1..7], "second motor mirrors the first");
        let la = u32::from((u16::from(r[5]) >> 6) | u16::from(r[6]) << 2);
        assert_eq!(la, 29000 >> 6);
        assert!(r[7..0x11].iter().all(|&b| b == 0), "one sample");
        let jc = rumble_report(Switch2Model::JoyConLeft, 0, Rumble::default(), 1);
        assert_eq!(jc[0], 0x01);
        assert!(jc[0x11..].iter().all(|&b| b == 0));

        let r = rumble_report(Switch2Model::Pro, 3, full_low, 3);
        assert_eq!(r[2..7], r[7..12]);
        assert_eq!(r[2..7], r[12..17], "three samples");
        assert_eq!(r[0x11..0x21], r[1..0x11], "for both motors");
        assert!(r[0x21..].iter().all(|&b| b == 0));
    }

    #[test]
    fn bluetooth_start() {
        let (bulk, sent) = fake_bulk(Vec::new());
        let mut d = Switch2Driver::bluetooth(Switch2Model::Pro, Box::new(bulk));
        let mut io = ScriptIo::silent();
        let id = d.init(&mut io).unwrap();
        assert!(id.notice.is_none());
        let sent = sent.lock().unwrap().clone();
        assert!(
            sent.iter().all(|c| c[2] == 0x01),
            "every header says Bluetooth"
        );
        assert!(
            sent.iter().all(|c| c[0] != 0x03),
            "no USB start-up: input already flows"
        );
        let steps: Vec<[u8; 2]> = sent.iter().map(|c| [c[0], c[3]]).collect();
        let features = steps.iter().position(|s| *s == [0x0C, 0x02]).unwrap();
        assert_eq!(
            steps[features..],
            [[0x0C, 0x02], [0x0A, 0x08], [0x0C, 0x04], [0x09, 0x07]]
        );
        assert!(
            steps[..features].iter().all(|s| *s == [0x02, 0x01]),
            "calibration first"
        );
    }

    #[test]
    fn bluetooth_rumble() {
        let (bulk, _) = fake_bulk(Vec::new());
        let mut d = Switch2Driver::bluetooth(Switch2Model::Pro, Box::new(bulk));
        let mut io = ScriptIo::silent();
        let half = Rumble {
            low: 0.5,
            high: 0.5,
        };
        for _ in 0..4 {
            d.service(&mut io, half).unwrap();
            io.advance(15 * MS);
        }
        let writes = io.writes();
        assert_eq!(writes.len(), 4, "one report per 15 ms");
        assert_eq!(writes[0][2..7], writes[0][12..17], "three samples");
        for _ in 0..5 {
            d.service(&mut io, Rumble::default()).unwrap();
            io.advance(15 * MS);
        }
        let stops = io.writes()[4..]
            .iter()
            .filter(|w| w[2..7] == hd_sample(HIGH_FREQ, 0, LOW_FREQ, 0))
            .count();
        assert_eq!(stops, 3, "the stop goes out three times");
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

    #[test]
    fn charge_by_voltage() {
        assert_eq!(charge_percent(2900.0), 0.0);
        assert_eq!(charge_percent(3800.0), 100.0);
        assert!((charge_percent(3688.0) - 96.7).abs() < 0.1);
        let mut prev = -1.0;
        for mv in (2900..3800).step_by(10) {
            let p = charge_percent(mv as f32);
            assert!(p >= prev, "rises with the voltage");
            prev = p;
        }
    }

    fn battery_report(mv: u16, charge: u8) -> Vec<u8> {
        let mut r = report();
        r[IN_BATTERY..IN_BATTERY + 2].copy_from_slice(&mv.to_le_bytes());
        r[IN_CHARGE] = charge;
        r
    }

    #[test]
    fn battery_level() {
        let mut d = Switch2Driver::new(Switch2Model::Pro, Err(String::new()));
        let t = Instant::now();
        d.parse(&report(), t);
        assert_eq!(d.battery(), None, "no voltage, no battery");

        // As read from a Pro Controller on its battery, nearly full.
        d.parse(&battery_report(3688, 0), t);
        let b = d.battery().unwrap();
        assert_eq!((b.level, b.charging, b.wired), (95, false, false));
        // A rumble burst pulls the voltage down for a moment.
        d.parse(&battery_report(3550, 0), t + 30 * MS);
        assert_eq!(d.battery().unwrap().level, 95);
        // A slow change comes through.
        for i in 1..=200 {
            d.parse(&battery_report(3400, 0), t + i * 300 * MS);
        }
        assert_eq!(d.battery().unwrap().level, 55);
        d.parse(&battery_report(3450, 0x34), t + 61_000 * MS);
        let b = d.battery().unwrap();
        assert_eq!(
            (b.level, b.charging),
            (65, true),
            "plugged in: no smoothing over"
        );
        for i in 1..=200 {
            d.parse(
                &battery_report(3715, CHARGED),
                t + 61_000 * MS + i * 300 * MS,
            );
        }
        let b = d.battery().unwrap();
        assert_eq!((b.level, b.charging), (100, false), "charged");
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
