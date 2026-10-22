//! Foreground application detection for automatic profile switching.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GA_ROOTOWNER, GetAncestor, GetForegroundWindow, GetWindowThreadProcessId,
};

use super::process;
use crate::ForegroundWatcher;

pub struct Foreground;

/// The window in front.
fn foreground_window() -> Option<HWND> {
    // SAFETY: plain query.
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_null()).then_some(hwnd)
}

/// Process id of a window.
fn window_pid(hwnd: HWND) -> Option<u32> {
    let mut pid = 0u32;
    // SAFETY: a window handle Windows returned and a valid out-pointer.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    (pid != 0).then_some(pid)
}

/// Process id of the window in front.
fn foreground_window_pid() -> Option<u32> {
    window_pid(foreground_window()?)
}

/// Process id of the window that owns the one in front (a dialog's main window).
fn foreground_owner_pid() -> Option<u32> {
    let hwnd = foreground_window()?;
    // SAFETY: a window handle Windows returned.
    let root = unsafe { GetAncestor(hwnd, GA_ROOTOWNER) };
    window_pid(if root.is_null() { hwnd } else { root })
}

impl ForegroundWatcher for Foreground {
    fn foreground_process(&mut self) -> Option<String> {
        let path = process::image_path(foreground_window_pid()?)?;
        path.rsplit(['\\', '/']).next().map(str::to_owned)
    }

    /// A dialog in front counts as its owner's: the editor's confirmations and file
    /// pickers belong to the editor even when another process draws them.
    fn foreground_pid(&mut self) -> Option<u32> {
        foreground_owner_pid()
    }
}
