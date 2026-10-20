//! Overlay windows: top-most windows over games that never take the focus. The window
//! itself is created by the app; these calls only adjust its styles and visibility.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, HWND_TOPMOST, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

/// Keep `hwnd` out of Alt+Tab and from ever being activated.
pub fn prepare(hwnd: isize) {
    let hwnd = hwnd as HWND;
    // SAFETY: plain style calls on a window handle; an invalid handle makes them fail.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ex | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize,
        );
    }
}

/// Show `hwnd` above every other window without activating it.
pub fn show(hwnd: isize) {
    let hwnd = hwnd as HWND;
    // SAFETY: as in `prepare`.
    unsafe {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

pub fn hide(hwnd: isize) {
    // SAFETY: as in `prepare`.
    unsafe {
        ShowWindow(hwnd as HWND, SW_HIDE);
    }
}
