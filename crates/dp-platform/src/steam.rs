//! Valve controllers on their vendor HID interface: the Steam Controller (USB cable and
//! wireless receiver) and the Steam Deck's built-in controls.
//!
//! Out of the box they pretend to be a keyboard and mouse ("lizard mode"). The driver turns
//! that off with feature-report commands while it runs (the Deck needs reminding twice a
//! second) and gives it back when it stops.

use std::io;
use std::time::{Duration, Instant};

use dp_core::pad::{Axis, Motion, PadButton, PadState, Rumble, TouchPoint, axis_from_i16};

use crate::padio::{Every, Identity, InitError, PadDriver, ReportIo, RumblePacer, earliest};
use crate::report::{le_i16, le_u16};
use crate::{Battery, PadFamily};

pub const VALVE_VID: u16 = 0x28DE;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SteamModel {
    /// Steam Controller on its USB cable.
    Controller,
    /// One slot of the Steam Controller's wireless receiver.
    Receiver,
    Deck,
}

pub fn model_for(vid: u16, pid: u16) -> Option<SteamModel> {
    match (vid, pid) {
        (VALVE_VID, 0x1102) => Some(SteamModel::Controller),
        (VALVE_VID, 0x1142) => Some(SteamModel::Receiver),
        (VALVE_VID, 0x1205) => Some(SteamModel::Deck),
        _ => None,
    }
}

const CMD_CLEAR_MAPPINGS: u8 = 0x81;
const CMD_DEFAULT_MAPPINGS: u8 = 0x85;
const CMD_SET_SETTINGS: u8 = 0x87;
const CMD_DEFAULT_SETTINGS: u8 = 0x8E;
const CMD_TURN_OFF: u8 = 0x9F;
const CMD_WIRELESS_STATE: u8 = 0xB4;
const CMD_RUMBLE: u8 = 0xEB;

const REG_LEFT_PAD_MODE: u8 = 0x07;
const REG_RIGHT_PAD_MODE: u8 = 0x08;
const REG_SMOOTH_MOUSE: u8 = 0x18;
const REG_IMU_MODE: u8 = 0x30;
const REG_LEFT_CLICK_PRESSURE: u8 = 0x34;
const REG_RIGHT_CLICK_PRESSURE: u8 = 0x35;

const PAD_MODE_NONE: u16 = 7;
const PAD_MODE_MOUSE: u16 = 0;
/// Raw accelerometer and raw gyro.
const IMU_RAW: u16 = 0x08 | 0x10;

const TYPE_STATE: u8 = 1;
const TYPE_LINK: u8 = 3;
const TYPE_BATTERY: u8 = 4;
const TYPE_DECK_STATE: u8 = 9;

const LINK_CONNECTED: u8 = 2;
const LINK_DISCONNECTED: u8 = 1;

/// The Deck brings lizard mode back unless it hears from us.
const DECK_FEED: Duration = Duration::from_millis(500);
/// Full scale of the motion sensors: ±2000 °/s and ±2 g.
const GYRO_DPS_PER_LSB: f32 = 2000.0 / 32768.0;
const ACCEL_G_PER_LSB: f32 = 2.0 / 32768.0;
/// The Steam Controller's trigger counts as fully pulled here (of 15 bits).
const TRIGGER_FULL: f32 = 26000.0;

/// One command as a feature report: report ID 0, then `[cmd, len, payload…]`.
fn command(cmd: u8, payload: &[u8]) -> [u8; 65] {
    let mut buf = [0u8; 65];
    buf[1] = cmd;
    buf[2] = payload.len() as u8;
    buf[3..3 + payload.len()].copy_from_slice(payload);
    buf
}

/// The settings command for `(register, value)` pairs.
fn settings(values: &[(u8, u16)]) -> [u8; 65] {
    let payload: Vec<u8> = values
        .iter()
        .flat_map(|&(reg, v)| {
            let [lo, hi] = v.to_le_bytes();
            [reg, lo, hi]
        })
        .collect();
    command(CMD_SET_SETTINGS, &payload)
}

/// The payload without the report-ID byte Windows puts in front.
fn payload(report: &[u8]) -> Option<&[u8]> {
    let d = match report.first() {
        Some(0) => &report[1..],
        _ => report,
    };
    (d.len() >= 64 && d[0] == 0x01 && d[1] == 0x00).then_some(d)
}

