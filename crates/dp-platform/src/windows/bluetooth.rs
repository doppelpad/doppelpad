//! Disconnecting a Bluetooth device from the local radio, which turns a controller off
//! without unpairing it.

use std::io;
use std::ptr::null_mut;

use windows_sys::Win32::Devices::Bluetooth::{
    BLUETOOTH_FIND_RADIO_PARAMS, BluetoothFindFirstRadio, BluetoothFindRadioClose,
};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::IO::DeviceIoControl;

use super::OwnedHandle;

/// `CTL_CODE(FILE_DEVICE_BLUETOOTH, 0x03, METHOD_BUFFERED, FILE_ANY_ACCESS)`.
const IOCTL_BTH_DISCONNECT_DEVICE: u32 = 0x0041_000C;

/// Ask the first radio to drop the connection to `address`.
pub fn disconnect(address: u64) -> io::Result<()> {
    let params = BLUETOOTH_FIND_RADIO_PARAMS {
        dwSize: size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32,
    };
    let mut radio: HANDLE = null_mut();
    // SAFETY: valid parameter block and out-pointer.
    let find = unsafe { BluetoothFindFirstRadio(&params, &mut radio) };
    if find.is_null() {
        return Err(io::Error::last_os_error());
    }
    let radio = OwnedHandle::new(radio);
    // SAFETY: the search handle is not used again.
    unsafe { BluetoothFindRadioClose(find) };
    let mut returned = 0u32;
    // SAFETY: valid radio handle, input buffer and out-pointer.
    let ok = unsafe {
        DeviceIoControl(
            radio.raw(),
            IOCTL_BTH_DISCONNECT_DEVICE,
            (&raw const address).cast(),
            size_of::<u64>() as u32,
            null_mut(),
            0,
            &mut returned,
            null_mut(),
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
