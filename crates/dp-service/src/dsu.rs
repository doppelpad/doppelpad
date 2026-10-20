//! A DSU ("cemuhook") server: emulators such as Cemu, Dolphin and Ryujinx read the
//! controllers' buttons, sticks and motion over UDP. Up to four controllers, one per slot.

use std::collections::HashMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use dp_core::pad::{Axis, PadButton, PadState};
use dp_platform::{Battery, PadSnapshot};
use serde::{Deserialize, Serialize};

pub const SLOTS: usize = 4;
const VERSION: u16 = 1001;
const MSG_VERSION: u32 = 0x10_0000;
const MSG_INFO: u32 = 0x10_0001;
const MSG_DATA: u32 = 0x10_0002;
/// A subscription ends when the client stops renewing it (clients renew about once a second).
const SUBSCRIPTION: Duration = Duration::from_secs(5);
/// Data goes out at most this often (the DualShock 4's own rate).
const SEND_EVERY: Duration = Duration::from_millis(4);
/// DualShock 4 touchpad size, which clients assume.
const TOUCH_W: f32 = 1919.0;
const TOUCH_H: f32 = 941.0;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DsuSettings {
    pub enabled: bool,
    /// Listen on all network interfaces, not only this computer.
    pub lan: bool,
    pub port: u16,
}

impl Default for DsuSettings {
    fn default() -> Self {
        DsuSettings {
            enabled: false,
            lan: false,
            port: 26760,
        }
    }
}

/// One slot's controller as the clients see it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotPad {
    pub mac: [u8; 6],
    pub state: PadState,
    pub battery: Option<Battery>,
    pub has_motion: bool,
    pub bluetooth: bool,
    /// Microseconds; changes only with new motion data.
    pub motion_us: u64,
    /// Ids of the two touches, new for every touch that begins.
    pub touch_ids: [u8; 2],
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                (c >> 1) ^ 0xEDB8_8320
            } else {
                c >> 1
            };
        }
    }
    !c
}

/// A server packet: header, message type, payload, with its CRC filled in.
fn packet(server_id: u32, msg: u32, payload: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(20 + payload.len());
    p.extend_from_slice(b"DSUS");
    p.extend_from_slice(&VERSION.to_le_bytes());
    p.extend_from_slice(&((4 + payload.len()) as u16).to_le_bytes());
    p.extend_from_slice(&[0; 4]);
    p.extend_from_slice(&server_id.to_le_bytes());
    p.extend_from_slice(&msg.to_le_bytes());
    p.extend_from_slice(payload);
    let crc = crc32(&p);
    p[8..12].copy_from_slice(&crc.to_le_bytes());
    p
}

fn battery_code(b: Option<Battery>) -> u8 {
    match b {
        None => 0x00,
        Some(b) if b.wired => 0xEF,
        Some(b) if b.charging => 0xEE,
        Some(b) => match b.level {
            95.. => 0x05,
            70.. => 0x04,
            40.. => 0x03,
            15.. => 0x02,
            _ => 0x01,
        },
    }
}

/// The 11 bytes every slot answer starts with.
fn shared_block(slot: u8, pad: Option<&SlotPad>) -> [u8; 11] {
    let mut b = [0u8; 11];
    b[0] = slot;
    if let Some(p) = pad {
        b[1] = 2; // connected
        b[2] = if p.has_motion { 2 } else { 1 };
        b[3] = if p.bluetooth { 2 } else { 1 };
        b[4..10].copy_from_slice(&p.mac);
        b[10] = battery_code(p.battery);
    }
    b
}

fn info_payload(slot: u8, pad: Option<&SlotPad>) -> [u8; 12] {
    let mut out = [0u8; 12];
    out[..11].copy_from_slice(&shared_block(slot, pad));
    out
}

fn stick(v: f32) -> u8 {
    (128.0 + v.clamp(-1.0, 1.0) * 127.0).round() as u8
}

