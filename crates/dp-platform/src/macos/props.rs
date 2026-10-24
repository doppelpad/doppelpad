//! What IOKit says about a HID device through its registry properties, read without opening
//! it.

use std::ffi::c_char;

use objc2_core_foundation::{CFBoolean, CFData, CFNumber, CFRetained, CFString, CFType};
use objc2_io_kit::{
    IOHIDDevice, IORegistryEntrySearchCFProperty, kIORegistryIterateParents,
    kIORegistryIterateRecursively, kIOServicePlane,
};

use crate::hiddesc;

#[derive(Clone, Debug, Default)]
pub struct Props {
    pub vid: u16,
    pub pid: u16,
    /// The primary top-level collection.
    pub usage_page: u16,
    pub usage: u16,
    /// `USB`, `Bluetooth`, `Bluetooth Low Energy`, ...
    pub transport: String,
    pub product: Option<String>,
    pub serial: Option<String>,
    /// Where the device is attached (the USB port; Bluetooth devices have one too).
    pub location: Option<u32>,
    /// Report lengths, the report ID included for devices that number their reports.
    pub max_input: usize,
    pub max_output: usize,
    pub max_feature: usize,
    /// The report descriptor.
    pub descriptor: Vec<u8>,
    /// The device numbers its reports (see [`hiddesc::numbered`]).
    pub numbered: bool,
    /// macOS made the device up for a controller it supports itself (`GCSyntheticDevice`), so
    /// that older games see an Xbox 360 pad.
    pub synthetic: bool,
    /// The USB interface the device belongs to (a charging grip has one per Joy-Con).
    pub interface: Option<u8>,
}

impl Props {
    pub fn of(device: &IOHIDDevice) -> Props {
        let number = |key| property(device, key).and_then(|v| as_number(&v));
        let u16_of = |key| number(key).and_then(|n| u16::try_from(n).ok()).unwrap_or(0);
        let len_of = |key| {
            number(key)
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0)
        };
        let string = |key| {
            property(device, key)
                .and_then(|v| v.downcast_ref::<CFString>().map(ToString::to_string))
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
        };
        let service = device.service();
        let descriptor = property(device, "ReportDescriptor")
            .and_then(|v| v.downcast_ref::<CFData>().map(CFData::to_vec))
            .unwrap_or_default();
        Props {
            vid: u16_of("VendorID"),
            pid: u16_of("ProductID"),
            usage_page: u16_of("PrimaryUsagePage"),
            usage: u16_of("PrimaryUsage"),
            transport: string("Transport").unwrap_or_default(),
            product: string("Product"),
            serial: string("SerialNumber"),
            location: number("LocationID").and_then(|n| u32::try_from(n).ok()),
            max_input: len_of("MaxInputReportSize"),
            max_output: len_of("MaxOutputReportSize"),
            max_feature: len_of("MaxFeatureReportSize"),
            numbered: hiddesc::numbered(&descriptor),
            descriptor,
            synthetic: property(device, "GCSyntheticDevice")
                .and_then(|v| v.downcast_ref::<CFBoolean>().map(CFBoolean::as_bool))
                .unwrap_or(false),
            interface: search_up(service, "bInterfaceNumber")
                .and_then(|v| as_number(&v))
                .and_then(|n| u8::try_from(n).ok()),
        }
    }

    /// Connected over Bluetooth (classic or LE).
    pub fn bluetooth(&self) -> bool {
        self.transport.starts_with("Bluetooth")
    }
}

fn property(device: &IOHIDDevice, key: &'static str) -> Option<CFRetained<CFType>> {
    device.property(&CFString::from_static_str(key))
}

fn as_number(v: &CFType) -> Option<i64> {
    v.downcast_ref::<CFNumber>()?.as_i64()
}

/// A property of `entry` or of the first parent in the service plane that has it.
fn search_up(entry: u32, key: &'static str) -> Option<CFRetained<CFType>> {
    let mut plane: [c_char; 128] = [0; 128];
    for (to, &from) in plane.iter_mut().zip(kIOServicePlane.to_bytes()) {
        *to = from as c_char;
    }
    // SAFETY: a live registry entry, a NUL-terminated plane name in a 128-byte buffer and a
    // valid key; the result is retained for us.
    unsafe {
        IORegistryEntrySearchCFProperty(
            entry,
            &mut plane,
            Some(&CFString::from_static_str(key)),
            None,
            kIORegistryIterateRecursively | kIORegistryIterateParents,
        )
    }
}
