//! The client side of the Attribute Protocol (ATT), as far as Switch 2 controllers need it:
//! exchanging the MTU, finding a service's characteristics and their configuration
//! descriptors, switching notifications on, and telling notifications apart from what else
//! the controller sends.
//!
//! BlueZ's own GATT client never finishes reading a Switch 2 controller (its Generic Attribute
//! service is empty, and a request goes unanswered until ATT's 30 s timeout), so Doppelpad
//! speaks ATT itself on the controller's ATT channel (see `ble`). Everything here works on
//! PDUs through a [`Bearer`], so it is unit-tested on every OS.

use std::io;
use std::time::Instant;

/// The fixed L2CAP channel ATT runs on over Bluetooth LE.
pub const CID: u16 = 4;
/// The MTU Doppelpad offers: the largest ATT allows. A Pro Controller 2 answers 512.
pub const MTU: u16 = 517;
/// The MTU before the exchange, which an input report (63 bytes) does not fit.
const DEFAULT_MTU: u16 = 23;

mod op {
    pub const ERROR: u8 = 0x01;
    pub const MTU_REQ: u8 = 0x02;
    pub const MTU_RSP: u8 = 0x03;
    pub const FIND_INFO_REQ: u8 = 0x04;
    pub const FIND_INFO_RSP: u8 = 0x05;
    pub const READ_BY_TYPE_REQ: u8 = 0x08;
    pub const READ_BY_TYPE_RSP: u8 = 0x09;
    pub const READ_BY_GROUP_REQ: u8 = 0x10;
    pub const READ_BY_GROUP_RSP: u8 = 0x11;
    pub const WRITE_REQ: u8 = 0x12;
    pub const WRITE_RSP: u8 = 0x13;
    pub const NOTIFICATION: u8 = 0x1B;
    pub const INDICATION: u8 = 0x1D;
    pub const CONFIRMATION: u8 = 0x1E;
    pub const WRITE_CMD: u8 = 0x52;
    /// Set in commands, which get no response.
    pub const COMMAND_FLAG: u8 = 0x40;
}

/// Error codes of an error response.
const ATTRIBUTE_NOT_FOUND: u8 = 0x0A;
const REQUEST_NOT_SUPPORTED: u8 = 0x06;

/// GATT's attribute types.
const PRIMARY_SERVICE: u16 = 0x2800;
const CHARACTERISTIC: u16 = 0x2803;
const CLIENT_CONFIGURATION: u16 = 0x2902;

/// A UUID as a number, read as its text form reads (see
/// [`crate::nintendo::switch2ble::parse_uuid`]).
pub type Uuid = u128;

/// The Bluetooth base UUID, which 16-bit UUIDs stand in for.
const BASE: Uuid = 0x0000_0000_0000_1000_8000_0080_5F9B_34FB;

fn short_uuid(u: u16) -> Uuid {
    BASE | Uuid::from(u) << 96
}

/// A UUID as ATT carries it: 2 or 16 bytes, little-endian.
fn read_uuid(le: &[u8]) -> Option<Uuid> {
    match le.len() {
        2 => Some(short_uuid(u16::from_le_bytes([le[0], le[1]]))),
        16 => Some(Uuid::from_le_bytes(le.try_into().ok()?)),
        _ => None,
    }
}

fn u16_at(pdu: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*pdu.get(at)?, *pdu.get(at + 1)?]))
}

/// Where PDUs go to the controller and come from it.
pub trait Bearer {
    fn send(&mut self, pdu: &[u8]) -> io::Result<()>;
    /// The next PDU, or `None` once `until` has passed.
    fn recv(&mut self, until: Instant) -> io::Result<Option<Vec<u8>>>;
}

/// What the controller sent, apart from responses to our requests.
#[derive(Debug, PartialEq, Eq)]
pub enum Incoming<'a> {
    Notification {
        handle: u16,
        value: &'a [u8],
    },
    /// A request or indication from the controller, with what to send back.
    Answer(Vec<u8>),
    /// A response, or a command, which needs nothing.
    Other,
}

