use std::collections::HashMap;

use dp_core::pad::{Axis, PadButton};

use super::calibration::{Calibration, ImuCal};
use super::protocol::{parse_battery, parse_full, spi_data, spi_read_args, subcommand_reply};
use super::rumble::{hd_rumble, rumble_data};
use super::*;
use crate::padio::testing::ScriptIo;
use crate::report::{le_u32, pack12};

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
                    let addr = le_u32(w, 11);
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
        transport: if bluetooth {
            Transport::Bluetooth
        } else {
            Transport::Usb
        },
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
            0x80 => 0x8000 | u16::from(w[1]),
            0x01 => u16::from(w[10]),
            id => 0x1000 | u16::from(id),
        })
        .collect()
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
    user[2..5].copy_from_slice(&pack12(1000, 1000));
    user[5..8].copy_from_slice(&pack12(2000, 2000));
    user[8..11].copy_from_slice(&pack12(1000, 1000));
    let mut spi = HashMap::new();
    spi.insert(SPI_USER_STICKS, user);
    let mut factory = vec![0u8; 18];
    factory[9..12].copy_from_slice(&pack12(1800, 1900)); // right centre
    factory[12..15].copy_from_slice(&pack12(500, 500)); // right min below
    factory[15..18].copy_from_slice(&pack12(600, 600)); // right max above
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
    r[6..9].copy_from_slice(&pack12(2048, 2048));
    r[9..12].copy_from_slice(&pack12(2048, 2048));
    r
}

