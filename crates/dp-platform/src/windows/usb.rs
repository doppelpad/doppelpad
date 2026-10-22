//! nusb helpers shared by the USB backends (GameCube adapter, Switch 2).

use std::io;

use nusb::Interface;
use nusb::descriptors::TransferType;
use nusb::transfer::{Direction, TransferError};

/// The I/O error for a failed transfer; a cancelled (timed-out) one is `TimedOut`.
pub(super) fn transfer_error(e: TransferError) -> io::Error {
    let kind = match e {
        TransferError::Cancelled => io::ErrorKind::TimedOut,
        TransferError::Disconnected => io::ErrorKind::NotConnected,
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, e)
}

/// The OUT and IN addresses of the `kind` endpoints of `interface`; `default` for those its
/// descriptor does not list.
pub(super) fn endpoint_addrs(
    interface: &Interface,
    kind: TransferType,
    default: (u8, u8),
) -> (u8, u8) {
    let (mut out, mut input) = default;
    if let Some(desc) = interface.descriptor() {
        for ep in desc.endpoints().filter(|ep| ep.transfer_type() == kind) {
            match ep.direction() {
                Direction::Out => out = ep.address(),
                Direction::In => input = ep.address(),
            }
        }
    }
    (out, input)
}