/// Tell a PDU from the controller apart. Doppelpad offers the controller no attributes, so
/// its requests are refused; indications are confirmed.
pub fn incoming(pdu: &[u8]) -> Incoming<'_> {
    let Some(&opcode) = pdu.first() else {
        return Incoming::Other;
    };
    match opcode {
        op::NOTIFICATION => match u16_at(pdu, 1) {
            Some(handle) => Incoming::Notification {
                handle,
                value: &pdu[3..],
            },
            None => Incoming::Other,
        },
        op::INDICATION => Incoming::Answer(vec![op::CONFIRMATION]),
        op::MTU_REQ => {
            let mut answer = vec![op::MTU_RSP];
            answer.extend(MTU.to_le_bytes());
            Incoming::Answer(answer)
        }
        // Requests have even opcodes below the command flag; responses odd ones.
        o if o & op::COMMAND_FLAG == 0 && o % 2 == 0 => {
            Incoming::Answer(vec![op::ERROR, o, 0, 0, REQUEST_NOT_SUPPORTED])
        }
        _ => Incoming::Other,
    }
}

/// A Write Command: a write without response.
pub fn write_command(handle: u16, value: &[u8]) -> Vec<u8> {
    let mut pdu = vec![op::WRITE_CMD];
    pdu.extend(handle.to_le_bytes());
    pdu.extend_from_slice(value);
    pdu
}

/// Send a request and wait for its response (`want`) or an error response, answering the
/// controller meanwhile. `Ok(None)`: "attribute not found", which ends a search.
fn request(
    b: &mut dyn Bearer,
    pdu: &[u8],
    want: u8,
    until: Instant,
) -> io::Result<Option<Vec<u8>>> {
    b.send(pdu)?;
    loop {
        let Some(answer) = b.recv(until)? else {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("no answer to ATT request {:#04x}", pdu[0]),
            ));
        };
        match answer.first() {
            Some(&o) if o == want => return Ok(Some(answer)),
            Some(&op::ERROR) if answer.get(1) == Some(&pdu[0]) => {
                return match answer.get(4) {
                    Some(&ATTRIBUTE_NOT_FOUND) => Ok(None),
                    code => Err(io::Error::other(format!(
                        "ATT request {:#04x} refused: {code:02x?}",
                        pdu[0]
                    ))),
                };
            }
            _ => {
                if let Incoming::Answer(reply) = incoming(&answer) {
                    b.send(&reply)?;
                }
            }
        }
    }
}

fn malformed(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("malformed {what}"))
}

/// Exchange the MTU; the one both sides can use.
pub fn exchange_mtu(b: &mut dyn Bearer, until: Instant) -> io::Result<u16> {
    let mut pdu = vec![op::MTU_REQ];
    pdu.extend(MTU.to_le_bytes());
    let answer = request(b, &pdu, op::MTU_RSP, until)?;
    let theirs = answer
        .as_deref()
        .and_then(|a| u16_at(a, 1))
        .unwrap_or(DEFAULT_MTU);
    Ok(theirs.clamp(DEFAULT_MTU, MTU))
}

/// A primary service's handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Service {
    pub start: u16,
    pub end: u16,
    pub uuid: Uuid,
}

/// Every primary service.
pub fn services(b: &mut dyn Bearer, until: Instant) -> io::Result<Vec<Service>> {
    let mut out = Vec::new();
    let mut start = 1u16;
    loop {
        let mut pdu = vec![op::READ_BY_GROUP_REQ];
        pdu.extend(start.to_le_bytes());
        pdu.extend(0xFFFF_u16.to_le_bytes());
        pdu.extend(PRIMARY_SERVICE.to_le_bytes());
        let Some(answer) = request(b, &pdu, op::READ_BY_GROUP_RSP, until)? else {
            return Ok(out);
        };
        let len = usize::from(*answer.get(1).ok_or_else(|| malformed("service list"))?);
        if len < 6 {
            return Err(malformed("service list"));
        }
        let mut last = start;
        for entry in answer[2..].chunks_exact(len) {
            let (s, e) = (u16_at(entry, 0), u16_at(entry, 2));
            let (Some(s), Some(e), Some(uuid)) = (s, e, read_uuid(&entry[4..])) else {
                return Err(malformed("service"));
            };
            out.push(Service {
                start: s,
                end: e,
                uuid,
            });
            last = e;
        }
        if last == 0xFFFF || last < start {
            return Ok(out);
        }
        start = last + 1;
    }
}

/// A characteristic: where it is declared, what it allows, where its value is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Characteristic {
    pub declaration: u16,
    pub properties: u8,
    pub value: u16,
    pub uuid: Uuid,
}

