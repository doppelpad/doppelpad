//! Running processes, for telling the user which program holds a controller.

use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};

use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};

use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongW, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, WS_EX_TOOLWINDOW,
};
use windows_sys::core::BOOL;

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
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
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

/// The executable file name (e.g. `game.exe`) a process runs.
pub(crate) fn exe_name(pid: u32) -> Option<String> {
    image_path(pid)?
        .rsplit(['\\', '/'])
        .next()
        .map(str::to_owned)
}

/// Every top-level window, in z order.
pub(crate) fn top_level_windows() -> Vec<HWND> {
    let mut windows: Vec<HWND> = Vec::new();
    // SAFETY: the callback only pushes into `windows`, which outlives the call.
    unsafe { EnumWindows(Some(collect_window), (&raw mut windows) as LPARAM) };
    windows
}

/// The process a window belongs to.
pub(crate) fn window_pid(hwnd: HWND) -> Option<u32> {
    let mut pid = 0u32;
    // SAFETY: a window handle Windows gave out and a valid out-pointer; an invalid handle
    // leaves `pid` at 0.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    (pid != 0).then_some(pid)
}

/// Executable file name and window title of each program with a visible main window (one per
/// program, the first window found), without this process.
pub(crate) fn windowed() -> Vec<(String, String)> {
    let own = std::process::id();
    let mut out: Vec<(String, String)> = Vec::new();
    for hwnd in top_level_windows() {
        if !is_main_window(hwnd) {
            continue;
        }
        let Some(pid) = window_pid(hwnd).filter(|&p| p != own) else {
            continue;
        };
        let Some(exe) = exe_name(pid) else {
            continue;
        };
        if out.iter().any(|(e, _)| e.eq_ignore_ascii_case(&exe)) {
            continue;
        }
        out.push((exe, window_title(hwnd)));
    }
    out
}

/// A visible, titled top-level window that no other window owns and that is not a tool window.
fn is_main_window(hwnd: HWND) -> bool {
    // SAFETY: plain queries on a window handle from EnumWindows.
    unsafe {
        IsWindowVisible(hwnd) != 0
            && GetWindow(hwnd, GW_OWNER).is_null()
            && (GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW) == 0
            && GetWindowTextLengthW(hwnd) > 0
    }
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: valid window handle; the length is the buffer size in characters.
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// `EnumWindows` callback: collect every top-level window.
unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `&mut Vec<HWND>` passed by `top_level_windows`.
    let windows = unsafe { &mut *(lparam as *mut Vec<HWND>) };
    windows.push(hwnd);
    1
}

/// The running program that most likely holds a controller (see [`crate::holders`]).
pub(crate) fn likely_holder() -> Option<&'static str> {
    let names = list();
    crate::holders::likely_holder(|exe| names.iter().any(|(_, n)| n.eq_ignore_ascii_case(exe)))
}
