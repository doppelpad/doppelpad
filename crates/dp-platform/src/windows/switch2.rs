//! Switch 2 controllers on Windows: the vendor USB interface next to the HID one
//! ([`crate::switch2usb`]), found by the device node of the controller's HID interface.
//!
//! The controller advertises WinUSB itself, so Windows binds it without a driver install.

use nusb::ErrorKind;

use super::devnode::usb_device_of;
use super::process;
use crate::holders::busy_message;
use crate::nintendo::Switch2Model;
use crate::padio::PadDriver;
use crate::switch2usb::{self, Locate, OpenError};
use crate::usb::is_busy;

/// Driver for the Switch 2 controller whose HID interface is device node `devinst`.
pub fn driver(model: Switch2Model, devinst: u32) -> Box<dyn PadDriver> {
    let usb_id = usb_device_of(devinst);
    let locate = Locate {
        is_it: Box::new(move |d: &nusb::DeviceInfo| {
            usb_id
                .as_deref()
                .is_some_and(|id| d.instance_id().to_string_lossy().eq_ignore_ascii_case(id))
        }),
        explain: open_error,
    };
    switch2usb::driver(model, locate)
}

/// A user-facing explanation of why the vendor interface cannot be used.
fn open_error(e: &nusb::Error) -> OpenError {
    if is_busy(e) {
        return OpenError {
            message: busy_message(
                "the controller's USB command interface",
                process::likely_holder(),
            ),
            busy: true,
        };
    }
    match e.kind() {
        ErrorKind::Unsupported | ErrorKind::PermissionDenied => {
            format!("the controller's USB command interface needs the WinUSB driver ({e})")
        }
        ErrorKind::Disconnected => "the controller was disconnected".into(),
        _ => format!("cannot open the controller's USB command interface: {e}"),
    }
    .into()
}