/// The controller data answer (80 bytes of payload).
fn data_payload(slot: u8, pad: &SlotPad, packet_no: u32) -> [u8; 80] {
    use PadButton::*;
    let s = &pad.state;
    let mut out = [0u8; 80];
    out[..11].copy_from_slice(&shared_block(slot, Some(pad)));
    out[11] = 1;
    out[12..16].copy_from_slice(&packet_no.to_le_bytes());
    let bits = |pairs: &[(u8, bool)]| {
        pairs
            .iter()
            .fold(0u8, |m, &(b, on)| m | if on { b } else { 0 })
    };
    let (lt, rt) = (s.axis(Axis::LeftTrigger), s.axis(Axis::RightTrigger));
    out[16] = bits(&[
        (0x80, s.pressed(DpadLeft)),
        (0x40, s.pressed(DpadDown)),
        (0x20, s.pressed(DpadRight)),
        (0x10, s.pressed(DpadUp)),
        (0x08, s.pressed(Start)),
        (0x04, s.pressed(RS)),
        (0x02, s.pressed(LS)),
        (0x01, s.pressed(Back)),
    ]);
    out[17] = bits(&[
        (0x80, s.pressed(Y)),
        (0x40, s.pressed(B)),
        (0x20, s.pressed(A)),
        (0x10, s.pressed(X)),
        (0x08, s.pressed(RB)),
        (0x04, s.pressed(LB)),
        (0x02, rt > 0.5),
        (0x01, lt > 0.5),
    ]);
    out[18] = s.pressed(Guide) as u8;
    out[19] = s.pressed(Touchpad) as u8;
    out[20] = stick(s.axis(Axis::LeftX));
    out[21] = stick(s.axis(Axis::LeftY));
    out[22] = stick(s.axis(Axis::RightX));
    out[23] = stick(s.axis(Axis::RightY));
    let full = |b: PadButton| if s.pressed(b) { 255 } else { 0 };
    let analog = [
        full(DpadLeft),
        full(DpadDown),
        full(DpadRight),
        full(DpadUp),
        full(Y),
        full(B),
        full(A),
        full(X),
        full(RB),
        full(LB),
        (rt.clamp(0.0, 1.0) * 255.0) as u8,
        (lt.clamp(0.0, 1.0) * 255.0) as u8,
    ];
    out[24..36].copy_from_slice(&analog);
    for (i, t) in s.touch.iter().enumerate() {
        let at = 36 + 6 * i;
        if let Some(t) = t {
            out[at] = 1;
            out[at + 1] = pad.touch_ids[i];
            out[at + 2..at + 4].copy_from_slice(&((t.x * TOUCH_W) as u16).to_le_bytes());
            out[at + 4..at + 6].copy_from_slice(&((t.y * TOUCH_H) as u16).to_le_bytes());
        }
    }
    out[48..56].copy_from_slice(&pad.motion_us.to_le_bytes());
    if let Some(m) = s.motion {
        // DSU reads acceleration the other way round from our sensor convention, and
        // positive yaw and roll turn right.
        let accel = [-m.accel[0], -m.accel[1], -m.accel[2]];
        let gyro = [m.gyro[0], -m.gyro[1], -m.gyro[2]];
        for (i, v) in accel.iter().chain(&gyro).enumerate() {
            out[56 + 4 * i..60 + 4 * i].copy_from_slice(&v.to_le_bytes());
        }
    }
    out
}

/// What a client asked for.
#[derive(Clone, Debug, PartialEq)]
enum Request {
    Version,
    Info(Vec<u8>),
    /// Data for all slots, one slot, or the slot with this MAC.
    Data {
        slot: Option<u8>,
        mac: Option<[u8; 6]>,
    },
}

fn parse_request(p: &[u8]) -> Option<Request> {
    if p.len() < 20 || &p[0..4] != b"DSUC" {
        return None;
    }
    let len = u16::from_le_bytes([p[6], p[7]]) as usize;
    if p.len() < 16 + len || len < 4 {
        return None;
    }
    let p = &p[..16 + len];
    let mut zeroed = p.to_vec();
    zeroed[8..12].fill(0);
    if crc32(&zeroed) != u32::from_le_bytes([p[8], p[9], p[10], p[11]]) {
        return None;
    }
    let body = &p[20..];
    match u32::from_le_bytes([p[16], p[17], p[18], p[19]]) {
        MSG_VERSION => Some(Request::Version),
        MSG_INFO if body.len() >= 4 => {
            let n = i32::from_le_bytes([body[0], body[1], body[2], body[3]]).clamp(0, 4) as usize;
            let slots = body[4..]
                .iter()
                .take(n)
                .copied()
                .filter(|&s| (s as usize) < SLOTS);
            Some(Request::Info(slots.collect()))
        }
        MSG_DATA if body.len() >= 8 => {
            let flags = body[0];
            let mut mac = [0u8; 6];
            mac.copy_from_slice(&body[2..8]);
            Some(Request::Data {
                slot: (flags & 1 != 0).then_some(body[1]),
                mac: (flags & 2 != 0).then_some(mac),
            })
        }
        _ => None,
    }
}

