//! SetupAPI / Configuration Manager helpers: enumerate device interfaces and
//! tell physical devices apart from ViGEmBus-created virtual ones.

use std::mem::size_of;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_PropertyW, CM_Get_DevNode_Registry_PropertyW, CM_Get_Device_IDW, CM_Get_Parent,
    CM_LOCATE_DEVNODE_NORMAL, CM_Locate_DevNodeW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
    SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
    SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceInterfaceDetailW,
};
use windows_sys::Win32::Devices::Properties::{
    DEVPKEY_Device_BusReportedDeviceDesc, DEVPROP_TYPE_STRING,
};
use windows_sys::core::GUID;

use super::{from_wide, wide};

/// Device interface class of XUSB (XInput) devices.
pub const GUID_DEVINTERFACE_XUSB: GUID = GUID::from_u128(0xEC87F1E3_C13B_4100_B5F7_8B84D54260CB);

const CM_DRP_SERVICE: u32 = 0x05;
const CR_SUCCESS: u32 = 0;

#[derive(Clone, Debug)]
pub struct InterfaceEntry {
    /// Device interface path, openable with `CreateFileW`.
    pub path: String,
    /// Device instance ID, e.g. `HID\VID_054C&PID_09CC&MI_03\7&...` (HidHide blacklist format).
    pub instance_id: String,
    pub devinst: u32,
}

