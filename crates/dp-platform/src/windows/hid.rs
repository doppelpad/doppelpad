//! Raw HID controllers XInput cannot see: DualShock 4, DualSense, Nintendo Switch pads, Steam
//! controllers and other HID gamepads (read through their report descriptor). This is the
//! Windows scanner of the shared HID backend ([`crate::hidhost`]): it finds HID interfaces
//! through SetupAPI, opens them with overlapped I/O and picks their drivers. Switch 2
//! controllers connected over Bluetooth LE ([`super::ble`]) join them.

use std::collections::HashMap;
use std::time::Instant;

use windows_sys::Win32::Devices::HumanInterfaceDevice::GUID_DEVINTERFACE_HID;
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION};

use super::devnode::{InterfaceEntry, enumerate_interfaces, is_vigem_device, product_name};
use super::hidio::HidIo;
use super::hidparse::HidDecoder;
use crate::generic::{GenericDriver, RawInput, ReportDecoder};
use crate::hidhost::{
    Devices, DriverKind, Found, HidBackend, MakeDriver, SKIP_FOREIGN, driver_kind, parse_bt_address,
};
use crate::nintendo::switch1::{Link, Switch1Driver};
use crate::nintendo::wired::WiredDriver;
use crate::nintendo::{self, Model};
use crate::scan::Pass;
use crate::sony::{self, SonyDriver, SonyModel};
use crate::steam::SteamDriver;
use crate::{DeviceId, JoyConSide};

/// The HID backend with the Windows scanner.
pub fn backend() -> HidBackend {
    let mut scan = Scanner {
        skip: HashMap::new(),
    };
    HidBackend::new(
        move |pass, devices| scan.scan(pass, devices),
        super::bluetooth::disconnect,
    )
}

/// Generic pads whose reports the HID parser cannot read: nothing to report.
struct NoDecoder;

impl ReportDecoder for NoDecoder {
    fn decode(&mut self, _report: &[u8]) -> Option<RawInput> {
        None
    }
}

/// The Bluetooth HID service class, which Bluetooth HID devices carry in their paths.
const BT_HID_SERVICE: &str = "00001124-0000-1000-8000-00805f9b34fb";