/// One subscribed client.
struct Client {
    all_until: Option<Instant>,
    slot_until: [Option<Instant>; SLOTS],
    packets: u32,
}

impl Client {
    fn wants(&self, slot: usize, now: Instant) -> bool {
        self.all_until.is_some_and(|t| now < t) || self.slot_until[slot].is_some_and(|t| now < t)
    }

    fn alive(&self, now: Instant) -> bool {
        (0..SLOTS).any(|s| self.wants(s, now))
    }
}

#[derive(Default)]
struct Frame {
    pads: [Option<SlotPad>; SLOTS],
    seq: u64,
}

pub struct DsuServer {
    frame: Arc<Mutex<Frame>>,
    clients: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub addr: SocketAddr,
    /// Which controller sits in which slot.
    slots: [Option<String>; SLOTS],
    epoch: Instant,
    last: [Option<SlotPad>; SLOTS],
}

impl DsuServer {
    pub fn start(settings: DsuSettings) -> io::Result<DsuServer> {
        let ip = if settings.lan {
            Ipv4Addr::UNSPECIFIED
        } else {
            Ipv4Addr::LOCALHOST
        };
        let socket = UdpSocket::bind((ip, settings.port))?;
        socket.set_read_timeout(Some(SEND_EVERY))?;
        let addr = socket.local_addr()?;
        let frame: Arc<Mutex<Frame>> = Arc::default();
        let clients = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (f, c, s) = (frame.clone(), clients.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name("dp-dsu".into())
            .spawn(move || serve(socket, &f, &c, &s))?;
        tracing::info!(%addr, "DSU motion server listening");
        Ok(DsuServer {
            frame,
            clients,
            stop,
            thread: Some(thread),
            addr,
            slots: Default::default(),
            epoch: Instant::now(),
            last: [None; SLOTS],
        })
    }

    /// Clients that subscribed recently.
    pub fn clients(&self) -> usize {
        self.clients.load(Ordering::Relaxed)
    }

    /// Hand over the controllers' current state; the first four keep their slots while
    /// connected.
    pub fn publish(&mut self, pads: &[PadSnapshot]) {
        for slot in self.slots.iter_mut() {
            if slot
                .as_ref()
                .is_some_and(|id| !pads.iter().any(|p| &p.info.id.0 == id))
            {
                *slot = None;
            }
        }
        for p in pads {
            if self.slots.iter().flatten().any(|id| *id == p.info.id.0) {
                continue;
            }
            if let Some(free) = self.slots.iter_mut().find(|s| s.is_none()) {
                *free = Some(p.info.id.0.clone());
            }
        }
        let now_us = self.epoch.elapsed().as_micros() as u64;
        let mut pads_out = [None; SLOTS];
        for (i, slot) in self.slots.iter().enumerate() {
            let Some(p) = slot
                .as_ref()
                .and_then(|id| pads.iter().find(|p| &p.info.id.0 == id))
            else {
                self.last[i] = None;
                continue;
            };
            let last = self.last[i];
            let motion_us = match last {
                Some(l) if l.state.motion == p.state.motion => l.motion_us,
                _ => now_us,
            };
            let mut touch_ids = last.map_or([0, 1], |l| l.touch_ids);
            for (t, id) in touch_ids.iter_mut().enumerate() {
                let was = last.is_some_and(|l| l.state.touch[t].is_some());
                if p.state.touch[t].is_some() && !was {
                    *id = id.wrapping_add(2);
                }
            }
            let pad = SlotPad {
                mac: mac_of(&p.info.id.0),
                state: p.state,
                battery: p.battery,
                has_motion: p.info.has_motion,
                bluetooth: p.info.name.contains("Bluetooth"),
                motion_us,
                touch_ids,
            };
            self.last[i] = Some(pad);
            pads_out[i] = Some(pad);
        }
        let mut f = self.frame.lock().unwrap();
        if f.pads != pads_out {
            f.pads = pads_out;
            f.seq += 1;
        }
    }
}

impl Drop for DsuServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// A made-up but stable MAC address per controller (some clients register by MAC).
fn mac_of(id: &str) -> [u8; 6] {
    let h = crc32(id.as_bytes());
    let g = crc32(&h.to_le_bytes());
    // Locally administered, unicast.
    let b = h.to_le_bytes();
    [0x02, b[0], b[1], b[2], b[3], g as u8]
}

fn serve(socket: UdpSocket, frame: &Mutex<Frame>, count: &AtomicUsize, stop: &AtomicBool) {
    let server_id: u32 = rand_id();
    let mut clients: HashMap<SocketAddr, Client> = HashMap::new();
    let mut buf = [0u8; 1024];
    let mut sent_seq = 0;
    let mut sent_at = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        if let Ok((n, from)) = socket.recv_from(&mut buf) {
            let now = Instant::now();
            let pads = frame.lock().unwrap().pads;
            match parse_request(&buf[..n]) {
                Some(Request::Version) => {
                    let _ = socket.send_to(
                        &packet(server_id, MSG_VERSION, &VERSION.to_le_bytes()),
                        from,
                    );
                }
                Some(Request::Info(slots)) => {
                    for s in slots {
                        let p = info_payload(s, pads[s as usize].as_ref());
                        let _ = socket.send_to(&packet(server_id, MSG_INFO, &p), from);
                    }
                }
                Some(Request::Data { slot, mac }) => {
                    let c = clients.entry(from).or_insert(Client {
                        all_until: None,
                        slot_until: [None; SLOTS],
                        packets: 0,
                    });
                    let until = Some(now + SUBSCRIPTION);
                    let by_mac =
                        mac.and_then(|m| pads.iter().position(|p| p.is_some_and(|p| p.mac == m)));
                    match (slot, by_mac) {
                        (None, None) if mac.is_none() => c.all_until = until,
                        (s, m) => {
                            for i in s.map(usize::from).into_iter().chain(m) {
                                if i < SLOTS {
                                    c.slot_until[i] = until;
                                }
                            }
                        }
                    }
                    // A new subscriber gets the current state at once.
                    sent_seq = 0;
                }
                None => {}
            }
        }
        let now = Instant::now();
        if now < sent_at + SEND_EVERY {
            continue;
        }
        clients.retain(|_, c| c.alive(now));
        count.store(clients.len(), Ordering::Relaxed);
        let (pads, seq) = {
            let f = frame.lock().unwrap();
            (f.pads, f.seq)
        };
        if seq == sent_seq {
            continue;
        }
        sent_seq = seq;
        sent_at = now;
        for (addr, c) in clients.iter_mut() {
            for (i, p) in pads.iter().enumerate() {
                if let Some(p) = p
                    && c.wants(i, now)
                {
                    c.packets = c.packets.wrapping_add(1);
                    let data = data_payload(i as u8, p, c.packets);
                    let _ = socket.send_to(&packet(server_id, MSG_DATA, &data), addr);
                }
            }
        }
    }
}

