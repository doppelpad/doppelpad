//! Windows backends: XInput, raw HID, USB (GameCube adapter, Switch 2) and Bluetooth LE
//! (Switch 2) input, ViGEmBus virtual pads, HidHide device hiding, `SendInput` output,
//! low-level hook / raw-input capture, and the overlay, magnifier, autostart and
//! foreground-window helpers.

pub mod autostart;
mod ble;
mod bluetooth;
mod capture;
mod devnode;
mod foreground;
mod gcadapter;
mod hid;
mod hidhide;
mod hidio;
mod hidparse;
mod magnifier;
mod osinfo;
mod overlapped;
pub mod overlay;
mod process;
mod registry;
pub mod rewasd;
mod sendinput;
pub mod stores;
mod switch2;
mod system;
mod vigem;
mod xinput;

pub use devnode::physical_xusb_instances;
pub use magnifier::{Magnifier, MagnifierView};
pub use osinfo::os_version;

/// The executable file name of the program in front (see `crate::foreground_program`).
pub fn foreground_program() -> Option<String> {
    use crate::ForegroundWatcher;
    foreground::Foreground.foreground_process()
}

/// Whether a process with this executable file name runs (any case).
pub fn program_running(exe: &str) -> bool {
    process::list()
        .iter()
        .any(|(_, name)| name.eq_ignore_ascii_case(exe))
}

/// The programs that show a window now (see `crate::windowed_programs`).
pub fn windowed_programs() -> Vec<crate::WindowedProgram> {
    process::windowed()
        .into_iter()
        .map(|(exe, title)| crate::WindowedProgram { exe, title })
        .collect()
}

use std::io;
use std::ptr::null;

use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{RegisterClassW, WNDCLASSW};

use crate::Platform;
use crate::joycon::JoyConCombiner;

pub fn platform() -> Platform {
    let switch2_search = crate::Switch2Search::default();
    // Before any backend enumerates: un-hide pads a killed earlier run left hidden.
    hidhide::recover_stale();
    Platform {
        probe_drivers: || crate::DriverReport {
            virtual_pads: vigem::available(),
            device_hiding: hidhide::available(),
            input_access: Ok(()),
            bluetooth: ble::check(),
            hiding_problems: hidhide::problems(),
            system_problems: Vec::new(),
        },
        gamepads: vec![
            Box::new(xinput::XInputBackend::new()),
            Box::new(gcadapter::GcAdapterBackend::new()),
            Box::new(JoyConCombiner::new(hid::backend(switch2_search.clone()))),
        ],
        virtual_pads: Box::new(vigem::VigemFactory::new()),
        kbm_out: Box::new(sendinput::SendInputOutput::default()),
        kbm_capture: Box::new(capture::HookCapture::default()),
        hider: Box::new(hidhide::HidHide::new()),
        foreground: Box::new(foreground::Foreground),
        system: Box::new(system::WinSystem),
        switch2_search,
    }
}

/// UTF-16, NUL-terminated.
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Read a NUL-terminated UTF-16 buffer.
pub(crate) fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Read a NUL-terminated UTF-16 string (null reads as empty).
///
/// # Safety
/// `p` is null or points to a NUL-terminated string.
pub(crate) unsafe fn from_wide_ptr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0;
    // SAFETY: the caller guarantees a terminating NUL.
    unsafe {
        while *p.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
    }
}

/// Register the window class `name` of this program with the window procedure `proc`;
/// returns the class name to create windows with.
pub(crate) fn register_class(
    name: &str,
    proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
) -> Vec<u16> {
    let class = wide(name);
    let wc = WNDCLASSW {
        lpfnWndProc: Some(proc),
        // SAFETY: returns this program's module handle.
        hInstance: unsafe { GetModuleHandleW(null()) },
        lpszClassName: class.as_ptr(),
        ..Default::default()
    };
    // SAFETY: `wc` points at the NUL-terminated name, which outlives the call. A class that
    // is registered already stays as it is.
    unsafe { RegisterClassW(&wc) };
    class
}

/// Owned Win32 handle closed on drop.
pub(crate) struct OwnedHandle(HANDLE);

// SAFETY: Win32 file/device handles may be used from any thread.
unsafe impl Send for OwnedHandle {}
// SAFETY: the handle is only passed to Win32 calls, which synchronise themselves.
unsafe impl Sync for OwnedHandle {}

impl OwnedHandle {
    /// Take over an open handle.
    pub(crate) fn new(h: HANDLE) -> Self {
        OwnedHandle(h)
    }

    /// The handle a call returned that reports failure with null (`OpenProcess`,
    /// `CreateEventW`); on failure, the error it left.
    pub(crate) fn from_null_ret(h: HANDLE) -> io::Result<Self> {
        if h.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(OwnedHandle(h))
        }
    }

    /// The handle a call returned that reports failure with `INVALID_HANDLE_VALUE`
    /// (`CreateFileW`, `CreateToolhelp32Snapshot`); on failure, the error it left.
    pub(crate) fn from_invalid_ret(h: HANDLE) -> io::Result<Self> {
        if h == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(OwnedHandle(h))
        }
    }

    pub(crate) fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            // SAFETY: we own the handle.
            unsafe { CloseHandle(self.0) };
        }
    }
}