/// A pad position as a touch point in its half of a shared surface, y from the top.
fn pad_point(d: &[u8], at: usize, right: bool) -> TouchPoint {
    let x = (f32::from(le_i16(d, at)) / 65536.0 + 0.5).clamp(0.0, 1.0);
    let y = (0.5 - f32::from(le_i16(d, at + 2)) / 65536.0).clamp(0.0, 1.0);
    TouchPoint {
        x: if right { 0.5 + x * 0.5 } else { x * 0.5 },
        y,
    }
}

fn motion(d: &[u8], accel_at: usize, gyro_at: usize, deck: bool) -> Motion {
    let v = |at: usize| f32::from(le_i16(d, at));
    let (ax, ay, az) = (v(accel_at), v(accel_at + 2), v(accel_at + 4));
    let (gx, gy, gz) = (v(gyro_at), v(gyro_at + 2), v(gyro_at + 4));
    // The controller's z points out of its face and y away from the player.
    let gyro_y = if deck { -gy } else { gy };
    Motion {
        gyro: [gx, gz, gyro_y].map(|g| g * GYRO_DPS_PER_LSB),
        accel: [ax, az, -ay].map(|a| a * ACCEL_G_PER_LSB),
    }
}

fn set_bits(s: &mut PadState, byte: u8, bits: &[(u8, PadButton)]) {
    for &(bit, b) in bits {
        if byte & bit != 0 {
            s.set(b, true);
        }
    }
}

/// Bytes 8 and 9, shared by both reports.
fn common_buttons(s: &mut PadState, d: &[u8]) {
    use PadButton::*;
    set_bits(
        s,
        d[8],
        &[
            (0x04, RB),
            (0x08, LB),
            (0x10, Y),
            (0x20, B),
            (0x40, X),
            (0x80, A),
        ],
    );
    set_bits(
        s,
        d[9],
        &[
            (0x01, DpadUp),
            (0x02, DpadRight),
            (0x04, DpadLeft),
            (0x08, DpadDown),
            (0x10, Back),
            (0x20, Guide),
            (0x40, Start),
        ],
    );
}

/// What the Steam Controller last reported for its shared stick / left-pad values.
#[derive(Clone, Copy, Debug, Default)]
struct Shared {
    stick: (f32, f32),
    left_pad: Option<TouchPoint>,
}

fn parse_controller(d: &[u8], last: &mut Shared) -> PadState {
    let mut s = PadState::default();
    common_buttons(&mut s, d);
    let (b9, b10) = (d[9], d[10]);
    set_bits(&mut s, b9, &[(0x80, PadButton::PaddleUL)]);
    set_bits(
        &mut s,
        b10,
        &[
            (0x01, PadButton::PaddleUR),
            (0x04, PadButton::Touchpad),
            (0x40, PadButton::LS),
        ],
    );
    let trigger = |v: u8| (((u32::from(v) << 7) | u32::from(v)) as f32 / TRIGGER_FULL).min(1.0);
    s.set_axis(Axis::LeftTrigger, trigger(d[11]));
    s.set_axis(Axis::RightTrigger, trigger(d[12]));
    // Stick and left pad share bytes 16–19; with both in use, reports alternate.
    let left_touched = b10 & 0x08 != 0;
    let both = b10 & 0x80 != 0;
    if left_touched {
        last.left_pad = Some(pad_point(d, 16, false));
        if !both {
            last.stick = (0.0, 0.0);
        }
    } else {
        last.stick = (axis_from_i16(le_i16(d, 16)), axis_from_i16(le_i16(d, 18)));
        if !both {
            last.left_pad = None;
        }
    }
    s.set_axis(Axis::LeftX, last.stick.0);
    s.set_axis(Axis::LeftY, last.stick.1);
    s.touch[0] = last.left_pad;
    // No right stick: the right pad stands in for it.
    if b10 & 0x10 != 0 {
        s.touch[1] = Some(pad_point(d, 20, true));
        s.set_axis(Axis::RightX, axis_from_i16(le_i16(d, 20)));
        s.set_axis(Axis::RightY, axis_from_i16(le_i16(d, 22)));
    }
    s.motion = Some(motion(d, 28, 34, false));
    s
}