/// A server id that differs from run to run.
fn rand_id() -> u32 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    crc32(&t.as_nanos().to_le_bytes()) ^ std::process::id()
}

#[cfg(test)]
mod tests {
    use dp_core::pad::{Motion, TouchPoint};

    use super::*;

    fn request(msg: u32, body: &[u8]) -> Vec<u8> {
        let mut p = packet(7, msg, body);
        p[0..4].copy_from_slice(b"DSUC");
        p[8..12].fill(0);
        let crc = crc32(&p);
        p[8..12].copy_from_slice(&crc.to_le_bytes());
        p
    }

    fn pad() -> SlotPad {
        let mut state = PadState::default();
        state.set(PadButton::A, true);
        state.set(PadButton::DpadLeft, true);
        state.set_axis(Axis::LeftY, 1.0);
        state.set_axis(Axis::RightTrigger, 1.0);
        state.motion = Some(Motion {
            gyro: [10.0, 20.0, 30.0],
            accel: [0.0, 1.0, 0.0],
        });
        state.touch[0] = Some(TouchPoint { x: 0.5, y: 1.0 });
        SlotPad {
            mac: [2, 1, 2, 3, 4, 5],
            state,
            battery: Some(Battery {
                level: 80,
                charging: false,
                wired: false,
            }),
            has_motion: true,
            bluetooth: true,
            motion_us: 1234,
            touch_ids: [9, 1],
        }
    }