/// All present devices exposing the given interface class.
pub fn enumerate_interfaces(class: &GUID) -> Vec<InterfaceEntry> {
    let mut out = Vec::new();
    // SAFETY: standard SetupAPI enumeration; all buffers are sized as the API requests.
    unsafe {
        let set = SetupDiGetClassDevsW(
            class,
            null(),
            null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        );
        if set as isize == -1 || set as isize == 0 {
            return out;
        }
        let mut index = 0;
        loop {
            let mut ifdata: SP_DEVICE_INTERFACE_DATA = std::mem::zeroed();
            ifdata.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
            if SetupDiEnumDeviceInterfaces(set, null(), class, index, &mut ifdata) == 0 {
                break;
            }
            index += 1;

            let mut required = 0u32;
            SetupDiGetDeviceInterfaceDetailW(
                set,
                &ifdata,
                null_mut(),
                0,
                &mut required,
                null_mut(),
            );
            if required == 0 {
                continue;
            }
            // u32 backing keeps the detail struct correctly aligned.
            let mut buf = vec![0u32; (required as usize).div_ceil(4) + 1];
            let detail = buf.as_mut_ptr().cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
            (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            let mut devinfo: SP_DEVINFO_DATA = std::mem::zeroed();
            devinfo.cbSize = size_of::<SP_DEVINFO_DATA>() as u32;
            if SetupDiGetDeviceInterfaceDetailW(
                set,
                &ifdata,
                detail,
                required,
                null_mut(),
                &mut devinfo,
            ) == 0
            {
                continue;
            }
            let path_ptr = std::ptr::addr_of!((*detail).DevicePath).cast::<u16>();
            let max = (required as usize - 4) / 2;
            let path = from_wide(std::slice::from_raw_parts(path_ptr, max));

            let mut id = [0u16; 512];
            let instance_id = if SetupDiGetDeviceInstanceIdW(
                set,
                &devinfo,
                id.as_mut_ptr(),
                id.len() as u32,
                null_mut(),
            ) != 0
            {
                from_wide(&id)
            } else {
                String::new()
            };
            out.push(InterfaceEntry {
                path,
                instance_id,
                devinst: devinfo.DevInst,
            });
        }
        SetupDiDestroyDeviceInfoList(set);
    }
    out
}

fn service_of(devinst: u32) -> Option<String> {
    let mut buf = [0u16; 256];
    let mut len = (buf.len() * 2) as u32;
    let mut ty = 0u32;
    // SAFETY: buffer and length describe valid memory.
    let rc = unsafe {
        CM_Get_DevNode_Registry_PropertyW(
            devinst,
            CM_DRP_SERVICE,
            &mut ty,
            buf.as_mut_ptr().cast(),
            &mut len,
            0,
        )
    };
    (rc == CR_SUCCESS).then(|| from_wide(&buf))
}

/// True if the device or any ancestor is driven by ViGEmBus, i.e. it is a virtual
/// controller (ours or another program's) rather than real hardware.
pub fn is_vigem_device(devinst: u32) -> bool {
    let mut node = devinst;
    for _ in 0..16 {
        if service_of(node).is_some_and(|s| s.eq_ignore_ascii_case("ViGEmBus")) {
            return true;
        }
        let mut parent = 0u32;
        // SAFETY: valid out-pointer.
        if unsafe { CM_Get_Parent(&mut parent, node, 0) } != CR_SUCCESS {
            return false;
        }
        node = parent;
    }
    false
}

/// Instance IDs of all physical (non-ViGEm) XInput devices; used to hide Xbox pads.
pub fn physical_xusb_instances() -> Vec<String> {
    enumerate_interfaces(&GUID_DEVINTERFACE_XUSB)
        .into_iter()
        .filter(|e| !is_vigem_device(e.devinst) && !e.instance_id.is_empty())
        .map(|e| e.instance_id)
        .collect()
}

fn instance_id_of(devinst: u32) -> Option<String> {
    let mut buf = [0u16; 512];
    // SAFETY: buffer and length describe valid memory.
    let rc = unsafe { CM_Get_Device_IDW(devinst, buf.as_mut_ptr(), buf.len() as u32, 0) };
    (rc == CR_SUCCESS).then(|| from_wide(&buf))
}

/// A whole USB device (`USB\VID_xxxx&PID_xxxx\...`), not one interface of it (`&MI_nn`).
fn is_usb_device_id(id: &str) -> bool {
    let id = id.to_ascii_uppercase();
    id.starts_with(r"USB\") && !id.contains("&MI_")
}

/// Instance ID of the USB device a HID interface belongs to (walking up past the
/// composite-device interface node).
pub fn usb_device_of(devinst: u32) -> Option<String> {
    let mut node = devinst;
    for _ in 0..8 {
        let mut parent = 0u32;
        // SAFETY: valid out-pointer.
        if unsafe { CM_Get_Parent(&mut parent, node, 0) } != CR_SUCCESS {
            return None;
        }
        node = parent;
        let id = instance_id_of(node)?;
        if is_usb_device_id(&id) {
            return Some(id);
        }
    }
    None
}

/// Devnode of a device instance ID, if that device is connected right now (Windows also
/// remembers devices that were unplugged).
pub fn present_devnode(instance_id: &str) -> Option<u32> {
    let id = wide(instance_id);
    let mut devinst = 0u32;
    // SAFETY: NUL-terminated ID and valid out-pointer.
    let rc = unsafe { CM_Locate_DevNodeW(&mut devinst, id.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) };
    (rc == CR_SUCCESS).then_some(devinst)
}

/// What the bus reported as the device's name (the USB product string).
fn bus_reported_name(devinst: u32) -> Option<String> {
    let mut buf = [0u16; 256];
    let mut len = (buf.len() * 2) as u32;
    let mut ty = 0u32;
    // SAFETY: buffer and length describe valid memory.
    let rc = unsafe {
        CM_Get_DevNode_PropertyW(
            devinst,
            &DEVPKEY_Device_BusReportedDeviceDesc,
            &mut ty,
            buf.as_mut_ptr().cast(),
            &mut len,
            0,
        )
    };
    let name = from_wide(&buf);
    (rc == CR_SUCCESS && ty == DEVPROP_TYPE_STRING && !name.trim().is_empty()).then_some(name)
}

/// How a device names itself, e.g. "DualSense Wireless Controller": what the whole USB
/// device above it reported, else the nearest name on the way up. A HID node's own name is
/// generic ("HID-compliant game controller") and an interface may report just its label
/// ("If_Hid"). `None` for devices off USB (Bluetooth), whose ancestors are the radio.
pub fn product_name(devinst: u32) -> Option<String> {
    let mut node = devinst;
    let mut nearest = None;
    for _ in 0..4 {
        let name = bus_reported_name(node);
        if instance_id_of(node).is_some_and(|id| is_usb_device_id(&id)) {
            return name.or(nearest);
        }
        nearest = nearest.or(name);
        let mut parent = 0u32;
        // SAFETY: valid out-pointer.
        if unsafe { CM_Get_Parent(&mut parent, node, 0) } != CR_SUCCESS
            || !instance_id_of(parent)
                .is_some_and(|id| id.to_ascii_uppercase().starts_with(r"USB\"))
        {
            break;
        }
        node = parent;
    }
    nearest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_device_ids() {
        assert!(is_usb_device_id(r"USB\VID_057E&PID_2069\5&2A3B"));
        assert!(!is_usb_device_id(r"USB\VID_057E&PID_2069&MI_00\6&1F"));
        assert!(!is_usb_device_id(r"HID\VID_057E&PID_2069&MI_00\7&AB"));
    }
}
