//! Raw HID controllers on macOS: DualShock 4, DualSense, Nintendo Switch and Switch 2 pads,
//! Steam controllers and other HID gamepads (read through their report descriptor). This is
//! the macOS scanner of the shared HID backend ([`crate::hidhost`]): it lists HID devices
//! through an IOHIDManager, leaves out the stand-ins macOS makes for the controllers it
//! supports itself, and opens the controllers it has drivers for. Switch 2 controllers
//! connected over Bluetooth LE ([`super::ble`]) join them.
//!
//! A controller is hidden by opening it exclusively (seizing it): no other program, the
//! GameController framework included, gets its reports until Doppelpad lets it go.

use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use objc2_core_foundation::CFRetained;
use objc2_io_kit::{IOHIDDevice, IOHIDManager, kIOHIDOptionsTypeNone};

use super::ble;
use super::hidio::HidIo;
use super::props::Props;
use crate::bdaddr::BdAddr;
use crate::generic::ReportDecoder;
use crate::hiddesc::Decoder;
use crate::hidhost::{
    Devices, Driver, DriverKind, Facts, Found, HidBackend, Look, MakeDriver, XboxPads, driver_kind,
    make_driver, scan_pass,
};
use crate::nintendo;
use crate::switch2usb::Locate;
use crate::{DeviceHider, DeviceId, JoyConSide, Result, Switch2Search, Transport};

/// The HID backend with the macOS scanner. When it stops, the Bluetooth LE controllers are let
/// go.
pub fn backend(search: Switch2Search) -> HidBackend {
    ble::start_if_allowed();
    let mut skip = HashMap::new();
    HidBackend::new(
        move |pass, devices| {
            let present = hid_devices().into_iter().map(|d| (path(&d), d)).collect();
            let wireless = ble::poll(search.on());
            scan_pass(
                pass,
                devices,
                &mut skip,
                present,
                wireless,
                |path, device| look(path, device, devices),
            );
        },
        disconnect,
    )
    .on_drop(ble::let_all_go)
}

/// Turning Sony pads off needs their Bluetooth link dropped, which is not done on macOS yet.
fn disconnect(_address: BdAddr) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "dropping a Bluetooth link is not supported on macOS yet",
    ))
}

/// Start the controller behind a HID device, if one of our drivers handles it.
fn look(path: &str, device: CFRetained<IOHIDDevice>, devices: &Devices) -> Look {
    let props = Props::of(&device);
    let Some((kind, driver)) = identify(&props) else {
        return Look::Skip(None);
    };
    match HidIo::open(device, &props) {
        Ok(io) => {
            devices.start(io, found(path, &props, kind), make(&props, driver));
            Look::Started
        }
        Err(e) => {
            tracing::debug!(%path, "cannot open HID device: {e}");
            let busy = super::hidio::is_busy(&e).then(|| {
                let name = props.product.as_deref().unwrap_or("controller");
                crate::holders::busy_message(&format!("the {name}"), None)
            });
            Look::Skip(busy)
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
fn identify(props: &Props) -> Option<(DriverKind, Driver)> {
    if props.synthetic {
        return None; // macOS's stand-in for a controller that is listed itself
    }
    // Without XInput, Xbox pads over Bluetooth are generic HID pads here.
    let kind = driver_kind(
        props.vid,
        props.pid,
        props.usage_page,
        props.usage,
        XboxPads::Here,
    )?;
    if matches!(kind, DriverKind::Nintendo(_))
        && props.link() == Transport::Usb
        && nintendo::usb_unsupported(props.vid, props.pid)
    {
        return None;
    }
    // A generic pad needs a descriptor its reports can be read with.
    let decoder = || Decoder::new(&props.descriptor).map(|d| Box::new(d) as Box<dyn ReportDecoder>);
    Some((kind, Driver::of(kind, decoder)?))
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

fn found(path: &str, props: &Props, kind: DriverKind) -> Found {
    let transport = props.link();
    let bluetooth = transport == Transport::Bluetooth;
    let id = stable_id(
        props.vid,
        props.pid,
        props.serial.as_deref().filter(|_| bluetooth),
        props.location,
        props.interface,
    );
    let bt_address = (bluetooth && matches!(kind, DriverKind::Sony(_)))
        .then(|| props.serial.as_deref().and_then(BdAddr::parse))
        .flatten();
    Found {
        path: path.to_owned(),
        id: DeviceId(id.clone()),
        transport,
        label: kind.label(transport, props.pid),
        vid: props.vid,
        pid: props.pid,
        instance_paths: vec![id],
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

fn make(props: &Props, driver: Driver) -> MakeDriver<HidIo> {
    let facts = Facts {
        vid: props.vid,
        pid: props.pid,
        transport: props.link(),
        product: props.product.clone(),
        feature_len: props.max_feature,
        grip_slot: grip_slot(props.interface),
    };
    let location = props.location;
    Box::new(move |_| {
        make_driver(driver, facts, || Locate {
            // The command channel is on the same USB device, at the same location.
            is_it: Box::new(move |d: &nusb::DeviceInfo| Some(d.location_id()) == location),
            holder: || crate::holders::likely_holder(super::process::running),
            refused: None,
        })
    })
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
        assert!(identify(&stand_in).is_none());
        let generic = Props {
            vid: 0x0079,
            pid: 0x0006,
            usage: 4,
            ..dualsense
        };
        assert!(
            identify(&generic).is_none(),
            "no descriptor to read it with"
        );
    }
}
