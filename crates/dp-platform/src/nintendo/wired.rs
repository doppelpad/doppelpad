//! Third-party wired Switch pads (HORI, PDP, PowerA, ZUIKI) that only send a plain input
//! report: no handshake, no subcommands, no rumble or motion.

use std::io;
use std::time::Instant;

use dp_core::pad::{Axis, PadButton, PadState, Rumble, Stick, stick_from_hid_u8};

use super::{HORI_VID, PDP_VID, POWERA_VID, ZUIKI_VID};
use crate::PadFamily;
use crate::padio::{Identity, InitError, PadDriver, ReportIo};

pub fn name(vid: u16) -> &'static str {
    match vid {
        HORI_VID => "HORI wired controller",
        PDP_VID => "PDP wired controller",
        POWERA_VID => "PowerA wired controller",
        ZUIKI_VID => "ZUIKI wired controller",
        _ => "Wired Switch controller",
    }
}

/// Parse one report. `r[0]` is the report ID (0 for these unnumbered reports on Windows),
/// followed by buttons[2], hat, LX, LY, RX, RY.
pub fn parse_report(r: &[u8]) -> Option<PadState> {
    let p = r.get(1..8)?;
    let mut s = PadState::default();
    s.set(PadButton::X, p[0] & 0x01 != 0); // Y
    s.set(PadButton::A, p[0] & 0x02 != 0); // B
    s.set(PadButton::B, p[0] & 0x04 != 0); // A
    s.set(PadButton::Y, p[0] & 0x08 != 0); // X
    s.set(PadButton::LB, p[0] & 0x10 != 0);
    s.set(PadButton::RB, p[0] & 0x20 != 0);
    s.set_axis(Axis::LeftTrigger, if p[0] & 0x40 != 0 { 1.0 } else { 0.0 });
    s.set_axis(Axis::RightTrigger, if p[0] & 0x80 != 0 { 1.0 } else { 0.0 });
    s.set(PadButton::Back, p[1] & 0x01 != 0);
    s.set(PadButton::Start, p[1] & 0x02 != 0);
    s.set(PadButton::LS, p[1] & 0x04 != 0);
    s.set(PadButton::RS, p[1] & 0x08 != 0);
    s.set(PadButton::Guide, p[1] & 0x10 != 0);
    s.set(PadButton::Misc, p[1] & 0x20 != 0);
    let (up, right, down, left) = match p[2] & 0x0F {
        0 => (true, false, false, false),
        1 => (true, true, false, false),
        2 => (false, true, false, false),
        3 => (false, true, true, false),
        4 => (false, false, true, false),
        5 => (false, false, true, true),
        6 => (false, false, false, true),
        7 => (true, false, false, true),
        _ => (false, false, false, false),
    };
    s.set(PadButton::DpadUp, up);
    s.set(PadButton::DpadRight, right);
    s.set(PadButton::DpadDown, down);
    s.set(PadButton::DpadLeft, left);
    s.set_stick(Stick::Left, stick_from_hid_u8(p[3], p[4]));
    s.set_stick(Stick::Right, stick_from_hid_u8(p[5], p[6]));
    Some(s)
}

pub struct WiredDriver {
    vid: u16,
}

impl WiredDriver {
    pub fn new(vid: u16) -> Self {
        WiredDriver { vid }
    }
}

impl PadDriver for WiredDriver {
    fn init(&mut self, _io: &mut dyn ReportIo) -> Result<Identity, InitError> {
        Ok(Identity {
            name: name(self.vid).to_owned(),
            family: PadFamily::SwitchPro,
            joycon: None,
            has_motion: false,
            has_touchpad: false,
            has_rumble: false,
            notice: None,
        })
    }

    fn parse(&mut self, report: &[u8], _now: Instant) -> Option<PadState> {
        parse_report(report)
    }

    fn service(&mut self, _io: &mut dyn ReportIo, _rumble: Rumble) -> io::Result<()> {
        Ok(())
    }

    fn next_deadline(&self) -> Option<Instant> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_report_is_neutral() {
        let s = parse_report(&[0, 0, 0, 0x0F, 0x80, 0x80, 0x80, 0x80]).unwrap();
        assert_eq!(s.buttons, 0);
        assert!(s.axis(Axis::LeftX).abs() < 0.01 && s.axis(Axis::RightY).abs() < 0.01);
    }

    #[test]
    fn buttons_hat_and_sticks() {
        let s = parse_report(&[0, 0x02 | 0x40, 0x10 | 0x20, 3, 0xFF, 0x00, 0x80, 0x80]).unwrap();
        assert!(s.pressed(PadButton::A), "B is south");
        assert_eq!(s.axis(Axis::LeftTrigger), 1.0);
        assert!(s.pressed(PadButton::Guide) && s.pressed(PadButton::Misc));
        assert!(s.pressed(PadButton::DpadRight) && s.pressed(PadButton::DpadDown));
        assert!(!s.pressed(PadButton::DpadUp));
        assert!(s.axis(Axis::LeftX) > 0.99);
        assert!(s.axis(Axis::LeftY) > 0.99, "HID 0 is up");
        assert!(parse_report(&[0, 0, 0]).is_none());
    }

    #[test]
    fn truncated_reports_do_not_panic() {
        let r = [0, 0xFF, 0xFF, 3, 0xFF, 0x00, 0x80, 0x80];
        assert!(parse_report(&r).is_some());
        for n in 0..=r.len() {
            parse_report(&r[..n]);
        }
    }
}
