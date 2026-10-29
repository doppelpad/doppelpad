//! Raw HID controllers XInput cannot see: DualShock 4, DualSense, Nintendo Switch pads, Steam
//! controllers and other HID gamepads (read through their report descriptor). This is the
//! Windows scanner of the shared HID backend ([`crate::hidhost`]): it finds HID interfaces
//! through SetupAPI, opens them with overlapped I/O and picks their drivers. Switch 2
//! controllers connected over Bluetooth LE ([`super::ble`]) join them.

use std::collections::HashMap;

use windows_sys::Win32::Devices::HumanInterfaceDevice::GUID_DEVINTERFACE_HID;
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION};

use super::devnode::{InterfaceEntry, enumerate_interfaces, is_vigem_device, product_name};
use super::hidio::{HidCaps, HidIo};
use crate::bdaddr::BdAddr;
use crate::generic::ReportDecoder;
use crate::hidhost::{
    Devices, Driver, DriverKind, Facts, Found, HidBackend, Look, MakeDriver, XboxPads, driver_kind,
    make_driver, scan_pass,
};
use crate::nintendo;
use crate::sony::{self, SonyModel};
use crate::{DeviceId, JoyConSide, Switch2Search, Transport};

/// The HID backend with the Windows scanner. When it stops, the Bluetooth LE controllers are
/// let go.
pub fn backend(search: Switch2Search) -> HidBackend {
    let mut skip = HashMap::new();
    HidBackend::new(
        move |pass, devices| {
            let present = enumerate_interfaces(&GUID_DEVINTERFACE_HID)
                .into_iter()
                .map(|e| (e.path.clone(), e))
                .collect();
            let wireless = super::ble::poll(search.on());
            scan_pass(pass, devices, &mut skip, present, wireless, |_, entry| {
                look(&entry, devices)
            });
        },
        super::bluetooth::disconnect,
    )
    .on_drop(super::ble::let_all_go)
}

/// The Bluetooth HID service class, which Bluetooth HID devices carry in their paths.
const BT_HID_SERVICE: &str = "00001124-0000-1000-8000-00805f9b34fb";

/// How a HID interface is connected, by its instance id and path.
fn transport(entry: &InterfaceEntry) -> Transport {
    if entry.instance_id.to_ascii_uppercase().contains("BTH")
        || entry.path.to_ascii_lowercase().contains(BT_HID_SERVICE)
    {
        Transport::Bluetooth
    } else {
        Transport::Usb
    }
}

/// Charging-grip slot of a HID interface: interface 1 holds the left Joy-Con.
fn grip_slot(instance_id: &str) -> Option<JoyConSide> {
    let id = instance_id.to_ascii_uppercase();
    if id.contains("&MI_01") {
        Some(JoyConSide::Left)
    } else if id.contains("&MI_00") {
        Some(JoyConSide::Right)
    } else {
        None
    }
}

/// Start the controller behind a HID interface, if one of our drivers handles it.
fn look(entry: &InterfaceEntry, devices: &Devices) -> Look {
    match identify(entry) {
        Ok(Some((io, found, make_driver))) => {
            devices.start(io, found, make_driver);
            Look::Started
        }
        Ok(None) => Look::Skip(None),
        Err(e) => {
            tracing::debug!(path = %entry.path, "cannot open HID device: {e}");
            Look::Skip(busy_problem(entry, &e))
        }
    }
}

/// What to tell the user about a controller that another program keeps to itself (it cannot
/// even be opened for reading); `None` for other errors and other devices. Valve devices are
/// left out: their keyboard and mouse collections always refuse to open.
fn busy_problem(entry: &InterfaceEntry, e: &std::io::Error) -> Option<String> {
    let code = u32::try_from(e.raw_os_error()?).ok()?;
    if !is_busy_code(code) {
        return None;
    }
    let (vid, pid) = crate::holders::ids_in_path(&entry.path)?;
    let model = sony::model_for(vid, pid)
        .map(SonyModel::name)
        .or_else(|| nintendo::model_for(vid, pid).map(|_| "Nintendo controller"))?;
    let name = product_name(entry.devinst).unwrap_or_else(|| model.to_owned());
    Some(crate::holders::busy_message(
        &format!("the {name}"),
        super::process::likely_holder(),
    ))
}

/// Opening failed because another handle excludes ours.
fn is_busy_code(code: u32) -> bool {
    code == ERROR_SHARING_VIOLATION || code == ERROR_ACCESS_DENIED
}

/// What a HID interface is to the scanner, from what it tells about itself: its driver kind
/// and how it is connected, or `None` for one no driver of ours reads. `virtual_pad`: it is a
/// virtual controller (ViGEm's, Doppelpad's own or another program's).
fn classify(
    (vid, pid): (u16, u16),
    caps: HidCaps,
    entry: &InterfaceEntry,
    virtual_pad: bool,
) -> Option<(DriverKind, Transport)> {
    // Xbox pads are read through XInput.
    let kind = driver_kind(vid, pid, caps.usage_page, caps.usage, XboxPads::Elsewhere)?;
    if virtual_pad {
        return None;
    }
    // XInput reads pads that also speak XInput (`IG_` in the path).
    if kind == DriverKind::Generic && entry.path.to_ascii_uppercase().contains("&IG_") {
        return None;
    }
    let transport = transport(entry);
    if transport == Transport::Usb
        && matches!(kind, DriverKind::Nintendo(_))
        && nintendo::usb_unsupported(vid, pid)
    {
        return None;
    }
    Some((kind, transport))
}

