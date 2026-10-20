//! Disconnecting a Bluetooth device from the local radio, which turns a controller off
//! without unpairing it.

use std::io;
use std::ptr::null_mut;

use windows_sys::Win32::Devices::Bluetooth::{
    BLUETOOTH_FIND_RADIO_PARAMS, BluetoothFindFirstRadio, BluetoothFindRadioClose,
};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::IO::DeviceIoControl;

/// `CTL_CODE(FILE_DEVICE_BLUETOOTH, 0x03, METHOD_BUFFERED, FILE_ANY_ACCESS)`.
const IOCTL_BTH_DISCONNECT_DEVICE: u32 = 0x0041_000C;

/// A Bluetooth address from text such as `a4:ae:12:34:56:78` or `a4ae12345678`.
pub fn parse_address(s: &str) -> Option<u64> {
    let hex: String = s.chars().filter(char::is_ascii_hexdigit).collect();
    (hex.len() == 12).then(|| u64::from_str_radix(&hex, 16).ok())?
}

/// Ask the first radio to drop the connection to `address`.
pub fn disconnect(address: u64) -> io::Result<()> {
    let params = BLUETOOTH_FIND_RADIO_PARAMS {
        dwSize: size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32,
    };
    let mut radio: HANDLE = null_mut();
    // SAFETY: valid parameter block and out-pointer; the handles are closed below.
    unsafe {
        let find = BluetoothFindFirstRadio(&params, &mut radio);
        if find.is_null() {
            return Err(io::Error::last_os_error());
        }
        let addr = address;
        let mut returned = 0u32;
        let ok = DeviceIoControl(
            radio,
            IOCTL_BTH_DISCONNECT_DEVICE,
            (&addr as *const u64).cast(),
            size_of::<u64>() as u32,
            null_mut(),
            0,
            &mut returned,
            null_mut(),
        );
        let err = io::Error::last_os_error();
        CloseHandle(radio);
        BluetoothFindRadioClose(find);
        if ok == 0 { Err(err) } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_with_and_without_colons() {
        assert_eq!(parse_address("a4:ae:12:34:56:78"), Some(0xA4AE_1234_5678));
        assert_eq!(parse_address("A4AE12345678"), Some(0xA4AE_1234_5678));
        assert_eq!(parse_address("12345"), None);
    }
}
