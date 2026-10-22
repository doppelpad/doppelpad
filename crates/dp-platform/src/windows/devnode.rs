//! SetupAPI / Configuration Manager helpers: enumerate device interfaces and
//! tell physical devices apart from ViGEmBus-created virtual ones.

use std::mem::offset_of;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_DRP_SERVICE, CM_Get_DevNode_PropertyW, CM_Get_DevNode_Registry_PropertyW, CM_Get_Device_IDW,
    CM_Get_Parent, CM_LOCATE_DEVNODE_NORMAL, CM_Locate_DevNodeW, CR_SUCCESS, DIGCF_DEVICEINTERFACE,
    DIGCF_PRESENT, HDEVINFO, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces,
    SetupDiGetClassDevsW, SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceInterfaceDetailW,
};
use windows_sys::Win32::Devices::Properties::{
    DEVPKEY_Device_BusReportedDeviceDesc, DEVPROP_TYPE_STRING,
};
use windows_sys::core::GUID;

use super::{from_wide, wide};

/// Device interface class of XUSB (XInput) devices.
const GUID_DEVINTERFACE_XUSB: GUID = GUID::from_u128(0xEC87F1E3_C13B_4100_B5F7_8B84D54260CB);

#[derive(Clone, Debug)]
pub struct InterfaceEntry {
    /// Device interface path, openable with `CreateFileW`.
    pub path: String,
    /// Device instance ID, e.g. `HID\VID_054C&PID_09CC&MI_03\7&...` (HidHide blacklist format).
    pub instance_id: String,
    pub devinst: u32,
}

/// A device information set, destroyed on drop.
struct DevInfoSet(HDEVINFO);

impl Drop for DevInfoSet {
    fn drop(&mut self) {
        // SAFETY: the set SetupDiGetClassDevsW returned, destroyed once.
        unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

/// All present devices exposing the given interface class.
pub fn enumerate_interfaces(class: &GUID) -> Vec<InterfaceEntry> {
    let mut out = Vec::new();
    // SAFETY: plain query.
    let set = unsafe {
        SetupDiGetClassDevsW(
            class,
            null(),
            null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };
    if set == -1 || set == 0 {
        return out;
    }
    let set = DevInfoSet(set);
    for index in 0.. {
        let mut ifdata = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        // SAFETY: valid set and class; the out-struct has its size set.
        if unsafe { SetupDiEnumDeviceInterfaces(set.0, null(), class, index, &mut ifdata) } == 0 {
            break;
        }
        if let Some(entry) = interface_entry(&set, &ifdata) {
            out.push(entry);
        }
    }
    out
}

/// Path, instance ID and devnode of one interface in `set`.
fn interface_entry(set: &DevInfoSet, ifdata: &SP_DEVICE_INTERFACE_DATA) -> Option<InterfaceEntry> {
    let mut required = 0u32;
    // SAFETY: asks for the size only.
    unsafe {
        SetupDiGetDeviceInterfaceDetailW(set.0, ifdata, null_mut(), 0, &mut required, null_mut())
    };
    if required == 0 {
        return None;
    }
    // u32 backing keeps the detail struct correctly aligned.
    let mut buf = vec![0u32; (required as usize).div_ceil(4) + 1];
    let detail = buf.as_mut_ptr().cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
    // SAFETY: `buf` is aligned for the struct and larger than its header.
    unsafe { (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32 };
    let mut devinfo = SP_DEVINFO_DATA {
        cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
        ..Default::default()
    };
    // SAFETY: `detail` points at `required` bytes; `devinfo` has its size set.
    let ok = unsafe {
        SetupDiGetDeviceInterfaceDetailW(set.0, ifdata, detail, required, null_mut(), &mut devinfo)
    };
    if ok == 0 {
        return None;
    }
    let path_at = offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath);
    let max = (required as usize - path_at) / size_of::<u16>();
    // SAFETY: the call wrote a NUL-terminated path of at most `max` characters there.
    let path = from_wide(unsafe {
        std::slice::from_raw_parts(std::ptr::addr_of!((*detail).DevicePath).cast::<u16>(), max)
    });
    let mut id = [0u16; 512];
    // SAFETY: valid set and device data; the buffer length is given in characters.
    let has_id = unsafe {
        SetupDiGetDeviceInstanceIdW(
            set.0,
            &devinfo,
            id.as_mut_ptr(),
            id.len() as u32,
            null_mut(),
        )
    } != 0;
    Some(InterfaceEntry {
        path,
        instance_id: if has_id {
            from_wide(&id)
        } else {
            String::new()
        },
        devinst: devinfo.DevInst,
    })
}

/// The parent devnode.
fn parent(devinst: u32) -> Option<u32> {
    let mut parent = 0u32;
    // SAFETY: valid out-pointer.
    (unsafe { CM_Get_Parent(&mut parent, devinst, 0) } == CR_SUCCESS).then_some(parent)
}

/// `devinst` and then its ancestors, at most `limit` nodes.
fn ancestors(devinst: u32, limit: usize) -> impl Iterator<Item = u32> {
    std::iter::successors(Some(devinst), |&n| parent(n)).take(limit)
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
    ancestors(devinst, 16)
        .any(|n| service_of(n).is_some_and(|s| s.eq_ignore_ascii_case("ViGEmBus")))
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
    for node in ancestors(devinst, 9).skip(1) {
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
    let on_usb =
        |n: u32| instance_id_of(n).is_some_and(|id| id.to_ascii_uppercase().starts_with(r"USB\"));
    let mut nearest = None;
    // Up the USB nodes only.
    for node in std::iter::successors(Some(devinst), |&n| parent(n).filter(|&p| on_usb(p))).take(4)
    {
        let name = bus_reported_name(node);
        if instance_id_of(node).is_some_and(|id| is_usb_device_id(&id)) {
            return name.or(nearest);
        }
        nearest = nearest.or(name);
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