fn parse_deck(d: &[u8]) -> PadState {
    use PadButton::*;
    let mut s = PadState::default();
    common_buttons(&mut s, d);
    set_bits(&mut s, d[9], &[(0x80, PaddleLL)]);
    set_bits(
        &mut s,
        d[10],
        &[
            (0x01, PaddleLR),
            (0x02, Touchpad),
            (0x04, Touchpad),
            (0x40, LS),
        ],
    );
    set_bits(&mut s, d[11], &[(0x04, RS)]);
    set_bits(&mut s, d[13], &[(0x02, PaddleUL), (0x04, PaddleUR)]);
    set_bits(&mut s, d[14], &[(0x04, Misc)]);
    if d[10] & 0x08 != 0 {
        s.touch[0] = Some(pad_point(d, 16, false));
    }
    if d[10] & 0x10 != 0 {
        s.touch[1] = Some(pad_point(d, 20, true));
    }
    s.motion = Some(motion(d, 24, 30, true));
    s.set_axis(
        Axis::LeftTrigger,
        (f32::from(le_u16(d, 44)) / 32767.0).min(1.0),
    );
    s.set_axis(
        Axis::RightTrigger,
        (f32::from(le_u16(d, 46)) / 32767.0).min(1.0),
    );
    for (axis, at) in [
        (Axis::LeftX, 48),
        (Axis::LeftY, 50),
        (Axis::RightX, 52),
        (Axis::RightY, 54),
    ] {
        s.set_axis(axis, axis_from_i16(le_i16(d, at)));
    }
    s
}

pub struct SteamDriver {
    model: SteamModel,
    /// A controller is there (always, except for an empty receiver slot).
    connected: bool,
    /// Lizard mode still has to be turned off (start, or a controller joined the receiver).
    setup_pending: bool,
    fed: Every,
    last: Shared,
    battery: Option<Battery>,
    rumble: RumblePacer,
}

impl SteamDriver {
    pub fn new(model: SteamModel) -> Self {
        SteamDriver {
            model,
            connected: model != SteamModel::Receiver,
            setup_pending: true,
            fed: Every::new(DECK_FEED),
            last: Shared::default(),
            battery: None,
            rumble: RumblePacer::new(Duration::from_millis(50), None, 3),
        }
    }

    /// Stop the keyboard and mouse emulation (and turn the gyro on).
    fn lizard_off(&self, io: &mut dyn ReportIo) -> io::Result<()> {
        io.set_feature(&command(CMD_CLEAR_MAPPINGS, &[]))?;
        let values: &[(u8, u16)] = match self.model {
            SteamModel::Deck => &[
                (REG_SMOOTH_MOUSE, 0),
                (REG_LEFT_PAD_MODE, PAD_MODE_NONE),
                (REG_RIGHT_PAD_MODE, PAD_MODE_NONE),
                (REG_LEFT_CLICK_PRESSURE, 0xFFFF),
                (REG_RIGHT_CLICK_PRESSURE, 0xFFFF),
            ],
            _ => &[
                (REG_LEFT_PAD_MODE, PAD_MODE_NONE),
                (REG_RIGHT_PAD_MODE, PAD_MODE_NONE),
                (REG_SMOOTH_MOUSE, 0),
                (REG_IMU_MODE, IMU_RAW),
            ],
        };
        io.set_feature(&settings(values))
    }

    /// Keep the Deck from turning lizard mode back on.
    fn feed(io: &mut dyn ReportIo) -> io::Result<()> {
        io.set_feature(&command(CMD_CLEAR_MAPPINGS, &[]))?;
        io.set_feature(&settings(&[(REG_RIGHT_PAD_MODE, PAD_MODE_NONE)]))
    }

    /// Ask the receiver whether a controller is on this slot.
    fn ask_link(&mut self, io: &mut dyn ReportIo) -> io::Result<()> {
        io.set_feature(&command(CMD_WIRELESS_STATE, &[]))?;
        let mut reply = [0u8; 65];
        io.get_feature(&mut reply)?;
        // Report ID, command echo, length, state.
        if reply[1] == CMD_WIRELESS_STATE {
            self.connected = reply[3] == LINK_CONNECTED;
        }
        Ok(())
    }

    fn rumble_report(r: Rumble) -> [u8; 65] {
        let motor = |v: f32| (v.clamp(0.0, 1.0) * 65535.0) as u16;
        let (l, h) = (motor(r.low), motor(r.high));
        command(
            CMD_RUMBLE,
            &[
                0,
                0,
                0,
                l as u8,
                (l >> 8) as u8,
                h as u8,
                (h >> 8) as u8,
                2,
                0,
            ],
        )
    }
}