/// The device, what the scanner found out about it, and how to build its driver.
type Identified = (HidIo, Found, MakeDriver<HidIo>);

/// Open an interface and decide whether one of our drivers handles it.
fn identify(entry: &InterfaceEntry) -> std::io::Result<Option<Identified>> {
    let io = HidIo::open(&entry.path)?;
    let Some((vid, pid)) = io.ids() else {
        return Ok(None);
    };
    let caps = io.caps();
    let virtual_pad = is_vigem_device(entry.devinst);
    let Some((kind, transport)) = classify((vid, pid), caps, entry, virtual_pad) else {
        return Ok(None);
    };
    // The parser must be able to read a generic pad's reports.
    let decoder = || io.decoder().map(|d| Box::new(d) as Box<dyn ReportDecoder>);
    let Some(driver) = Driver::of(kind, decoder) else {
        return Ok(None);
    };
    let bt_address = (transport == Transport::Bluetooth && matches!(kind, DriverKind::Sony(_)))
        .then(|| io.serial().and_then(|s| BdAddr::parse(&s)))
        .flatten();
    let found = Found {
        path: entry.path.clone(),
        id: DeviceId(format!("hid:{}", entry.instance_id)),
        transport,
        label: kind.label(transport, pid),
        vid,
        pid,
        instance_paths: vec![entry.instance_id.clone()],
        bt_address,
    };
    let facts = Facts {
        vid,
        pid,
        transport,
        product: io.product(),
        feature_len: caps.feature_len,
        grip_slot: grip_slot(&entry.instance_id),
    };
    let devinst = entry.devinst;
    let make: MakeDriver<HidIo> =
        Box::new(move |_| make_driver(driver, facts, || super::switch2::locate(devinst)));
    Ok(Some((io, found, make)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nintendo::Model;

    #[test]
    fn busy_codes() {
        assert!(is_busy_code(ERROR_SHARING_VIOLATION));
        assert!(is_busy_code(ERROR_ACCESS_DENIED));
        assert!(!is_busy_code(2), "file not found");
    }

    #[test]
    fn grip_slots() {
        assert_eq!(
            grip_slot(r"HID\VID_057E&PID_200E&MI_01\8&1A2B"),
            Some(JoyConSide::Left)
        );
        assert_eq!(
            grip_slot(r"HID\VID_057E&PID_200E&MI_00\8&1A2B"),
            Some(JoyConSide::Right)
        );
        assert_eq!(grip_slot(r"HID\VID_057E&PID_2009\8&1A2B"), None);
    }

    fn entry(path: &str, instance_id: &str) -> InterfaceEntry {
        InterfaceEntry {
            path: path.into(),
            instance_id: instance_id.into(),
            devinst: 0,
        }
    }

    fn caps(usage_page: u16, usage: u16) -> HidCaps {
        HidCaps {
            usage_page,
            usage,
            ..HidCaps::default()
        }
    }

    #[test]
    fn interfaces_are_classified_by_what_they_tell() {
        let usb = entry(
            r"\\?\hid#vid_054c&pid_0ce6&mi_03#7&1a2b&0&0000#{4d1e55b2}",
            r"HID\VID_054C&PID_0CE6&MI_03\7&1A2B&0&0000",
        );
        let bt = entry(
            r"\\?\hid#{00001124-0000-1000-8000-00805f9b34fb}_vid&0002054c_pid&0ce6#9&2#{4d1e55b2}",
            r"HID\{00001124-0000-1000-8000-00805F9B34FB}_VID&0002054C_PID&0CE6\9&2",
        );
        let pad = caps(1, 5);
        let dualsense = (0x054C, 0x0CE6);
        let sony = DriverKind::Sony(SonyModel::DualSense);
        assert_eq!(
            classify(dualsense, pad, &usb, false),
            Some((sony, Transport::Usb))
        );
        assert_eq!(
            classify(dualsense, pad, &bt, false),
            Some((sony, Transport::Bluetooth))
        );
        assert_eq!(classify(dualsense, pad, &usb, true), None, "a virtual DS4");
        assert_eq!(classify(dualsense, caps(1, 4), &usb, false), None);

        // A pad that also speaks XInput is XInput's.
        let generic = (0x0079, 0x0006);
        let xinput = entry(
            r"\\?\hid#vid_0079&pid_0006&ig_00#1",
            r"HID\VID_0079&PID_0006&IG_00\1",
        );
        assert_eq!(classify(generic, pad, &xinput, false), None);
        assert_eq!(
            classify(generic, pad, &usb, false),
            Some((DriverKind::Generic, Transport::Usb))
        );
        // Xbox pads are XInput's in any case.
        assert_eq!(classify((0x045E, 0x0B13), pad, &usb, false), None);
        // The HORI Wireless Switch Pad works over Bluetooth only.
        let hori = (0x0F0D, 0x00F6);
        assert_eq!(classify(hori, pad, &usb, false), None);
        assert_eq!(
            classify(hori, pad, &bt, false),
            Some((DriverKind::Nintendo(Model::Switch1), Transport::Bluetooth))
        );
    }
}