#[test]
fn parse_buttons_sticks_and_motion() {
    let mut cal = Calibration::default();
    let mut r = full_report();
    r[3] = 0x08; // A
    r[5] = 0x80; // ZL
    r[6..9].copy_from_slice(&pack12(4095, 2048)); // left stick right
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
fn silent_usb_pad_is_started_again() {
    let (mut d, mut io) = ready_driver(false);
    d.parse(&full_report(), io.now());
    for _ in 0..9 {
        io.advance(100 * MS);
        d.service(&mut io, Rumble::default()).unwrap();
        assert!(
            d.next_deadline().unwrap() > io.now(),
            "no deadline in the past"
        );
    }
    assert!(
        !sequence(&io).contains(&0x8002),
        "keep-alives only for a second"
    );

    io.advance(100 * MS);
    d.service(&mut io, Rumble::default()).unwrap();
    let seq = sequence(&io);
    let start = seq
        .iter()
        .position(|&s| s == 0x8002)
        .expect("handshake again");
    let after = &seq[start..];
    let pos = |s: u16| after.iter().position(|&x| x == s).unwrap();
    assert!(pos(0x40) < pos(0x48) && pos(0x48) < pos(0x03) && pos(0x03) < pos(0x30));

    io.written.clear();
    io.advance(500 * MS);
    d.service(&mut io, Rumble::default()).unwrap();
    assert!(!sequence(&io).contains(&0x8002), "at most once a second");

    d.parse(&full_report(), io.now());
    io.advance(50 * MS);
    d.service(&mut io, Rumble::default()).unwrap();
    assert!(!sequence(&io).contains(&0x8002), "reporting again");
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
    assert!(d.can_power_off());
    d.power_off(&mut io).unwrap();
    assert!(io.writes().iter().any(|w| off(w)));
    let (d, _) = ready_driver(false);
    assert!(!d.can_power_off());
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

/// A full report with every part the parser reads in use: battery, buttons, sticks
/// and three IMU frames.
fn busy_full_report(id: u8) -> Vec<u8> {
    let mut r = full_report();
    r[0] = id;
    r[2] = 0x8E;
    r[3] = 0x08;
    for f in 0..3 {
        let at = 13 + f * 12;
        r[at + 4..at + 6].copy_from_slice(&4096i16.to_le_bytes());
    }
    r
}

#[test]
fn truncated_reports_do_not_panic() {
    let mut factory = Vec::new();
    for v in [
        0i16, 0, 0, 16384, 16384, 8192, 10, 20, 30, 13381, 13391, 13401,
    ] {
        factory.extend_from_slice(&v.to_le_bytes());
    }
    let mut user = [0u8; 20];
    user[..2].copy_from_slice(&CAL_MAGIC);
    let mut spi_reply = vec![0u8; 49];
    spi_reply[0] = 0x21;
    spi_reply[13] = 0x90;
    spi_reply[14] = SUB_SPI_READ;
    spi_reply[15..20].copy_from_slice(&spi_read_args(SPI_FACTORY_IMU, 24));
    spi_reply[20..44].copy_from_slice(&factory);

    let types = [
        ControllerType::Pro,
        ControllerType::JoyConRight,
        ControllerType::Snes,
    ];
    let full = busy_full_report(0x30);
    let s = parse_full(ControllerType::Pro, &mut Calibration::default(), &full);
    assert!(s.unwrap().motion.is_some(), "the whole report parses");
    let data = subcommand_reply(&spi_reply, SUB_SPI_READ).unwrap();
    assert_eq!(spi_data(data, SPI_FACTORY_IMU, 24), Some(&factory[..]));

    let mut d = Switch1Driver::new(link(true, 0x2009));
    let now = Instant::now();
    for r in [full, busy_full_report(0x31), spi_reply] {
        for n in 0..=r.len() {
            let r = &r[..n];
            for ty in types {
                parse_full(ty, &mut Calibration::default(), r);
            }
            parse_battery(r);
            d.parse(r, now);
            if let Some(data) = subcommand_reply(r, SUB_SPI_READ) {
                for k in 0..=data.len() {
                    spi_data(&data[..k], SPI_FACTORY_IMU, 24);
                }
            }
        }
    }
    for n in 0..=factory.len() {
        ImuCal::from_spi(&factory[..n], None);
        for m in 0..=user.len() {
            ImuCal::from_spi(&factory[..n], Some(&user[..m]));
        }
    }
}

#[test]
fn lost_motion_is_switched_on_again() {
    let (mut d, mut io) = ready_driver(true);
    let imu_on = |io: &ScriptIo| {
        io.writes()
            .iter()
            .filter(|w| w[0] == 0x01 && w[10] == SUB_IMU && w[11] == 0x01)
            .count()
    };
    d.parse(&busy_full_report(0x30), io.now());
    d.service(&mut io, Rumble::default()).unwrap();
    assert_eq!(imu_on(&io), 0, "the IMU reports");

    d.parse(&full_report(), io.now());
    assert!(d.next_deadline().is_some_and(|t| t <= io.now()), "due now");
    d.service(&mut io, Rumble::default()).unwrap();
    assert_eq!(imu_on(&io), 1);
    assert_eq!(d.next_deadline(), Some(io.now() + IMU_RETRY));

    io.advance(IMU_RETRY - MS);
    d.parse(&full_report(), io.now());
    d.service(&mut io, Rumble::default()).unwrap();
    assert_eq!(imu_on(&io), 1, "not again before the retry time");
    io.advance(MS);
    d.parse(&full_report(), io.now());
    d.service(&mut io, Rumble::default()).unwrap();
    assert_eq!(imu_on(&io), 2);

    d.parse(&busy_full_report(0x30), io.now());
    io.advance(IMU_RETRY);
    d.service(&mut io, Rumble::default()).unwrap();
    assert_eq!(imu_on(&io), 2, "motion is back");
    assert_eq!(d.next_deadline(), None);
}

#[test]
fn player_lights_follow_the_slot() {
    let (mut d, mut io) = ready_driver(true);
    let lights = |io: &ScriptIo| -> Vec<u8> {
        io.writes()
            .iter()
            .filter(|w| w[0] == 0x01 && w[10] == SUB_PLAYER_LEDS)
            .map(|w| w[11])
            .collect()
    };
    let player = |n| Leds {
        player: Some(n),
        mic: None,
    };
    d.set_leds(player(1));
    d.service(&mut io, Rumble::default()).unwrap();
    assert!(lights(&io).is_empty(), "init lit player 1 already");
    d.set_leds(player(2));
    d.service(&mut io, Rumble::default()).unwrap();
    d.service(&mut io, Rumble::default()).unwrap();
    assert_eq!(lights(&io), [0x03], "LEDs 1 and 2, sent once");
    d.set_leds(player(0));
    d.service(&mut io, Rumble::default()).unwrap();
    assert_eq!(lights(&io), [0x03, 0x00]);
}
