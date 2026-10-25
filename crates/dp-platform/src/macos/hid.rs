//! Raw HID controllers on macOS: DualShock 4, DualSense, Nintendo Switch and Switch 2 pads,
//! Steam controllers and other HID gamepads (read through their report descriptor). This is the macOS scanner of the shared HID backend ([`crate::hidhost`]): it
//! lists HID devices through an IOHIDManager, leaves out the stand-ins macOS makes for the
//! controllers it supports itself, and opens the controllers it has drivers for. Switch 2
//! controllers connected over Bluetooth LE ([`super::ble`]) join them.
//!
//! A controller is hidden by opening it exclusively (seizing it): no other program, the
//! GameController framework included, gets its reports until Doppelpad lets it go.

use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::Instant;

use objc2_core_foundation::CFRetained;
use objc2_io_kit::{IOHIDDevice, IOHIDManager, kIOHIDOptionsTypeNone};

use super::ble;
use super::hidio::HidIo;
use super::props::Props;
use crate::generic::GenericDriver;
use crate::hiddesc::Decoder;
use crate::hidhost::{
    Devices, DriverKind, Found, HidBackend, MakeDriver, SKIP_FOREIGN, driver_kind, parse_bt_address,
};
use crate::nintendo::switch1::{Link, Switch1Driver};
use crate::nintendo::wired::WiredDriver;
use crate::nintendo::{self, Model};
use crate::scan::Pass;
use crate::sony::SonyDriver;
use crate::steam::SteamDriver;
use crate::switch2usb::{self, Locate, OpenError};
use crate::{DeviceHider, DeviceId, JoyConSide, Result};

/// The HID backend with the macOS scanner.
pub fn backend() -> HidBackend {
    ble::start_if_allowed();
    let mut scan = Scanner {
        skip: HashMap::new(),
    };
    HidBackend::new(move |pass, devices| scan.scan(pass, devices), disconnect)
}

/// Turning Sony pads off needs their Bluetooth link dropped, which is not done on macOS yet.
fn disconnect(_address: u64) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "dropping a Bluetooth link is not supported on macOS yet",
    ))
}

/// Discovery: which HID devices are there, and which of them are controllers.
struct Scanner {
    /// Devices not to look at again before the given time, by path.
    skip: HashMap<String, Instant>,
}

impl Scanner {
    fn scan(&mut self, pass: &Pass, devices: &Devices) {
        if pass.requested {
            self.skip.clear();
        }
        let now = Instant::now();
        self.skip.retain(|_, until| now < *until);
        let present: Vec<(String, CFRetained<IOHIDDevice>)> =
            hid_devices().into_iter().map(|d| (path(&d), d)).collect();
        let wireless = ble::poll();
        devices.sweep(
            |path| present.iter().any(|(p, _)| p == path) || wireless.connected.contains(path),
            &mut self.skip,
            now,
        );
        for pad in wireless.ready {
            // Hidden as any other (seizing it does nothing; no other program sees it).
            let hide = vec![pad.id.clone()];
            pad.start(devices, hide);
        }

        for (path, device) in present {
            if pass.stopping() {
                return;
            }
            if self.skip.contains_key(&path) || devices.has(&path) {
                continue;
            }
            let props = Props::of(&device);
            let Some(kind) = identify(&props) else {
                self.skip.insert(path, now + SKIP_FOREIGN);
                continue;
            };
            match HidIo::open(device, &props) {
                Ok(io) => {
                    let found = found(path, &props, kind);
                    devices.start(io, found, driver(&props, kind));
                }
                Err(e) => {
                    tracing::debug!(%path, "cannot open HID device: {e}");
                    if super::hidio::is_busy(&e) {
                        let name = props.product.as_deref().unwrap_or("controller");
                        let problem = crate::holders::busy_message(&format!("the {name}"), None);
                        devices.set_problem(&path, Some(problem));
                    }
                    self.skip.insert(path, now + SKIP_FOREIGN);
                }
            }
        }
    }
}

