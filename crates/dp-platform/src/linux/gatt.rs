//! Setting a Switch 2 controller up over ATT ([`super::att`]): the MTU its input needs, its
//! characteristics ([`crate::nintendo::switch2ble`]), and its input and reply notifications
//! switched on. No I/O of its own, so it is unit-tested on every OS.

use std::io;
use std::time::Instant;

use super::att;
use crate::nintendo::Switch2Model;
use crate::nintendo::switch2ble::{self, Role};

/// An input notification: report 0x05 without its ID. The MTU must leave room for it.
const INPUT_LEN: u16 = 63;

/// Where a controller's characteristics have their values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handles {
    pub input: u16,
    pub reply: u16,
    pub command: u16,
    pub vibration: Option<u16>,
}

/// Exchange the MTU, find the controller's characteristics and switch input and reply
/// notifications on.
pub fn set_up(
    bearer: &mut dyn att::Bearer,
    model: Switch2Model,
    until: Instant,
) -> io::Result<Handles> {
    let mtu = att::exchange_mtu(bearer, until)?;
    if mtu < INPUT_LEN + 3 {
        return Err(io::Error::other(format!("its MTU stays at {mtu}")));
    }
    let service = att::services(bearer, until)?
        .into_iter()
        .find(|s| s.uuid == switch2ble::SERVICE)
        .ok_or_else(|| io::Error::other("it lacks its input service"))?;
    let role = |c: &att::Characteristic| switch2ble::role(c.uuid, model);
    let wanted = [Role::Input, Role::Command, Role::Reply, Role::Vibration];
    let found = att::characteristics(
        bearer,
        service.start,
        service.end,
        |found| {
            wanted
                .iter()
                .all(|w| found.iter().any(|c| role(c) == Some(*w)))
        },
        until,
    )?;
    let value = |r: Role| found.iter().find(|c| role(c) == Some(r)).map(|c| c.value);
    let (Some(input), Some(command), Some(reply)) =
        (value(Role::Input), value(Role::Command), value(Role::Reply))
    else {
        return Err(io::Error::other(
            "it lacks input or command characteristics",
        ));
    };
    for handle in [input, reply] {
        let Some(configuration) = att::configuration(bearer, handle, service.end, until)? else {
            return Err(io::Error::other("its input cannot be switched on"));
        };
        att::notify(bearer, configuration, until)?;
    }
    Ok(Handles {
        input,
        reply,
        command,
        vibration: value(Role::Vibration),
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::linux::att::testing::ScriptBearer;
    use crate::nintendo::switch2ble::parse_uuid;

    /// One characteristic declaration per answer, as the controller sends them.
    fn declaration(decl: u16, props: u8, value: u16, uuid: u128) -> Vec<Vec<u8>> {
        let mut a = vec![0x09, 21];
        a.extend(decl.to_le_bytes());
        a.push(props);
        a.extend(value.to_le_bytes());
        a.extend(uuid.to_le_bytes());
        vec![a]
    }

    /// Set up as a Pro Controller 2 answered, with its handles.
    #[test]
    fn set_up_a_pro_controller() {
        let mut services = vec![0x11, 20];
        let gap = parse_uuid("00C5AF5D-1964-4E30-8F51-1956F96BD280").unwrap();
        for (start, end, uuid) in [(0x01_u16, 0x07_u16, gap), (0x08, 0x32, switch2ble::SERVICE)] {
            services.extend(start.to_le_bytes());
            services.extend(end.to_le_bytes());
            services.extend(uuid.to_le_bytes());
        }
        let other = parse_uuid("7492866C-EC3E-4619-8258-32755FFCC0F8").unwrap();
        let mut b = ScriptBearer::new(vec![
            vec![vec![0x03, 0x00, 0x02]],
            vec![services],
            vec![vec![0x11, 6, 0x33, 0, 0x38, 0, 0x00, 0x18]],
            vec![vec![0x01, 0x10, 0x39, 0x00, 0x0A]],
            declaration(0x09, 0x12, 0x0A, switch2ble::INPUT),
            declaration(0x0D, 0x12, 0x0E, other),
            declaration(0x11, 0x04, 0x12, switch2ble::vibration(Switch2Model::Pro)),
            declaration(0x13, 0x04, 0x14, switch2ble::COMMAND),
            declaration(0x15, 0x04, 0x16, other),
            declaration(0x17, 0x04, 0x18, other),
            declaration(0x19, 0x10, 0x1A, switch2ble::REPLY),
            vec![vec![0x05, 1, 0x0B, 0x00, 0x02, 0x29]],
            vec![vec![0x13]],
            vec![vec![0x05, 1, 0x1B, 0x00, 0x02, 0x29]],
            // Input already flows.
            vec![vec![0x1B, 0x0A, 0x00, 0xAA], vec![0x13]],
        ]);
        let until = Instant::now() + Duration::from_secs(1);
        let Ok(handles) = set_up(&mut b, Switch2Model::Pro, until) else {
            panic!("not set up: {:02x?}", b.sent);
        };
        assert_eq!(
            handles,
            Handles {
                input: 0x0A,
                reply: 0x1A,
                command: 0x14,
                vibration: Some(0x12),
            }
        );
        // No characteristic is looked for after the reply's.
        assert!(b.answers.is_empty());
        let writes: Vec<&Vec<u8>> = b.sent.iter().filter(|p| p[0] == 0x12).collect();
        assert_eq!(
            writes,
            [&vec![0x12, 0x0B, 0, 1, 0], &vec![0x12, 0x1B, 0, 1, 0]]
        );
    }

    #[test]
    fn set_up_needs_room_for_input() {
        let mut b = ScriptBearer::new(vec![vec![vec![0x03, 0x17, 0x00]]]);
        let until = Instant::now() + Duration::from_secs(1);
        let why = set_up(&mut b, Switch2Model::Pro, until).unwrap_err();
        assert_eq!(why.to_string(), "its MTU stays at 23");
    }
}
