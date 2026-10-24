//! The USB command channel of Switch 2 controllers: a vendor interface next to the HID one,
//! driven through `nusb`. The controllers stay silent until they are started over it.
//!
//! Each OS finds the channel's USB device for a controller and explains what went wrong when
//! it cannot be opened; claiming it, the transfers and retrying are the same everywhere. The
//! claim is exclusive, so a program that got there first (usually Steam) keeps it. The pad
//! then runs on that program's initialisation, and a helper thread keeps trying to claim the
//! interface so the driver can take it over once it is free.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use nusb::descriptors::TransferType;
use nusb::transfer::{Buffer, Bulk, In, Out};
use nusb::{Endpoint, Interface, MaybeFuture};

use crate::nintendo::switch2::Switch2Driver;
use crate::nintendo::{self, NINTENDO_VID, Switch2Model};
use crate::padio::{self, BulkIo, BulkRetry, PadDriver};
use crate::usb::{endpoint_addrs, transfer_error};

/// Vendor-specific interface class.
const VENDOR_CLASS: u8 = 0xFF;
/// How often to try again to claim a vendor interface another program holds.
const RETRY: Duration = Duration::from_secs(3);

/// Why the vendor interface cannot be used.
pub(crate) struct OpenError {
    /// For the user.
    pub message: String,
    /// Another program holds the interface and may let go of it later.
    pub busy: bool,
}

impl From<String> for OpenError {
    fn from(message: String) -> Self {
        OpenError {
            message,
            busy: false,
        }
    }
}

/// How an OS reaches a controller's channel: whether a listed USB device is the controller's,
/// and what to tell the user about an error opening it.
pub(crate) struct Locate {
    pub is_it: Box<dyn Fn(&nusb::DeviceInfo) -> bool + Send + Sync>,
    pub explain: fn(&nusb::Error) -> OpenError,
}

/// The driver for a Switch 2 controller whose channel `locate` finds.
pub(crate) fn driver(model: Switch2Model, locate: Locate) -> Box<dyn PadDriver> {
    let locate = Arc::new(locate);
    match open(&locate) {
        Ok(b) => Box::new(Switch2Driver::new(model, Ok(Box::new(b)))),
        Err(e) => {
            tracing::warn!("Switch 2 controller: {}", e.message);
            let driver = Switch2Driver::new(model, Err(e.message));
            if !e.busy {
                return Box::new(driver);
            }
            let (late, retry) = padio::late_bulk();
            spawn_retry(locate, retry);
            Box::new(driver.with_retry(late))
        }
    }
}

/// Keep trying to claim the vendor interface until it works, it fails for another reason
/// than being busy, or the driver is gone.
fn spawn_retry(locate: Arc<Locate>, retry: BulkRetry) {
    let spawned = std::thread::Builder::new()
        .name("dp-switch2-claim".into())
        .spawn(move || {
            while retry.wait(RETRY) {
                match open(&locate) {
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

/// Find the controller's USB device and claim its vendor interface.
fn open(locate: &Locate) -> Result<VendorBulk, OpenError> {
    let devices: Vec<nusb::DeviceInfo> = nusb::list_devices()
        .wait()
        .map_err(|e| format!("cannot list USB devices: {e}"))?
        .filter(|d| d.vendor_id() == NINTENDO_VID)
        .collect();
    // Without a match, a single Switch 2 controller is still unambiguous.
    let info = match devices.iter().find(|d| (locate.is_it)(d)) {
        Some(d) => d,
        None => {
            let mut switch2 = devices.iter().filter(|d| {
                matches!(
                    nintendo::model_for(d.vendor_id(), d.product_id()),
                    Some(nintendo::Model::Switch2(_))
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
    let explain = locate.explain;
    let number = info
        .interfaces()
        .find(|i| i.class() == VENDOR_CLASS)
        .map_or(1, |i| i.interface_number());
    let device = info.open().wait().map_err(|e| explain(&e))?;
    let interface = device
        .claim_interface(number)
        .wait()
        .map_err(|e| explain(&e))?;
    let (out_addr, in_addr) = endpoint_addrs(&interface, TransferType::Bulk, (0x02, 0x82));
    let out = interface
        .endpoint::<Bulk, Out>(out_addr)
        .map_err(|e| explain(&e))?;
    let input = interface
        .endpoint::<Bulk, In>(in_addr)
        .map_err(|e| explain(&e))?;
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