/// Every HID device there is now.
fn hid_devices() -> Vec<CFRetained<IOHIDDevice>> {
    let manager = IOHIDManager::new(None, kIOHIDOptionsTypeNone);
    // SAFETY: no matching dictionary matches every device.
    unsafe { manager.set_device_matching(None) };
    let Some(set) = manager.devices() else {
        return Vec::new();
    };
    let mut raw = vec![std::ptr::null(); usize::try_from(set.count()).unwrap_or(0)];
    // SAFETY: the buffer has room for every value of the set.
    unsafe { set.values(raw.as_mut_ptr()) };
    raw.into_iter()
        .filter_map(|p| {
            let p = std::ptr::NonNull::new(p.cast_mut().cast::<IOHIDDevice>())?;
            // SAFETY: the manager's set holds IOHIDDevices; retaining one keeps it alive past
            // the set.
            Some(unsafe { CFRetained::retain(p) })
        })
        .collect()
}

/// The key a device is listed under: its registry entry, unique while it is connected.
fn path(device: &IOHIDDevice) -> String {
    let mut id = 0;
    // SAFETY: the service of a live device and a valid out-pointer.
    unsafe { objc2_io_kit::IORegistryEntryGetRegistryEntryID(device.service(), &mut id) };
    format!("{id:#x}")
}

/// The driver for a device, if one handles it.
fn identify(props: &Props) -> Option<DriverKind> {
    if props.synthetic {
        return None; // macOS's stand-in for a controller that is listed itself
    }
    // Without XInput, Xbox pads over Bluetooth are generic HID pads here.
    let kind = driver_kind(props.vid, props.pid, props.usage_page, props.usage, false)?;
    match kind {
        DriverKind::Generic if Decoder::new(&props.descriptor).is_none() => None,
        DriverKind::Nintendo(_)
            if !props.bluetooth() && nintendo::usb_unsupported(props.vid, props.pid) =>
        {
            None
        }
        _ => Some(kind),
    }
}

/// The id of a controller, stable across reconnects: over Bluetooth its address (which it
/// reports as its serial number), over USB the port it is on, and the interface on a device with
/// several. USB serial numbers are no use: every Switch Pro Controller reports `000000000001`.
fn stable_id(
    vid: u16,
    pid: u16,
    bt_serial: Option<&str>,
    location: Option<u32>,
    interface: Option<u8>,
) -> String {
    let place = match (bt_serial, location) {
        (Some(s), _) => s.to_owned(),
        (None, Some(l)) => format!("{l:08x}"),
        (None, None) => "?".to_owned(),
    };
    match interface {
        Some(i) => format!("hid:{vid:04x}:{pid:04x}:{place}/{i}"),
        None => format!("hid:{vid:04x}:{pid:04x}:{place}"),
    }
}

fn found(path: String, props: &Props, kind: DriverKind) -> Found {
    let bluetooth = props.bluetooth();
    let id = stable_id(
        props.vid,
        props.pid,
        props.serial.as_deref().filter(|_| bluetooth),
        props.location,
        props.interface,
    );
    let bt_address = (bluetooth && matches!(kind, DriverKind::Sony(_)))
        .then(|| props.serial.as_deref().and_then(parse_bt_address))
        .flatten();
    Found {
        path,
        id: DeviceId(id.clone()),
        transport: kind.transport(bluetooth, props.pid),
        vid: props.vid,
        pid: props.pid,
        instance_paths: vec![id],
        bluetooth,
        bt_address,
    }
}

/// Which Joy-Con a charging grip's interface holds: interface 1 the left one.
fn grip_slot(interface: Option<u8>) -> Option<JoyConSide> {
    match interface? {
        0 => Some(JoyConSide::Right),
        1 => Some(JoyConSide::Left),
        _ => None,
    }
}

