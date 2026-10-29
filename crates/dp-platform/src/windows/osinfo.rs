//! Windows version for logs and bug reports, read from the registry (no process to spawn,
//! and unaffected by the compatibility shims that make `GetVersionEx` lie).

use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;

use super::registry;

const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

fn dword(value: &str) -> Option<u32> {
    registry::get_dword(HKEY_LOCAL_MACHINE, KEY, value)
}

fn string(value: &str) -> Option<String> {
    registry::get_sz(HKEY_LOCAL_MACHINE, KEY, value)
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