impl PadDriver for SteamDriver {
    fn init(&mut self, io: &mut dyn ReportIo) -> Result<Identity, InitError> {
        if self.model == SteamModel::Receiver
            && let Err(e) = self.ask_link(io)
        {
            tracing::debug!("cannot ask the Steam receiver for its controller: {e}");
        }
        let deck = self.model == SteamModel::Deck;
        let name = if deck {
            "Steam Deck"
        } else {
            "Steam Controller"
        };
        Ok(Identity {
            has_motion: true,
            has_touchpad: true,
            has_rumble: deck,
            ..Identity::new(name, PadFamily::Xbox)
        })
    }

    fn parse(&mut self, report: &[u8], _now: Instant) -> Option<PadState> {
        let d = payload(report)?;
        match d[2] {
            TYPE_STATE if self.model != SteamModel::Deck => {
                if !self.connected {
                    // Input means a controller is there, whatever the receiver said.
                    self.connected = true;
                    self.setup_pending = true;
                }
                Some(parse_controller(d, &mut self.last))
            }
            TYPE_DECK_STATE if self.model == SteamModel::Deck => Some(parse_deck(d)),
            TYPE_LINK => {
                match d[4] {
                    LINK_CONNECTED if !self.connected => {
                        self.connected = true;
                        self.setup_pending = true;
                    }
                    LINK_DISCONNECTED => {
                        self.connected = false;
                        self.battery = None;
                        self.last = Shared::default();
                    }
                    _ => {}
                }
                None
            }
            TYPE_BATTERY => {
                self.battery = Some(Battery {
                    level: d[14].min(100),
                    charging: false,
                    wired: false,
                });
                None
            }
            _ => None,
        }
    }

    fn connected(&self) -> bool {
        self.connected
    }

    fn battery(&self) -> Option<Battery> {
        self.battery
    }

    fn service(&mut self, io: &mut dyn ReportIo, rumble: Rumble) -> io::Result<()> {
        if !self.connected {
            return Ok(());
        }
        let now = io.now();
        if self.setup_pending {
            self.setup_pending = false;
            self.lizard_off(io)?;
            self.fed.ran(now);
        } else if self.model == SteamModel::Deck && self.fed.due(now) {
            Self::feed(io)?;
            self.fed.ran(now);
        }
        if self.model == SteamModel::Deck
            && let Some(r) = self.rumble.poll(now, rumble)
        {
            io.set_feature(&Self::rumble_report(r))?;
        }
        Ok(())
    }

    fn power_off(&mut self, io: &mut dyn ReportIo) -> io::Result<bool> {
        if self.model != SteamModel::Receiver || !self.connected {
            return Ok(false);
        }
        io.set_feature(&command(CMD_TURN_OFF, b"off!"))?;
        Ok(true)
    }

    fn next_deadline(&self) -> Option<Instant> {
        if self.model != SteamModel::Deck || !self.connected {
            return None;
        }
        earliest([self.fed.next(), self.rumble.next_deadline()])
    }