fn driver(props: &Props, kind: DriverKind) -> MakeDriver<HidIo> {
    let (vid, pid, bluetooth) = (props.vid, props.pid, props.bluetooth());
    let (feature_len, interface, location) = (props.max_feature, props.interface, props.location);
    let name = props
        .product
        .clone()
        .unwrap_or_else(|| "Game controller".into());
    let descriptor = props.descriptor.clone();
    Box::new(move |_io| match kind {
        DriverKind::Generic => match Decoder::new(&descriptor) {
            Some(d) => Box::new(GenericDriver::new(name, vid, pid, Box::new(d))),
            None => unreachable!("identified with a decoder"),
        },
        DriverKind::Sony(m) => Box::new(SonyDriver::new(m, bluetooth, feature_len)),
        DriverKind::Nintendo(Model::Switch1) => Box::new(Switch1Driver::new(Link {
            bluetooth,
            vid,
            pid,
            grip_slot: if pid == nintendo::pid::CHARGING_GRIP {
                grip_slot(interface)
            } else {
                None
            },
        })),
        DriverKind::Nintendo(Model::Wired) => Box::new(WiredDriver::new(vid)),
        DriverKind::Steam(m) => Box::new(SteamDriver::new(m)),
        DriverKind::Nintendo(Model::Switch2(m)) => {
            // The command channel is on the same USB device, at the same location.
            let locate = Locate {
                is_it: Box::new(move |d: &nusb::DeviceInfo| Some(d.location_id()) == location),
                explain: usb_open_error,
            };
            switch2usb::driver(m, locate)
        }
    })
}

/// A user-facing explanation of why the Switch 2 command interface cannot be used.
fn usb_open_error(e: &nusb::Error) -> OpenError {
    if crate::usb::is_busy(e) {
        let holder = crate::holders::likely_holder(super::process::running);
        return OpenError {
            message: crate::holders::busy_message("the controller's USB command interface", holder),
            busy: true,
        };
    }
    match e.kind() {
        nusb::ErrorKind::Disconnected => "the controller was disconnected".to_owned(),
        _ => format!("cannot open the controller's USB command interface: {e}"),
    }
    .into()
}

/// Hides controllers by seizing them (see the module docs).
pub struct SeizeHider {
    devices: Arc<Devices>,
}

impl SeizeHider {
    pub fn new(backend: &HidBackend) -> SeizeHider {
        SeizeHider {
            devices: backend.devices(),
        }
    }
}

impl DeviceHider for SeizeHider {
    fn available(&self) -> Result<()> {
        Ok(())
    }

    fn hide(&mut self, instance_paths: &[String]) -> Result<()> {
        self.devices.set_hidden(instance_paths);
        if !instance_paths.is_empty() {
            tracing::info!(devices = ?instance_paths, "devices hidden by seizing them");
        }
        Ok(())
    }

    fn restore(&mut self) -> Result<()> {
        self.devices.set_hidden(&[]);
        Ok(())
    }

    fn hides_without_virtual_pad(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_ids() {
        assert_eq!(
            stable_id(0x054C, 0x0CE6, None, Some(0x0110_0000), Some(3)),
            "hid:054c:0ce6:01100000/3"
        );
        assert_eq!(
            stable_id(0x054C, 0x0CE6, Some("10-18-49-75-fc-86"), Some(1), None),
            "hid:054c:0ce6:10-18-49-75-fc-86"
        );
        assert_eq!(
            stable_id(0x057E, 0x2009, None, None, None),
            "hid:057e:2009:?"
        );
    }

    #[test]
    fn grip_slots() {
        assert_eq!(grip_slot(Some(1)), Some(JoyConSide::Left));
        assert_eq!(grip_slot(Some(0)), Some(JoyConSide::Right));
        assert_eq!(grip_slot(Some(2)), None);
        assert_eq!(grip_slot(None), None);
    }

    #[test]
    fn apple_stand_ins_and_unreadable_pads_are_left_out() {
        let dualsense = Props {
            vid: 0x054C,
            pid: 0x0CE6,
            usage_page: 1,
            usage: 5,
            transport: "USB".into(),
            ..Props::default()
        };
        assert!(identify(&dualsense).is_some());
        let stand_in = Props {
            vid: 0x045E,
            pid: 0x028E,
            synthetic: true,
            ..dualsense.clone()
        };
        assert_eq!(identify(&stand_in), None);
        let generic = Props {
            vid: 0x0079,
            pid: 0x0006,
            usage: 4,
            ..dualsense
        };
        assert_eq!(identify(&generic), None, "no descriptor to read it with");
    }
}
