//! Output reports: rumble, light bar, player and microphone lights, trigger effects.

use dp_core::pad::{Rgb, Rumble};
use dp_core::profile::TriggerEffect;

use super::SonyModel;
use crate::report::motor_u8;
use crate::{Leds, TriggerEffects};

/// What an output report sets.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub(super) struct SonyOutput {
    pub rumble: Rumble,
    /// `None` leaves the light bar as it is.
    pub lightbar: Option<Rgb>,
    pub leds: Leds,
    /// Trigger effects, left then right (DualSense).
    pub triggers: TriggerEffects,
    /// Take the light bar over from the DualSense firmware (once, before the first colour).
    pub setup: bool,
}

/// Bits `start..=9` set: the trigger zones an effect covers.
fn zones_from(start: u8) -> u16 {
    (0x3FF << start.min(9)) & 0x3FF
}

/// `value` (1–8) as three bits in every zone of `zones`.
fn per_zone(zones: u16, value: u8) -> u32 {
    let v = u32::from(value.clamp(1, 8) - 1);
    (0..10)
        .filter(|i| zones & (1 << i) != 0)
        .fold(0, |acc, i| acc | v << (3 * i))
}

/// The eleven bytes the DualSense takes for one trigger effect.
pub(super) fn effect_bytes(e: TriggerEffect) -> [u8; 11] {
    let mut b = [0u8; 11];
    match e {
        TriggerEffect::Off => b[0] = 0x05,
        TriggerEffect::Resistance { start, strength } => {
            let zones = zones_from(start);
            b[0] = 0x21;
            b[1..3].copy_from_slice(&zones.to_le_bytes());
            b[3..7].copy_from_slice(&per_zone(zones, strength).to_le_bytes());
        }
        TriggerEffect::Weapon {
            start,
            end,
            strength,
        } => {
            let start = start.clamp(2, 7);
            let end = end.clamp(start + 1, 8);
            let zones: u16 = (1 << start) | (1 << end);
            b[0] = 0x25;
            b[1..3].copy_from_slice(&zones.to_le_bytes());
            b[3] = strength.clamp(1, 8) - 1;
        }
        TriggerEffect::Vibration {
            start,
            amplitude,
            frequency,
        } => {
            let zones = zones_from(start);
            b[0] = 0x26;
            b[1..3].copy_from_slice(&zones.to_le_bytes());
            b[3..7].copy_from_slice(&per_zone(zones, amplitude).to_le_bytes());
            b[9] = frequency.max(1);
        }
        TriggerEffect::Raw { bytes } => b = bytes,
    }
    b
}

/// CRC-32 (IEEE) as the Bluetooth reports carry it.
pub(super) fn crc32(data: impl IntoIterator<Item = u8>) -> u32 {
    let mut c = !0u32;
    for b in data {
        c ^= u32::from(b);
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

/// Bluetooth output reports end with a CRC over the 0xA2 header byte and everything before it.
fn seal(r: &mut [u8]) {
    let n = r.len() - 4;
    let crc = crc32(std::iter::once(0xA2).chain(r[..n].iter().copied()));
    r[n..].copy_from_slice(&crc.to_le_bytes());
}

/// DualSense player lights for players 1–5 (centre first, then outwards).
const DUALSENSE_PLAYERS: [u8; 5] = [0x04, 0x0A, 0x15, 0x1B, 0x1F];

/// The common part of a DualShock 4 output report (10 bytes from the first flag byte).
fn ds4_common(c: &mut [u8], o: &SonyOutput) {
    c[0] = 0x01; // rumble valid
    c[3] = motor_u8(o.rumble.high);
    c[4] = motor_u8(o.rumble.low);
    if let Some(l) = o.lightbar {
        c[0] |= 0x02; // light bar valid
        c[5..8].copy_from_slice(&[l.r, l.g, l.b]);
    }
}

/// The common part of a DualSense output report (47 bytes from the first flag byte).
fn dualsense_common(c: &mut [u8], o: &SonyOutput) {
    c[0] = 0x03; // compatible vibration + haptics select
    c[2] = motor_u8(o.rumble.high);
    c[3] = motor_u8(o.rumble.low);
    if let Some(on) = o.leds.mic {
        c[1] |= 0x01; // microphone LED control
        c[8] = u8::from(on);
    }
    if o.setup {
        c[38] |= 0x02; // light bar setup control
        c[41] = 0x02; // light out: the firmware lets go of the light bar
    }
    if let Some(p) = o.leds.player {
        c[1] |= 0x10; // player LED control
        c[43] = match p {
            0 => 0,
            n => DUALSENSE_PLAYERS[(n as usize - 1).min(4)],
        };
    }
    if let Some(l) = o.lightbar {
        c[1] |= 0x04; // light bar control
        c[44..47].copy_from_slice(&[l.r, l.g, l.b]);
    }
    let [left, right] = o.triggers;
    if let Some(e) = right {
        c[0] |= 0x04; // right trigger effect
        c[10..21].copy_from_slice(&effect_bytes(e));
    }
    if let Some(e) = left {
        c[0] |= 0x08; // left trigger effect
        c[21..32].copy_from_slice(&effect_bytes(e));
    }
}

/// Build an output report: rumble, light bar and lights. Over Bluetooth `seq` (0–15) counts
/// DualSense reports.
pub(super) fn output_report(model: SonyModel, bluetooth: bool, o: &SonyOutput, seq: u8) -> Vec<u8> {
    match (model, bluetooth) {
        (SonyModel::DualShock4, false) => {
            let mut r = vec![0u8; 32];
            r[0] = 0x05;
            ds4_common(&mut r[1..11], o);
            r
        }
        (SonyModel::DualShock4, true) => {
            let mut r = vec![0u8; 78];
            r[0] = 0x11;
            r[1] = 0xC0; // HID output with CRC
            ds4_common(&mut r[3..13], o);
            seal(&mut r);
            r
        }
        (_, false) => {
            let mut r = vec![0u8; 48];
            r[0] = 0x02;
            dualsense_common(&mut r[1..48], o);
            r
        }
        (_, true) => {
            let mut r = vec![0u8; 78];
            r[0] = 0x31;
            r[1] = (seq & 0x0F) << 4;
            r[2] = 0x10; // output tag
            dualsense_common(&mut r[3..50], o);
            seal(&mut r);
            r
        }
    }
}
