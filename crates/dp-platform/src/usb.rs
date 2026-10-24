//! nusb helpers shared by the USB backends (GameCube adapter, Switch 2).

use std::io;

use nusb::Interface;
use nusb::descriptors::TransferType;
use nusb::transfer::{Direction, TransferError};

/// The OS error that says another program holds the device: on Windows another process's
/// WinUSB handle makes opening fail with access denied, on macOS IOKit answers with exclusive
/// access.
#[cfg(windows)]
const HELD_ELSEWHERE: u32 = windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
#[cfg(not(windows))]
const HELD_ELSEWHERE: u32 = 0xE000_02C5;

/// Whether another program holds the device or interface. nusb reports `Busy` only for claims
/// within this process; another process's claim shows as [`HELD_ELSEWHERE`].
pub(crate) fn is_busy(e: &nusb::Error) -> bool {
    busy(e.kind(), e.os_error())
}

fn busy(kind: nusb::ErrorKind, os_error: Option<u32>) -> bool {
    kind == nusb::ErrorKind::Busy || os_error == Some(HELD_ELSEWHERE)
}

/// The I/O error for a failed transfer; a cancelled (timed-out) one is `TimedOut`.
pub(crate) fn transfer_error(e: TransferError) -> io::Error {
    let kind = match e {
        TransferError::Cancelled => io::ErrorKind::TimedOut,
        TransferError::Disconnected => io::ErrorKind::NotConnected,
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, e)
}

/// The OUT and IN addresses of the `kind` endpoints of `interface`; `default` for those its
/// descriptor does not list.
pub(crate) fn endpoint_addrs(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_devices() {
        assert!(busy(nusb::ErrorKind::Busy, None));
        assert!(busy(nusb::ErrorKind::Other, Some(HELD_ELSEWHERE)));
        assert!(!busy(nusb::ErrorKind::Other, Some(2)));
        assert!(!busy(nusb::ErrorKind::Unsupported, None));
    }
}
