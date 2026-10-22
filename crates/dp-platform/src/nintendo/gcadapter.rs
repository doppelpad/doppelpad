//! The GameCube controller adapter (Wii U / Switch, and clones in that mode): four ports
//! in one USB device, streamed as one 37-byte report.

use dp_core::pad::{Axis, PadButton, PadState};

use super::{NINTENDO_VID, gc_trigger};

pub const ADAPTER_PID: u16 = 0x0337;
pub const PORTS: usize = 4;
/// Starts the input stream.
pub const START: [u8; 1] = [0x13];
const REPORT_ID: u8 = 0x21;
const REPORT_LEN: usize = 37;
const RUMBLE: u8 = 0x11;

pub fn is_adapter(vid: u16, pid: u16) -> bool {
    vid == NINTENDO_VID && pid == ADAPTER_PID
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PortKind {
    Wired,
    /// WaveBird: no rumble.
    Wireless,
}

/// Stick centres and resting triggers, taken when a controller appears.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct Origin {
    sticks: [u8; 4],
    triggers: [u8; 2],
}

/// One occupied port as of the last report.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Port {
    pub kind: PortKind,
    pub state: PadState,
}

/// The adapter's ports, kept across reports for their origins.
#[derive(Clone, Debug, Default)]
pub struct Adapter {
    origins: [Option<Origin>; PORTS],
}

/// Raw stick counts from the centre to the edge.
const STICK_RANGE: f32 = 100.0;

fn stick(v: u8, origin: u8) -> f32 {
    ((f32::from(v) - f32::from(origin)) / STICK_RANGE).clamp(-1.0, 1.0)
}

fn trigger(v: u8, origin: u8, click: bool) -> f32 {
    if click { 1.0 } else { gc_trigger(v, origin) }
}

fn port_state(b: &[u8], o: &Origin) -> PadState {
    use PadButton::*;
    let mut s = PadState::default();
    let (b1, b2) = (b[1], b[2]);
    // By the pad's geometry: big A south, B left of it, X right, Y on top.
    for (bit, button) in [
        (0x01, A),
        (0x02, X),
        (0x04, B),
        (0x08, Y),
        (0x10, DpadLeft),
        (0x20, DpadRight),
        (0x40, DpadDown),
        (0x80, DpadUp),
    ] {
        s.set(button, b1 & bit != 0);
    }
    s.set(Start, b2 & 0x01 != 0);
    s.set(RB, b2 & 0x02 != 0); // Z
    for (axis, i) in [
        (Axis::LeftX, 0),
        (Axis::LeftY, 1),
        (Axis::RightX, 2),
        (Axis::RightY, 3),
    ] {
        s.set_axis(axis, stick(b[3 + i], o.sticks[i]));
    }
    s.set_axis(
        Axis::LeftTrigger,
        trigger(b[7], o.triggers[0], b2 & 0x08 != 0),
    );
    s.set_axis(
        Axis::RightTrigger,
        trigger(b[8], o.triggers[1], b2 & 0x04 != 0),
    );
    s
}

impl Adapter {
    /// The ports of an input report (report ID first); `None` for other reports.
    pub fn parse(&mut self, report: &[u8]) -> Option<[Option<Port>; PORTS]> {
        if report.len() < REPORT_LEN || report[0] != REPORT_ID {
            return None;
        }
        let mut ports = [None; PORTS];
        for (i, port) in ports.iter_mut().enumerate() {
            let b = &report[1 + 9 * i..1 + 9 * (i + 1)];
            let kind = match b[0] {
                s if s & 0x10 != 0 => PortKind::Wired,
                s if s & 0x20 != 0 => PortKind::Wireless,
                _ => {
                    self.origins[i] = None;
                    continue;
                }
            };
            let origin = *self.origins[i].get_or_insert(Origin {
                sticks: [b[3], b[4], b[5], b[6]],
                triggers: [b[7], b[8]],
            });
            *port = Some(Port {
                kind,
                state: port_state(b, &origin),
            });
        }
        Some(ports)
    }
}

/// Motors on or off, ports 1–4.
pub fn rumble_report(on: [bool; PORTS]) -> [u8; 5] {
    let mut r = [RUMBLE, 0, 0, 0, 0];
    for (i, &m) in on.iter().enumerate() {
        r[1 + i] = u8::from(m);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(blocks: &[(usize, [u8; 9])]) -> Vec<u8> {
        let mut r = vec![0u8; REPORT_LEN];
        r[0] = REPORT_ID;
        for (port, b) in blocks {
            r[1 + 9 * port..1 + 9 * (port + 1)].copy_from_slice(b);
        }
        r
    }

    #[test]
    fn ports_with_origin() {
        let mut a = Adapter::default();
        let rest = [0x14, 0, 0, 130, 125, 128, 128, 30, 25];
        let ports = a.parse(&report(&[(1, rest)])).unwrap();
        assert!(ports[0].is_none() && ports[2].is_none());
        let p = ports[1].unwrap();
        assert_eq!(p.kind, PortKind::Wired);
        assert_eq!(
            p.state.axis(Axis::LeftX),
            0.0,
            "the first report is the origin"
        );

        let moved = [
            0x14,
            0x01 | 0x02 | 0x80,
            0x02 | 0x08,
            230,
            25,
            128,
            128,
            30,
            128,
        ];
        let s = a.parse(&report(&[(1, moved)])).unwrap()[1].unwrap().state;
        assert!(s.pressed(PadButton::A) && s.pressed(PadButton::X) && s.pressed(PadButton::DpadUp));
        assert!(s.pressed(PadButton::RB), "Z");
        assert_eq!(s.axis(Axis::LeftX), 1.0);
        assert_eq!(s.axis(Axis::LeftY), -1.0);
        assert_eq!(s.axis(Axis::LeftTrigger), 1.0, "L click");
        let r = s.axis(Axis::RightTrigger);
        assert!((r - 103.0 / 207.0).abs() < 0.01, "{r}");

        // Unplugged and plugged in again: a new origin.
        a.parse(&report(&[]));
        let s = a.parse(&report(&[(1, moved)])).unwrap()[1].unwrap().state;
        assert_eq!(s.axis(Axis::LeftX), 0.0);
    }

    #[test]
    fn truncated_reports_do_not_panic() {
        let pad = [0x14, 0xFF, 0x0F, 230, 25, 128, 128, 200, 200];
        let r = report(&[(0, pad), (1, pad), (2, pad), (3, pad)]);
        assert!(
            Adapter::default()
                .parse(&r)
                .unwrap()
                .iter()
                .all(Option::is_some)
        );
        let mut a = Adapter::default();
        for n in 0..=r.len() {
            a.parse(&r[..n]);
        }
    }

    #[test]
    fn wavebird_and_other_reports() {
        let mut a = Adapter::default();
        let ports = a
            .parse(&report(&[(3, [0x20, 0, 0, 128, 128, 128, 128, 0, 0])]))
            .unwrap();
        assert_eq!(ports[3].unwrap().kind, PortKind::Wireless);
        assert_eq!(a.parse(&[0x11, 0, 0]), None);
        assert_eq!(
            rumble_report([true, false, false, true]),
            [0x11, 1, 0, 0, 1]
        );
    }
}