    #[test]
    fn crc_and_requests() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(
            parse_request(&request(MSG_VERSION, &[])),
            Some(Request::Version)
        );
        let info = request(MSG_INFO, &[2, 0, 0, 0, 0, 3]);
        assert_eq!(parse_request(&info), Some(Request::Info(vec![0, 3])));
        let all = request(MSG_DATA, &[0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            parse_request(&all),
            Some(Request::Data {
                slot: None,
                mac: None
            })
        );
        let one = request(MSG_DATA, &[1, 2, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            parse_request(&one),
            Some(Request::Data {
                slot: Some(2),
                mac: None
            })
        );
        let mut bad = one.clone();
        bad[21] = 3;
        assert_eq!(parse_request(&bad), None, "CRC mismatch");
    }

    #[test]
    fn data_packet_layout() {
        let p = packet(1, MSG_DATA, &data_payload(1, &pad(), 5));
        assert_eq!(p.len(), 100);
        assert_eq!(&p[0..4], b"DSUS");
        assert_eq!(u16::from_le_bytes([p[6], p[7]]), 84);
        let d = &p[20..];
        assert_eq!(
            &d[0..4],
            &[1, 2, 2, 2],
            "slot, connected, full gyro, Bluetooth"
        );
        assert_eq!(d[10], 0x04, "high battery");
        assert_eq!(u32::from_le_bytes([d[12], d[13], d[14], d[15]]), 5);
        assert_eq!(d[16], 0x80, "D-pad left");
        assert_eq!(d[17], 0x20 | 0x02, "A and R2");
        assert_eq!((d[20], d[21]), (128, 255), "left stick up");
        assert_eq!(d[34], 255, "R2 analog");
        assert_eq!(&d[36..38], &[1, 9], "touch active with its id");
        assert_eq!(u16::from_le_bytes([d[40], d[41]]), 941);
        assert_eq!(u64::from_le_bytes(d[48..56].try_into().unwrap()), 1234);
        let f = |at: usize| f32::from_le_bytes(d[at..at + 4].try_into().unwrap());
        assert_eq!(f(60), -1.0, "lying flat reads -1 g on y");
        assert_eq!((f(68), f(72), f(76)), (10.0, -20.0, -30.0));
    }

    #[test]
    fn serves_a_subscriber() {
        let mut server = DsuServer::start(DsuSettings {
            enabled: true,
            lan: false,
            port: 0,
        })
        .unwrap();
        let client = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let snap = dp_platform::PadSnapshot {
            info: Arc::new(dp_platform::DeviceInfo {
                id: dp_platform::DeviceId("pad1".into()),
                name: "Pad (USB)".into(),
                family: dp_platform::PadFamily::DualSense,
                vendor_id: 0,
                product_id: 0,
                backend: "fake".into(),
                instance_paths: vec![],
                joycon: None,
                has_motion: true,
                has_touchpad: true,
                has_rumble: true,
            }),
            state: pad().state,
            battery: None,
            raw: None,
        };
        server.publish(std::slice::from_ref(&snap));
        client
            .send_to(&request(MSG_INFO, &[1, 0, 0, 0, 0]), server.addr)
            .unwrap();
        let mut buf = [0u8; 128];
        let n = client.recv(&mut buf).unwrap();
        assert_eq!(n, 32);
        assert_eq!(buf[21], 2, "slot 0 connected");
        client
            .send_to(&request(MSG_DATA, &[0; 8]), server.addr)
            .unwrap();
        let n = client.recv(&mut buf).unwrap();
        assert_eq!(n, 100);
        assert_eq!(buf[20 + 17] & 0x20, 0x20, "A");
        assert_eq!(buf[20 + 3], 1, "USB");
    }
}
