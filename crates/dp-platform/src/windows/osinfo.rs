//! Windows version for logs and bug reports, read from the registry (no process to spawn,
//! and unaffected by the compatibility shims that make `GetVersionEx` lie).

use std::ptr::null_mut;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
};

use super::wide;

const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

fn dword(value: &str) -> Option<u32> {
    let (key, value) = (wide(KEY), wide(value));
    let mut data = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: NUL-terminated names; `data` holds `size` bytes.
    let r = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&raw mut data).cast(),
            &mut size,
        )
    };
    (r == ERROR_SUCCESS).then_some(data)
}

fn string(value: &str) -> Option<String> {
    let (key, value) = (wide(KEY), wide(value));
    let mut buf = [0u16; 128];
    let mut size = size_of_val(&buf) as u32;
    // SAFETY: NUL-terminated names; `buf` holds `size` bytes.
    let r = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if r != ERROR_SUCCESS {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

/// E.g. `Windows 10.0.26100.4652 (24H2, Professional)`. Build 22000 and later is Windows 11.
pub fn os_version() -> String {
    let mut s = format!(
        "Windows {}.{}.{}",
        dword("CurrentMajorVersionNumber").unwrap_or(0),
        dword("CurrentMinorVersionNumber").unwrap_or(0),
        string("CurrentBuildNumber").unwrap_or_else(|| "?".into()),
    );
    if let Some(ubr) = dword("UBR") {
        s.push_str(&format!(".{ubr}"));
    }
    let extra: Vec<String> = [string("DisplayVersion"), string("EditionID")]
        .into_iter()
        .flatten()
        .collect();
    if !extra.is_empty() {
        s.push_str(&format!(" ({})", extra.join(", ")));
    }
    s
}
