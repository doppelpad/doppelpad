//! How Switch 2 controllers present themselves over Bluetooth LE, independent of the OS's
//! Bluetooth interface: what they advertise, and how their GATT characteristics carry the
//! HID reports and commands [`super::switch2`] speaks.
//!
//! They implement neither HID over GATT nor standard pairing (SMP): a host that tries to pair
//! the standard way loses the link. A host reads one in pairing mode (its sync button held)
//! without pairing at all, through the characteristics below. Sources: ndeadly's
//! `switch2_controller_research` (`bluetooth_interface.md`), checked against a Pro
//! Controller 2.

use super::{NINTENDO_VID, Switch2Model, model_for};
use crate::nintendo::Model;

/// Nintendo's Bluetooth company identifier, first in the manufacturer data.
const COMPANY_ID: u16 = 0x0553;

/// The service with every characteristic below.
pub const SERVICE: &str = "AB7DE9BE-89FE-49AD-828F-118F09DF7FD0";
/// Input report 0x05 (the USB report without its ID), on every model.
pub const INPUT: &str = "AB7DE9BE-89FE-49AD-828F-118F09DF7FD2";
/// Commands (the vendor commands of USB), written without response.
pub const COMMAND: &str = "649D4AC9-8EB7-4E6C-AF44-1EA54FE5F005";
/// Command replies, as notifications.
pub const REPLY: &str = "C765A961-D9D8-4D36-A20A-5315B111836A";

/// The characteristic that takes a model's rumble output report.
pub fn vibration(model: Switch2Model) -> &'static str {
    match model {
        Switch2Model::Pro => "CC483F51-9258-427D-A939-630C31F72B05",
        Switch2Model::JoyConLeft => "289326CB-A471-485D-A8F4-240C14F18241",
        Switch2Model::JoyConRight => "FA19B0FB-CD1F-46A7-84A1-BBB09E00C149",
        Switch2Model::GameCube => "3F8FB670-AB25-45BF-B540-38C72834D064",
    }
}

/// Length of the rumble output reports over Bluetooth.
const OUTPUT_LEN: usize = 0x2A;

/// What a controller's advertisement says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Advert {
    pub pid: u16,
    pub model: Switch2Model,
    /// In pairing mode, looking for any host. Otherwise it is looking for the host it is
    /// paired with (to reconnect, or to wake a console), and taking it would steal it.
    pub pairing: bool,
}

/// Read a Switch 2 controller's manufacturer data (company identifier first): company,
/// three unknown bytes, USB vendor and product id, three unknown bytes (the last 0x81 for a
/// wake-up call), the host's address (zero in pairing mode).
pub fn parse_advert(data: &[u8]) -> Option<Advert> {
    let u16_at = |at: usize| Some(u16::from_le_bytes([*data.get(at)?, *data.get(at + 1)?]));
    if u16_at(0)? != COMPANY_ID || u16_at(5)? != NINTENDO_VID {
        return None;
    }
    let pid = u16_at(7)?;
    let Some(Model::Switch2(model)) = model_for(NINTENDO_VID, pid) else {
        return None;
    };
    let host = data.get(0x0C..0x12)?;
    Some(Advert {
        pid,
        model,
        pairing: host.iter().all(|&b| b == 0),
    })
}

/// An input notification as the HID report it carries (its ID put back in front).
pub fn input_report(value: &[u8], out: &mut Vec<u8>) {
    out.clear();
    out.push(super::switch2::INPUT_REPORT);
    out.extend_from_slice(value);
}

/// A rumble output report as its characteristic takes it: report ID 0, 42 bytes.
pub fn output_report(report: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; OUTPUT_LEN];
    let body = report.get(1..).unwrap_or_default();
    let n = body.len().min(OUTPUT_LEN - 1);
    out[1..=n].copy_from_slice(&body[..n]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nintendo::pid;

    #[test]
    fn adverts() {
        // A Pro Controller 2 in pairing mode, as macOS received it.
        let pairing = [
            0x53, 0x05, 0x01, 0x00, 0x03, 0x7E, 0x05, 0x69, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x0F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(
            parse_advert(&pairing),
            Some(Advert {
                pid: pid::SWITCH2_PRO,
                model: Switch2Model::Pro,
                pairing: true
            })
        );
        let mut reconnect = pairing;
        reconnect[0x0C..0x12].copy_from_slice(&[0x5F, 0x11, 0x85, 0xEB, 0xF1, 0x48]);
        assert!(
            !parse_advert(&reconnect).unwrap().pairing,
            "it wants its console"
        );
        let mut gamecube = pairing;
        gamecube[7..9].copy_from_slice(&pid::SWITCH2_GAMECUBE.to_le_bytes());
        assert_eq!(
            parse_advert(&gamecube).unwrap().model,
            Switch2Model::GameCube
        );

        let mut switch1 = pairing;
        switch1[7..9].copy_from_slice(&0x2009u16.to_le_bytes());
        assert_eq!(parse_advert(&switch1), None);
        assert_eq!(parse_advert(&pairing[..0x10]), None, "cut short");
        assert_eq!(
            parse_advert(&[0x4C, 0x00, 0x10, 0x05]),
            None,
            "someone else"
        );
    }

    #[test]
    fn reports() {
        let mut r = Vec::new();
        input_report(&[0xAA; 63], &mut r);
        assert_eq!(r.len(), 64);
        assert_eq!(r[..2], [0x05, 0xAA]);

        let mut usb = [0u8; 64];
        usb[0] = 0x02;
        usb[1] = 0x53;
        usb[0x11] = 0x53;
        usb[0x30] = 0xFF;
        let bt = output_report(&usb);
        assert_eq!(bt.len(), 0x2A);
        assert_eq!(bt[..2], [0x00, 0x53]);
        assert_eq!(bt[0x11], 0x53, "the second motor stays where it was");
        assert_eq!(output_report(&[0x02, 0x51]), {
            let mut v = vec![0u8; 0x2A];
            v[1] = 0x51;
            v
        });
    }
}