/// The characteristics of a service (`start..=end`), in order, until `enough` says so.
pub fn characteristics(
    b: &mut dyn Bearer,
    start: u16,
    end: u16,
    enough: impl Fn(&[Characteristic]) -> bool,
    until: Instant,
) -> io::Result<Vec<Characteristic>> {
    let mut out: Vec<Characteristic> = Vec::new();
    let mut from = start;
    while from <= end && !enough(&out) {
        let mut pdu = vec![op::READ_BY_TYPE_REQ];
        pdu.extend(from.to_le_bytes());
        pdu.extend(end.to_le_bytes());
        pdu.extend(CHARACTERISTIC.to_le_bytes());
        let Some(answer) = request(b, &pdu, op::READ_BY_TYPE_RSP, until)? else {
            break;
        };
        let len = usize::from(*answer.get(1).ok_or_else(|| malformed("characteristics"))?);
        if len < 7 {
            return Err(malformed("characteristics"));
        }
        let before = out.len();
        for entry in answer[2..].chunks_exact(len) {
            let parsed = (u16_at(entry, 0), u16_at(entry, 3), read_uuid(&entry[5..]));
            let (Some(declaration), Some(value), Some(uuid)) = parsed else {
                return Err(malformed("characteristic"));
            };
            if declaration < from {
                return Err(malformed("characteristics"));
            }
            out.push(Characteristic {
                declaration,
                properties: entry[2],
                value,
                uuid,
            });
            from = declaration.saturating_add(1);
        }
        if out.len() == before || from == 0xFFFF {
            break;
        }
    }
    Ok(out)
}

/// The client configuration descriptor of the characteristic whose value is at `value`: the
/// first one after it, before the next characteristic or `end`.
pub fn configuration(
    b: &mut dyn Bearer,
    value: u16,
    end: u16,
    until: Instant,
) -> io::Result<Option<u16>> {
    let mut from = value.saturating_add(1);
    while from <= end && from > value {
        let mut pdu = vec![op::FIND_INFO_REQ];
        pdu.extend(from.to_le_bytes());
        pdu.extend(end.to_le_bytes());
        let Some(answer) = request(b, &pdu, op::FIND_INFO_RSP, until)? else {
            return Ok(None);
        };
        let len = match answer.get(1) {
            Some(1) => 4,
            Some(2) => 18,
            _ => return Err(malformed("descriptors")),
        };
        let mut last = from;
        for entry in answer[2..].chunks_exact(len) {
            let (Some(handle), Some(kind)) = (u16_at(entry, 0), read_uuid(&entry[2..])) else {
                return Err(malformed("descriptor"));
            };
            if kind == short_uuid(CLIENT_CONFIGURATION) {
                return Ok(Some(handle));
            }
            if kind == short_uuid(CHARACTERISTIC) {
                return Ok(None);
            }
            last = handle;
        }
        from = last.saturating_add(1);
    }
    Ok(None)
}

