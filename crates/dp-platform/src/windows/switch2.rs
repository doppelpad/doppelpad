//! Switch 2 controllers on Windows: the vendor USB interface next to the HID one.
//!
//! The vendor interface is driven through WinUSB (the `nusb` crate). The controller
//! advertises WinUSB itself, so Windows binds it without a driver install; WinUSB handles
//! are exclusive, though, so a program that got there first (usually Steam) keeps it.
//! The pad then runs on that program's initialisation, and a helper thread keeps trying
//! to claim the interface so the driver can take it over once it is free.

use std::io;
use std::time::Duration;

use nusb::descriptors::TransferType;
use nusb::transfer::{Buffer, Bulk, In, Out};
use nusb::{Endpoint, ErrorKind, Interface, MaybeFuture};
use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;

use super::devnode::usb_device_of;
use super::process;
use super::usb::{endpoint_addrs, transfer_error};
use crate::nintendo::switch2::Switch2Driver;
use crate::nintendo::{NINTENDO_VID, Switch2Model};
use crate::padio::{self, BulkIo, BulkRetry, PadDriver};

/// Vendor-specific interface class.
const VENDOR_CLASS: u8 = 0xFF;
/// How often to try again to claim a vendor interface another program holds.
const RETRY: Duration = Duration::from_secs(3);

/// Driver for the Switch 2 controller whose HID interface is device node `devinst`.
pub fn driver(model: Switch2Model, devinst: u32) -> Box<dyn PadDriver> {
    match open_vendor_bulk(devinst) {
        Ok(b) => Box::new(Switch2Driver::new(model, Ok(Box::new(b)))),
        Err(e) => {
            tracing::warn!("Switch 2 controller: {}", e.message);
            let driver = Switch2Driver::new(model, Err(e.message));
            if !e.busy {
                return Box::new(driver);
            }
            let (late, retry) = padio::late_bulk();
            spawn_retry(devinst, retry);
            Box::new(driver.with_retry(late))
        }
    }
}

/// Keep trying to claim the vendor interface until it works, it fails for another reason
/// than being busy, or the driver is gone.
fn spawn_retry(devinst: u32, retry: BulkRetry) {
    let spawned = std::thread::Builder::new()
        .name("dp-switch2-claim".into())
        .spawn(move || {
            while retry.wait(RETRY) {
                match open_vendor_bulk(devinst) {
                    Ok(b) => {
                        retry.hand_over(Box::new(b));
                        return;
                    }
                    Err(e) if e.busy => {}
                    Err(e) => {
                        tracing::debug!("Switch 2 controller: no longer retrying: {}", e.message);
                        return;
                    }
                }
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("cannot start retrying the Switch 2 command interface: {e}");
    }
}

/// Why the vendor interface cannot be used.
struct OpenError {
    /// For the user.
    message: String,
    /// Another program holds the interface and may let go of it later.
    busy: bool,
}

impl From<String> for OpenError {
    fn from(message: String) -> Self {
        OpenError {
            message,
            busy: false,
        }
    }
}

/// The claimed vendor interface and its two bulk endpoints.
struct VendorBulk {
    out: Endpoint<Bulk, Out>,
    input: Endpoint<Bulk, In>,
    // Keeps the interface claimed (and other programs out) while the controller is used.
    _interface: Interface,
}

impl BulkIo for VendorBulk {
    fn command(&mut self, cmd: &[u8], reply_len: usize, timeout: Duration) -> io::Result<Vec<u8>> {
        self.out
            .transfer_blocking(Buffer::from(cmd.to_vec()), timeout)
            .status
            .map_err(transfer_error)?;
        let packet = self.input.max_packet_size();
        let mut reply = Vec::new();
        while reply.len() < reply_len {
            let done = self.input.transfer_blocking(Buffer::new(packet), timeout);
            done.status.map_err(transfer_error)?;
            reply.extend_from_slice(&done.buffer[..done.actual_len]);
            if done.actual_len < packet {
                break;
            }
        }
        Ok(reply)
    }
}

fn open_vendor_bulk(hid_devinst: u32) -> Result<VendorBulk, OpenError> {
    let usb_id = usb_device_of(hid_devinst);
    let devices: Vec<nusb::DeviceInfo> = nusb::list_devices()
        .wait()
        .map_err(|e| format!("cannot list USB devices: {e}"))?
        .filter(|d| d.vendor_id() == NINTENDO_VID)
        .collect();
    let by_id = usb_id.as_deref().and_then(|id| {
        devices
            .iter()
            .find(|d| d.instance_id().to_string_lossy().eq_ignore_ascii_case(id))
    });
    // Without a match by instance ID, a single Switch 2 controller is still unambiguous.
    let info = match by_id {
        Some(d) => d,
        None => {
            let mut switch2 = devices.iter().filter(|d| {
                matches!(
                    crate::nintendo::model_for(d.vendor_id(), d.product_id()),
                    Some(crate::nintendo::Model::Switch2(_))
                )
            });
            match (switch2.next(), switch2.next()) {
                (Some(d), None) => d,
                _ => {
                    return Err(String::from(
                        "the controller's USB command interface was not found",
                    )
                    .into());
                }
            }
        }
    };
    let number = info
        .interfaces()
        .find(|i| i.class() == VENDOR_CLASS)
        .map_or(1, |i| i.interface_number());
    let device = info.open().wait().map_err(|e| open_error(&e))?;
    let interface = device
        .claim_interface(number)
        .wait()
        .map_err(|e| open_error(&e))?;
    let (out_addr, in_addr) = endpoint_addrs(&interface, TransferType::Bulk, (0x02, 0x82));
    let out = interface
        .endpoint::<Bulk, Out>(out_addr)
        .map_err(|e| open_error(&e))?;
    let input = interface
        .endpoint::<Bulk, In>(in_addr)
        .map_err(|e| open_error(&e))?;
    tracing::debug!(
        interface = number,
        out_addr,
        in_addr,
        "Switch 2 vendor interface claimed"
    );
    Ok(VendorBulk {
        out,
        input,
        _interface: interface,
    })
}

/// A user-facing explanation of why the vendor interface cannot be used.
fn open_error(e: &nusb::Error) -> OpenError {
    if is_busy(e.kind(), e.os_error()) {
        return OpenError {
            message: busy_message(process::running("steam.exe")),
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

/// Whether another program holds the interface. nusb reports `Busy` only for claims within
/// this process; another process's WinUSB handle makes opening fail with access denied.
fn is_busy(kind: ErrorKind, os_error: Option<u32>) -> bool {
    kind == ErrorKind::Busy || os_error == Some(ERROR_ACCESS_DENIED)
}

fn busy_message(steam: bool) -> String {
    let holder = if steam { "Steam" } else { "another program" };
    format!("{holder} is using the controller's USB command interface")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_interfaces() {
        assert!(is_busy(ErrorKind::Busy, None));
        assert!(is_busy(ErrorKind::Other, Some(ERROR_ACCESS_DENIED)));
        assert!(!is_busy(ErrorKind::Other, Some(2)));
        assert!(!is_busy(ErrorKind::Unsupported, None));
        assert_eq!(
            busy_message(true),
            "Steam is using the controller's USB command interface"
        );
        assert!(busy_message(false).starts_with("another program "));
    }
}
