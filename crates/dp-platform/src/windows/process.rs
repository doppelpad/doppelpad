//! Running processes, for telling the user which program holds a controller.

use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};

use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};

use super::{OwnedHandle, from_wide};

/// Process ID and executable file name (e.g. `steam.exe`) of every running process.
pub(crate) fn list() -> Vec<(u32, String)> {
    let mut out = Vec::new();
    // SAFETY: plain call; the handle is owned and closed on return.
    let Ok(snap) =
        OwnedHandle::from_invalid_ret(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) })
    else {
        return out;
    };
    // SAFETY: plain data; dwSize set as the API requires.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: valid snapshot handle and entry.
    let mut more = unsafe { Process32FirstW(snap.raw(), &mut entry) } != 0;
    while more {
        out.push((entry.th32ProcessID, from_wide(&entry.szExeFile)));
        // SAFETY: as above.
        more = unsafe { Process32NextW(snap.raw(), &mut entry) } != 0;
    }
    out
}

/// The full path of the executable a process runs.
pub(crate) fn image_path(pid: u32) -> Option<String> {
    // SAFETY: plain query; the handle is owned and closed on return.
    let process = OwnedHandle::from_null_ret(unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid)
    })
    .ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: valid handle; `len` is the buffer size in characters.
    let ok = unsafe {
        QueryFullProcessImageNameW(
            process.raw(),
            PROCESS_NAME_WIN32,
            buf.as_mut_ptr(),
            &mut len,
        )
    };
    (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
}

/// Whether a process with this executable file name runs (any case).
pub(crate) fn running(exe: &str) -> bool {
    list()
        .iter()
        .any(|(_, name)| name.eq_ignore_ascii_case(exe))
}