/// Switch a characteristic's notifications on through its configuration descriptor.
pub fn notify(b: &mut dyn Bearer, configuration: u16, until: Instant) -> io::Result<()> {
    let mut pdu = vec![op::WRITE_REQ];
    pdu.extend(configuration.to_le_bytes());
    pdu.extend(1u16.to_le_bytes());
    match request(b, &pdu, op::WRITE_RSP, until)? {
        Some(_) => Ok(()),
        None => Err(io::Error::other("the controller has no such descriptor")),
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::collections::VecDeque;

    use super::*;

    /// A controller that answers each request with the next scripted PDUs, and remembers what
    /// it was sent.
    #[derive(Default)]
    pub struct ScriptBearer {
        /// What arrives after each send, in order.
        pub answers: VecDeque<Vec<Vec<u8>>>,
        pending: VecDeque<Vec<u8>>,
        pub sent: Vec<Vec<u8>>,
    }

    impl ScriptBearer {
        pub fn new(answers: Vec<Vec<Vec<u8>>>) -> Self {
            ScriptBearer {
                answers: answers.into(),
                ..Default::default()
            }
        }
    }

    impl Bearer for ScriptBearer {
        fn send(&mut self, pdu: &[u8]) -> io::Result<()> {
            self.sent.push(pdu.to_vec());
            // Confirmations and refusals get nothing back.
            if !matches!(
                pdu.first(),
                Some(&op::CONFIRMATION | &op::ERROR | &op::MTU_RSP)
            ) {
                self.pending
                    .extend(self.answers.pop_front().unwrap_or_default());
            }
            Ok(())
        }

        fn recv(&mut self, _until: Instant) -> io::Result<Option<Vec<u8>>> {
            Ok(self.pending.pop_front())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::testing::ScriptBearer;
    use super::*;

    fn soon() -> Instant {
        Instant::now() + Duration::from_secs(1)
    }

    fn uuid(text: &str) -> Option<Uuid> {
        crate::nintendo::switch2ble::parse_uuid(text)
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn uuids() {
        let service = uuid("AB7DE9BE-89FE-49AD-828F-118F09DF7FD0").unwrap();
        let le = service.to_le_bytes();
        assert_eq!((le[15], le[0]), (0xAB, 0xD0), "on the air backwards");
        assert_eq!(read_uuid(&le), Some(service));
        assert_eq!(
            read_uuid(&[0x00, 0x18]),
            uuid("00001800-0000-1000-8000-00805F9B34FB")
        );
        assert_eq!(read_uuid(&[0x00]), None);
    }

    #[test]
    fn what_the_controller_sends() {
        assert_eq!(
            incoming(&[0x1B, 0x0A, 0x00, 0xAA, 0xBB]),
            Incoming::Notification {
                handle: 0x000A,
                value: &[0xAA, 0xBB]
            }
        );
        assert_eq!(
            incoming(&[0x1D, 0x03, 0x00, 0x01]),
            Incoming::Answer(vec![0x1E])
        );
        assert_eq!(
            incoming(&[0x02, 0x00, 0x02]),
            Incoming::Answer(vec![0x03, 0x05, 0x02])
        );
        assert_eq!(
            incoming(&[0x10, 0x01, 0x00, 0xFF, 0xFF, 0x00, 0x28]),
            Incoming::Answer(vec![0x01, 0x10, 0x00, 0x00, 0x06]),
            "a service search of ours is refused"
        );
        assert_eq!(incoming(&[0x13]), Incoming::Other, "a response");
        assert_eq!(incoming(&[0x52, 0x01, 0x00]), Incoming::Other, "a command");
        assert_eq!(incoming(&[]), Incoming::Other);
        assert_eq!(incoming(&[0x1B, 0x0A]), Incoming::Other, "cut short");
    }

    #[test]
    fn mtu() {
        let mut b = ScriptBearer::new(vec![vec![vec![0x03, 0x00, 0x02]]]);
        assert_eq!(exchange_mtu(&mut b, soon()).unwrap(), 512);
        assert_eq!(b.sent, [vec![0x02, 0x05, 0x02]]);

        // The controller asks first; the request still gets its answer.
        let mut b = ScriptBearer::new(vec![vec![vec![0x02, 0x17, 0x00], vec![0x03, 0x17, 0x00]]]);
        assert_eq!(exchange_mtu(&mut b, soon()).unwrap(), 23);
        assert_eq!(b.sent[1], [0x03, 0x05, 0x02]);

        let mut silent = ScriptBearer::default();
        let e = exchange_mtu(&mut silent, soon()).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::TimedOut);
    }

    /// A service search's answer: start, end and UUID (as text, or a 16-bit one) of each.
    fn group(entries: &[(u16, u16, &str)]) -> Vec<u8> {
        let uuid_le = |u: &str| match u16::from_str_radix(u, 16) {
            Ok(short) => short.to_le_bytes().to_vec(),
            Err(_) => uuid(u).unwrap().to_le_bytes().to_vec(),
        };
        let mut a = vec![0x11, (4 + uuid_le(entries[0].2).len()) as u8];
        for (s, e, u) in entries {
            a.extend(s.to_le_bytes());
            a.extend(e.to_le_bytes());
            a.extend(uuid_le(u));
        }
        a
    }

    /// The services of a Pro Controller 2, as it listed them.
    #[test]
    fn services_of_a_pro_controller() {
        let mut b = ScriptBearer::new(vec![
            vec![group(&[
                (0x01, 0x07, "00C5AF5D-1964-4E30-8F51-1956F96BD280"),
                (0x08, 0x32, "AB7DE9BE-89FE-49AD-828F-118F09DF7FD0"),
            ])],
            vec![group(&[(0x33, 0x37, "1800"), (0x38, 0x38, "1801")])],
            vec![vec![0x01, 0x10, 0x39, 0x00, 0x0A]],
        ]);
        let services = services(&mut b, soon()).unwrap();
        let ranges: Vec<(u16, u16)> = services.iter().map(|s| (s.start, s.end)).collect();
        assert_eq!(
            ranges,
            [(0x01, 0x07), (0x08, 0x32), (0x33, 0x37), (0x38, 0x38)]
        );
        assert_eq!(
            services[1].uuid,
            uuid("AB7DE9BE-89FE-49AD-828F-118F09DF7FD0").unwrap()
        );
        assert_eq!(
            services[3].uuid,
            short_uuid(0x1801),
            "the empty GATT service"
        );
        // Each search goes on after the last service found.
        assert_eq!(b.sent[1][1..3], [0x33, 0x00]);
        assert_eq!(b.sent[2][1..3], [0x39, 0x00]);
    }

    fn declaration(decl: u16, props: u8, value: u16, uuid_text: &str) -> Vec<u8> {
        let mut e = decl.to_le_bytes().to_vec();
        e.push(props);
        e.extend(value.to_le_bytes());
        e.extend(uuid(uuid_text).unwrap().to_le_bytes());
        e
    }

    #[test]
    fn characteristics_one_per_answer_until_enough() {
        const INPUT: &str = "AB7DE9BE-89FE-49AD-828F-118F09DF7FD2";
        const OTHER: &str = "7492866C-EC3E-4619-8258-32755FFCC0F8";
        let answer = |e: Vec<u8>| {
            let mut a = vec![0x09, 21];
            a.extend(e);
            vec![a]
        };
        let mut b = ScriptBearer::new(vec![
            answer(declaration(0x09, 0x12, 0x0A, INPUT)),
            answer(declaration(0x0D, 0x12, 0x0E, OTHER)),
            answer(declaration(0x11, 0x04, 0x12, OTHER)),
        ]);
        let found = characteristics(&mut b, 0x08, 0x32, |found| found.len() == 2, soon()).unwrap();
        assert_eq!(
            found[0],
            Characteristic {
                declaration: 0x09,
                properties: 0x12,
                value: 0x0A,
                uuid: uuid(INPUT).unwrap()
            }
        );
        assert_eq!(found.len(), 2, "stops once it has enough");
        assert_eq!(b.sent[1][1..5], [0x0A, 0x00, 0x32, 0x00]);

        // To the end of the service.
        let mut b = ScriptBearer::new(vec![
            answer(declaration(0x09, 0x12, 0x0A, INPUT)),
            vec![hex("01080a000a")],
        ]);
        let found = characteristics(&mut b, 0x08, 0x32, |_| false, soon()).unwrap();
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn configuration_descriptors() {
        // As the input characteristic had them: its configuration, then one of Nintendo's.
        let mut b = ScriptBearer::new(vec![vec![hex("05010b000229")]]);
        assert_eq!(
            configuration(&mut b, 0x0A, 0x32, soon()).unwrap(),
            Some(0x0B)
        );
        assert_eq!(b.sent, [vec![0x04, 0x0B, 0x00, 0x32, 0x00]]);

        // None before the next characteristic.
        let mut b = ScriptBearer::new(vec![vec![hex("05010d000328")]]);
        assert_eq!(configuration(&mut b, 0x0C, 0x32, soon()).unwrap(), None);

        // A 128-bit descriptor first, then the configuration.
        let mut first = vec![0x05, 0x02, 0x0B, 0x00];
        first.extend([0x11; 16]);
        let mut b = ScriptBearer::new(vec![vec![first], vec![hex("05010c000229")]]);
        assert_eq!(
            configuration(&mut b, 0x0A, 0x32, soon()).unwrap(),
            Some(0x0C)
        );
        assert_eq!(b.sent[1][1..3], [0x0C, 0x00]);

        let mut b = ScriptBearer::new(vec![vec![hex("01040b000a")]]);
        assert_eq!(configuration(&mut b, 0x0A, 0x32, soon()).unwrap(), None);
    }

    #[test]
    fn notifications_on() {
        // An input notification may come before the write's response.
        let mut b = ScriptBearer::new(vec![vec![vec![0x1B, 0x0A, 0x00, 0x01], vec![0x13]]]);
        notify(&mut b, 0x0B, soon()).unwrap();
        assert_eq!(b.sent, [vec![0x12, 0x0B, 0x00, 0x01, 0x00]]);

        let mut b = ScriptBearer::new(vec![vec![hex("01120b0003")]]);
        assert!(notify(&mut b, 0x0B, soon()).is_err(), "write not permitted");
    }

    #[test]
    fn write_commands() {
        assert_eq!(
            write_command(0x0014, &[0x09, 0x91]),
            [0x52, 0x14, 0x00, 0x09, 0x91]
        );
    }
}
