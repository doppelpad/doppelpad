//! Foreground application detection for automatic profile switching.

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GA_ROOTOWNER, GetAncestor, GetForegroundWindow, GetWindowThreadProcessId,
};

use crate::ForegroundWatcher;

pub struct Foreground;

/// Process id of the window in front, or with `owner` of the window that owns it (a
/// dialog's main window).
fn foreground_window_pid(owner: bool) -> Option<u32> {
    // SAFETY: plain queries on the window handles Windows returns.
    unsafe {
        let mut hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        if owner {
            let root = GetAncestor(hwnd, GA_ROOTOWNER);
            if !root.is_null() {
                hwnd = root;
            }
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        (pid != 0).then_some(pid)
    }
}

impl ForegroundWatcher for Foreground {
    fn foreground_process(&mut self) -> Option<String> {
        let pid = foreground_window_pid(false)?;
        // SAFETY: plain queries; the process handle is closed before returning.
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None;
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok =
                QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
            CloseHandle(process);
            if ok == 0 {
                return None;
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            path.rsplit(['\\', '/']).next().map(str::to_owned)
        }
    }

    /// A dialog in front counts as its owner's: the editor's confirmations and file
    /// pickers belong to the editor even when another process draws them.
    fn foreground_pid(&mut self) -> Option<u32> {
        foreground_window_pid(true)
    }
}