fn is_bluetooth(entry: &InterfaceEntry) -> bool {
    entry.instance_id.to_ascii_uppercase().contains("BTH")
        || entry.path.to_ascii_lowercase().contains(BT_HID_SERVICE)
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

/// What the scanner knows about a device before its driver starts.
#[derive(Clone, Debug)]
struct Base {
    path: String,
    instance_id: String,
    devinst: u32,
    vid: u16,
    pid: u16,
    bluetooth: bool,
    kind: DriverKind,
    feature_len: usize,
    /// The controller's Bluetooth address (Sony pads report it as their serial number).
    bt_address: Option<u64>,
}

impl Base {
    fn found(&self) -> Found {
        Found {
            path: self.path.clone(),
            id: DeviceId(format!("hid:{}", self.instance_id)),
            transport: self.kind.transport(self.bluetooth, self.pid),
            vid: self.vid,
            pid: self.pid,
            instance_paths: vec![self.instance_id.clone()],
            bt_address: self.bt_address,
        }
    }

    /// Builds the driver; `decoder` is the report decoder `identify` built for a generic pad.
    fn driver(&self, decoder: Option<HidDecoder>) -> MakeDriver<HidIo> {
        let base = self.clone();
        Box::new(move |io| match base.kind {
            DriverKind::Generic => {
                let name = io.product().unwrap_or_else(|| "Game controller".into());
                let decoder: Box<dyn ReportDecoder> = match decoder {
                    Some(d) => Box::new(d),
                    None => Box::new(NoDecoder),
                };
                Box::new(GenericDriver::new(name, base.vid, base.pid, decoder))
            }
            DriverKind::Sony(m) => Box::new(SonyDriver::new(m, base.bluetooth, base.feature_len)),
            DriverKind::Nintendo(Model::Switch1) => Box::new(Switch1Driver::new(Link {
                bluetooth: base.bluetooth,
                vid: base.vid,
                pid: base.pid,
                grip_slot: if base.pid == nintendo::pid::CHARGING_GRIP {
                    grip_slot(&base.instance_id)
                } else {
                    None
                },
            })),
            DriverKind::Nintendo(Model::Wired) => Box::new(WiredDriver::new(base.vid)),
            DriverKind::Nintendo(Model::Switch2(m)) => super::switch2::driver(m, base.devinst),
            DriverKind::Steam(m) => Box::new(SteamDriver::new(m)),
        })
    }
}

/// Discovery: which HID interfaces are there, and which of them are controllers.
struct Scanner {
    /// Paths not to reopen before the given time.
    skip: HashMap<String, Instant>,
}

impl Scanner {
    fn scan(&mut self, pass: &Pass, devices: &Devices) {
        if pass.requested {
            self.skip.clear();
        }
        let now = Instant::now();
        self.skip.retain(|_, until| now < *until);
        let present = enumerate_interfaces(&GUID_DEVINTERFACE_HID);
        let wireless = super::ble::poll();
        devices.sweep(
            |path| present.iter().any(|e| e.path == path) || wireless.connected.contains(path),
            &mut self.skip,
            now,
        );
        for pad in wireless.ready {
            // Nothing to hide: no other program sees it.
            pad.start(devices, Vec::new());
        }

        for entry in present {
            if pass.stopping() {
                return;
            }
            if self.skip.contains_key(&entry.path) || devices.has(&entry.path) {
                continue;
            }
            match identify(&entry) {
                Ok(Some((io, base, decoder))) => {
                    let make_driver = base.driver(decoder);
                    devices.start(io, base.found(), make_driver);
                }
                Ok(None) => {
                    self.skip.insert(entry.path, now + SKIP_FOREIGN);
                }
                Err(e) => {
                    tracing::debug!(path = %entry.path, "cannot open HID device: {e}");
                    if let Some(problem) = busy_problem(&entry, &e) {
                        devices.set_problem(&entry.path, Some(problem));
                    }
                    self.skip.insert(entry.path, now + SKIP_FOREIGN);
                }
            }
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

/// Open an interface and decide whether one of our drivers handles it (with the report
/// decoder of a generic pad).
fn identify(entry: &InterfaceEntry) -> std::io::Result<Option<(HidIo, Base, Option<HidDecoder>)>> {
    let io = HidIo::open(&entry.path)?;
    let Some((vid, pid)) = io.ids() else {
        return Ok(None);
    };
    let caps = io.caps();
    // Xbox pads are read through XInput.
    let Some(kind) = driver_kind(vid, pid, caps.usage_page, caps.usage, true) else {
        return Ok(None);
    };
    if is_vigem_device(entry.devinst) {
        return Ok(None); // our own (or someone else's) virtual DS4
    }
    // XInput reads pads that also speak XInput (`IG_` in the path); the parser must be
    // able to read the others.
    let decoder = match kind {
        DriverKind::Generic if entry.path.to_ascii_uppercase().contains("&IG_") => return Ok(None),
        DriverKind::Generic => match io.decoder() {
            Some(d) => Some(d),
            None => return Ok(None),
        },
        _ => None,
    };
    let bluetooth = is_bluetooth(entry);
    if !bluetooth && matches!(kind, DriverKind::Nintendo(_)) && nintendo::usb_unsupported(vid, pid)
    {
        return Ok(None);
    }
    let bt_address = (bluetooth && matches!(kind, DriverKind::Sony(_)))
        .then(|| io.serial().and_then(|s| parse_bt_address(&s)))
        .flatten();
    Ok(Some((
        io,
        Base {
            path: entry.path.clone(),
            instance_id: entry.instance_id.clone(),
            devinst: entry.devinst,
            vid,
            pid,
            bluetooth,
            kind,
            feature_len: caps.feature_len,
            bt_address,
        },
        decoder,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