    fn shutdown(&mut self, io: &mut dyn ReportIo) {
        if !self.connected || !self.fed.done() {
            return;
        }
        let mut give_back = || -> io::Result<()> {
            if self.model == SteamModel::Deck {
                io.set_feature(&Self::rumble_report(Rumble::default()))?;
            }
            io.set_feature(&command(CMD_DEFAULT_MAPPINGS, &[]))?;
            io.set_feature(&command(CMD_DEFAULT_SETTINGS, &[]))?;
            if self.model != SteamModel::Deck {
                io.set_feature(&settings(&[(REG_RIGHT_PAD_MODE, PAD_MODE_MOUSE)]))?;
            }
            Ok(())
        };
        if let Err(e) = give_back() {
            tracing::debug!("cannot give the Steam controller its mouse back: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::padio::testing::ScriptIo;

    /// A report as Windows reads it: report ID 0, then `01 00 type len` and the payload.
    fn report(ty: u8, fill: impl FnOnce(&mut [u8])) -> Vec<u8> {
        let mut r = vec![0u8; 65];
        r[1] = 0x01;
        r[3] = ty;
        r[4] = 60;
        fill(&mut r[1..]);
        r
    }

    fn put(d: &mut [u8], at: usize, v: i16) {
        d[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }

    #[test]
    fn truncated_reports_do_not_panic() {
        let busy = |p: &mut [u8]| p[4..].fill(0xFF);
        let reports = [
            report(TYPE_STATE, busy),
            report(TYPE_DECK_STATE, busy),
            report(TYPE_LINK, |p| p[4] = LINK_CONNECTED),
            report(TYPE_LINK, |p| p[4] = LINK_DISCONNECTED),
            report(TYPE_BATTERY, busy),
        ];
        for model in [
            SteamModel::Controller,
            SteamModel::Receiver,
            SteamModel::Deck,
        ] {
            let mut d = SteamDriver::new(model);
            for r in &reports {
                // With the report ID byte (Windows) and without it.
                for r in [&r[..], &r[1..]] {
                    for n in 0..=r.len() {
                        d.parse(&r[..n], Instant::now());
                    }
                }
            }
        }
    }

    #[test]
    fn models() {
        assert_eq!(model_for(0x28DE, 0x1102), Some(SteamModel::Controller));
        assert_eq!(model_for(0x28DE, 0x1142), Some(SteamModel::Receiver));
        assert_eq!(model_for(0x28DE, 0x1205), Some(SteamModel::Deck));
        assert_eq!(
            model_for(0x28DE, 0x1106),
            None,
            "Bluetooth LE is not supported"
        );
    }

    #[test]
    fn controller_buttons_pads_and_motion() {
        let mut d = SteamDriver::new(SteamModel::Controller);
        let r = report(TYPE_STATE, |p| {
            p[8] = 0x80 | 0x08; // A, LB
            p[9] = 0x20 | 0x80; // Steam, left grip
            p[10] = 0x10 | 0x04; // right pad touched and clicked
            p[11] = 255;
            put(p, 16, 32767); // stick right
            put(p, 20, -32768); // right pad at its left edge
            put(p, 22, 32767); // and top
            put(p, 32, 16384); // accel z: 1 g out of the face
            put(p, 34, 16384); // gyro x: 1000 °/s
        });
        let s = d.parse(&r, Instant::now()).unwrap();
        use PadButton::*;
        for b in [A, LB, Guide, PaddleUL, Touchpad] {
            assert!(s.pressed(b), "{b:?}");
        }
        assert!(!s.pressed(B) && !s.pressed(LS));
        assert_eq!(s.axis(Axis::LeftTrigger), 1.0);
        assert_eq!(s.axis(Axis::LeftX), 1.0);
        assert_eq!(
            s.axis(Axis::RightX),
            -1.0,
            "the right pad stands in for the stick"
        );
        let t = s.touch[1].unwrap();
        assert!((t.x - 0.5).abs() < 0.01 && t.y < 0.01, "{t:?}");
        assert_eq!(s.touch[0], None);
        let m = s.motion.unwrap();
        assert!((m.gyro[0] - 1000.0).abs() < 0.1);
        assert!(
            (m.accel[1] - 1.0).abs() < 0.01,
            "face up reads 1 g on y: {m:?}"
        );
    }

    #[test]
    fn controller_left_pad_and_stick_alternate() {
        let mut d = SteamDriver::new(SteamModel::Controller);
        let stick = report(TYPE_STATE, |p| {
            p[10] = 0x80;
            put(p, 18, 32767);
        });
        let pad = report(TYPE_STATE, |p| {
            p[10] = 0x80 | 0x08;
            put(p, 16, 16384);
        });
        d.parse(&stick, Instant::now());
        let s = d.parse(&pad, Instant::now()).unwrap();
        assert_eq!(s.axis(Axis::LeftY), 1.0, "the stick keeps its last value");
        let t = s.touch[0].unwrap();
        assert!((t.x - 0.375).abs() < 0.01, "left half: {t:?}");
        let alone = report(TYPE_STATE, |p| p[10] = 0x08);
        let s = d.parse(&alone, Instant::now()).unwrap();
        assert_eq!(s.axis(Axis::LeftY), 0.0, "pad alone: the stick is centred");
    }

    #[test]
    fn deck_report() {
        let mut d = SteamDriver::new(SteamModel::Deck);
        let r = report(TYPE_DECK_STATE, |p| {
            p[9] = 0x80; // L5
            p[10] = 0x02 | 0x08; // left pad clicked and touched
            p[11] = 0x04; // R3
            p[13] = 0x04; // R4
            p[14] = 0x04; // quick access
            p[44..46].copy_from_slice(&32767u16.to_le_bytes());
            put(p, 50, -32768); // left stick down
            put(p, 32, 16384); // gyro y
        });
        let s = d.parse(&r, Instant::now()).unwrap();
        use PadButton::*;
        for b in [PaddleLL, Touchpad, RS, PaddleUR, Misc] {
            assert!(s.pressed(b), "{b:?}");
        }
        assert_eq!(s.axis(Axis::LeftTrigger), 1.0);
        assert_eq!(s.axis(Axis::LeftY), -1.0);
        assert!(s.touch[0].is_some() && s.touch[1].is_none());
        assert!((s.motion.unwrap().gyro[2] + 1000.0).abs() < 0.1);
        assert_eq!(
            d.parse(&report(TYPE_STATE, |_| {}), Instant::now()),
            None,
            "a Deck reads Deck reports only"
        );
    }

    #[test]
    fn lizard_mode_off_and_back() {
        let mut d = SteamDriver::new(SteamModel::Controller);
        let mut io = ScriptIo::silent();
        d.init(&mut io).unwrap();
        d.service(&mut io, Rumble::default()).unwrap();
        let sent = std::mem::take(&mut io.set_features);
        assert_eq!(sent[0][1], CMD_CLEAR_MAPPINGS);
        assert_eq!(sent[1][1..3], [CMD_SET_SETTINGS, 12]);
        assert_eq!(sent[1][3..6], [REG_LEFT_PAD_MODE, 7, 0]);
        assert_eq!(sent[1][12..15], [REG_IMU_MODE, 0x18, 0], "gyro on");
        d.service(&mut io, Rumble::default()).unwrap();
        assert!(io.set_features.is_empty(), "once");
        d.shutdown(&mut io);
        let cmds: Vec<u8> = io.set_features.iter().map(|f| f[1]).collect();
        assert_eq!(
            cmds,
            [CMD_DEFAULT_MAPPINGS, CMD_DEFAULT_SETTINGS, CMD_SET_SETTINGS]
        );
    }

    #[test]
    fn deck_is_fed_and_rumbles() {
        let mut d = SteamDriver::new(SteamModel::Deck);
        let mut io = ScriptIo::silent();
        d.init(&mut io).unwrap();
        d.service(&mut io, Rumble::default()).unwrap();
        io.set_features.clear();
        io.advance(Duration::from_millis(100));
        let r = Rumble {
            low: 1.0,
            high: 0.0,
        };
        d.service(&mut io, r).unwrap();
        let f = &io.set_features[0];
        assert_eq!(f[1..12], [CMD_RUMBLE, 9, 0, 0, 0, 0xFF, 0xFF, 0, 0, 2, 0]);
        io.set_features.clear();
        io.advance(DECK_FEED);
        d.service(&mut io, r).unwrap();
        let cmds: Vec<u8> = io.set_features.iter().map(|f| f[1]).collect();
        assert_eq!(cmds, [CMD_CLEAR_MAPPINGS, CMD_SET_SETTINGS]);
        assert!(d.next_deadline().is_some());
    }

    #[test]
    fn receiver_slot_follows_its_controller() {
        let mut d = SteamDriver::new(SteamModel::Receiver);
        let mut io = ScriptIo::silent();
        let mut reply = vec![0u8; 65];
        reply[1..4].copy_from_slice(&[CMD_WIRELESS_STATE, 1, LINK_DISCONNECTED]);
        io.feature_replies.push_back(reply);
        d.init(&mut io).unwrap();
        assert!(!d.connected());
        d.service(&mut io, Rumble::default()).unwrap();
        assert_eq!(io.set_features.len(), 1, "only the question");
        assert!(!d.power_off(&mut io).unwrap());

        d.parse(
            &report(TYPE_LINK, |p| p[4] = LINK_CONNECTED),
            Instant::now(),
        );
        assert!(d.connected());
        d.service(&mut io, Rumble::default()).unwrap();
        assert_eq!(
            io.set_features[1][1], CMD_CLEAR_MAPPINGS,
            "set up on connect"
        );
        d.parse(&report(TYPE_BATTERY, |p| p[14] = 64), Instant::now());
        assert_eq!(d.battery().unwrap().level, 64);
        assert!(d.power_off(&mut io).unwrap());
        assert_eq!(
            io.set_features.last().unwrap()[1..7],
            [CMD_TURN_OFF, 4, b'o', b'f', b'f', b'!']
        );

        d.parse(
            &report(TYPE_LINK, |p| p[4] = LINK_DISCONNECTED),
            Instant::now(),
        );
        assert!(!d.connected() && d.battery().is_none());
    }
}
