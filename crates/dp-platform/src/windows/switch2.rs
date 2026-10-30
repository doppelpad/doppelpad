//! Switch 2 controllers on Windows: the vendor USB interface next to the HID one
//! ([`crate::switch2usb`]), found by the device node of the controller's HID interface.
//!
//! The controller advertises WinUSB itself, so Windows binds it without a driver install.

use super::devnode::usb_device_of;
use super::process;
use crate::switch2usb::Locate;

/// How the command channel of the Switch 2 controller whose HID interface is device node
/// `devinst` is found.
pub fn locate(devinst: u32) -> Locate {
    let usb_id = usb_device_of(devinst);
    Locate {
        is_it: Box::new(move |d: &nusb::DeviceInfo| {
            usb_id
                .as_deref()
                .is_some_and(|id| d.instance_id().to_string_lossy().eq_ignore_ascii_case(id))
        }),
        holder: process::likely_holder,
        refused: Some("it needs the WinUSB driver"),
    }
}
