//! Running processes, for telling the user which program holds a controller.

use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};

use super::{OwnedHandle, from_wide};

/// Process ID and executable file name (e.g. `steam.exe`) of every running process.
pub(crate) fn list() -> Vec<(u32, String)> {
    let mut out = Vec::new();
    // SAFETY: plain call; the handle is owned and closed on return.
    let snap = OwnedHandle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) });
    if !snap.valid() {
        return out;
    }
    // SAFETY: plain data; dwSize set as the API requires.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: valid snapshot handle and entry.
    let mut more = unsafe { Process32FirstW(snap.0, &mut entry) } != 0;
    while more {
        out.push((entry.th32ProcessID, from_wide(&entry.szExeFile)));
        // SAFETY: as above.
        more = unsafe { Process32NextW(snap.0, &mut entry) } != 0;
    }
    out
}

/// Whether a process with this executable file name runs (any case).
pub(crate) fn running(exe: &str) -> bool {
    list()
        .iter()
        .any(|(_, name)| name.eq_ignore_ascii_case(exe))
}
