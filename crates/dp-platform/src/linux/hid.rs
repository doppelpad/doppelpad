//! The HID backend on Linux ([`crate::hidhost`]), for the controllers evdev cannot read: Switch
//! 2 controllers, which no kernel driver starts. On USB it reads them through hidraw
//! ([`super::hidraw`]) and starts them over their command channel ([`crate::switch2usb`]);
//! over Bluetooth LE they come from [`super::ble`].

use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use super::ble;
use super::gamepad::GrabHider;
use super::hidraw::{self, Entry, Hidraw};
use crate::hidhost::{Devices, Found, HidBackend, Look, MakeDriver, scan_pass};
use crate::switch2usb::{self, Locate};
use crate::{DeviceHider, DeviceId, Result, Switch2Search, Transport};

/// The start of the ids of the controllers on USB, which tells their instance paths from
/// evdev's.
const ID_PREFIX: &str = "hid:";

/// The backend: Switch 2 controllers on USB and over Bluetooth. When it stops, the Bluetooth
/// ones are let go.
pub(crate) fn backend(search: Switch2Search) -> HidBackend {
    let mut skip = HashMap::new();
    HidBackend::new(
        move |pass, devices| {
            let present = hidraw::switch2_devices()
                .into_iter()
                .map(|e| (node(&e), e))
                .collect();
            let wireless = ble::poll(search.on());
            scan_pass(
                pass,
                devices,
                &mut skip,
                present,
                wireless,
                |path, entry| look(path, &entry, devices),
            );
        },
        ble::turn_off,
    )
    .on_drop(ble::let_all_go)
}

/// Start a Switch 2 controller on USB.
fn look(path: &str, entry: &Entry, devices: &Devices) -> Look {
    match Hidraw::open(entry) {
        Ok(io) => {
            devices.start(io, found(entry), driver(entry));
            Look::Started
        }
        Err(e) => {
            tracing::debug!(path, "cannot open the Switch 2 controller: {e}");
            Look::Skip((e.kind() == io::ErrorKind::PermissionDenied).then(|| {
                "a Switch 2 controller on USB cannot be read: it needs the udev rule from \
                 docs/LINUX.md"
                    .to_owned()
            }))
        }
    }
}

fn node(entry: &Entry) -> String {
    entry.node.display().to_string()
}

fn found(entry: &Entry) -> Found {
    // Where it is plugged in, as evdev ids name a controller on USB.
    let id = format!(
        "{ID_PREFIX}{:04x}:{:04x}:{}",
        entry.vid, entry.pid, entry.phys
    );
    Found {
        path: node(entry),
        id: DeviceId(id.clone()),
        transport: Transport::Usb,
        label: "USB",
        vid: entry.vid,
        pid: entry.pid,
        // Its device thread hides it (see `Hidraw::set_exclusive`).
        instance_paths: vec![id],
        bt_address: None,
    }
}

fn driver(entry: &Entry) -> MakeDriver<Hidraw> {
    let (model, usb_device) = (entry.model, entry.usb_device.clone());
    Box::new(move |_io| {
        // The command channel is on the same USB device. nusb names it by its sysfs path
        // under `/sys/bus/usb/devices`, sysfs by its place in the device tree; both end in the
        // device's port (`1-1.3`).
        let locate = Locate {
            is_it: Box::new(move |d: &nusb::DeviceInfo| {
                d.sysfs_path().file_name() == usb_device.file_name()
            }),
            holder: || crate::holders::likely_holder(super::procname::running),
            refused: Some("it needs the udev rule from docs/LINUX.md"),
        };
        switch2usb::driver(model, locate)
    })
}

/// Hides the controllers of both backends: evdev's by grabbing their nodes, the HID backend's
/// by having their device threads grab the input nodes the kernel made for them.
pub(crate) struct Hider {
    grab: GrabHider,
    hid: Arc<Devices>,
}

impl Hider {
    pub fn new(grab: GrabHider, backend: &HidBackend) -> Hider {
        Hider {
            grab,
            hid: backend.devices(),
        }
    }
}

impl DeviceHider for Hider {
    fn available(&self) -> Result<()> {
        self.grab.available()
    }

    fn hide(&mut self, instance_paths: &[String]) -> Result<()> {
        let (hid, evdev): (Vec<String>, Vec<String>) = instance_paths
            .iter()
            .cloned()
            .partition(|p| p.starts_with(ID_PREFIX));
        self.hid.set_hidden(&hid);
        if !hid.is_empty() {
            tracing::info!(devices = ?hid, "devices hidden via EVIOCGRAB of their input nodes");
        }
        self.grab.hide(&evdev)
    }

    fn restore(&mut self) -> Result<()> {
        self.hid.set_hidden(&[]);
        self.grab.restore()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::nintendo::Switch2Model;

    fn pro() -> Entry {
        Entry {
            node: PathBuf::from("/dev/hidraw11"),
            vid: 0x057E,
            pid: 0x2069,
            model: Switch2Model::Pro,
            phys: "usb-0000:04:00.3-1.3/input0".into(),
            usb_device: PathBuf::from(
                "/sys/devices/pci0000:00/0000:00:08.1/0000:04:00.3/usb1/1-1/1-1.3",
            ),
            event_nodes: vec![PathBuf::from("/dev/input/event11")],
        }
    }

    #[test]
    fn controllers_on_usb_are_found_by_their_port() {
        let f = found(&pro());
        assert_eq!(f.path, "/dev/hidraw11");
        assert_eq!(f.id.0, "hid:057e:2069:usb-0000:04:00.3-1.3/input0");
        assert_eq!(f.instance_paths, vec![f.id.0.clone()], "hidden by its id");
        assert_eq!((f.transport, f.label), (Transport::Usb, "USB"));
    }

    /// Starts the first Switch 2 controller on USB and drives it (see
    /// [`crate::hidhost::testing::drive`]); unplug it at the end. Run with `--nocapture`.
    #[test]
    #[ignore = "starts a Switch 2 controller on USB on this computer"]
    fn switch2_on_usb() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("dp_platform=debug")
            .try_init();
        let entry = hidraw::switch2_devices()
            .into_iter()
            .next()
            .expect("no Switch 2 controller on USB");
        let mut io = Hidraw::open(&entry).expect("its hidraw node opens");
        let id = found(&entry).id.0;
        let driver = driver(&entry)(&mut io);
        crate::hidhost::testing::drive(&id, driver, &mut io, || {});
    }
}
