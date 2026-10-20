//! Windows backends: XInput + raw HID input, ViGEmBus virtual pads, HidHide device
//! hiding, `SendInput` output, and low-level hook / raw-input capture.

pub mod autostart;
mod bluetooth;
mod capture;
mod devnode;
mod foreground;
mod hid;
mod hidhide;
mod hidio;
mod magnifier;
mod osinfo;
pub mod overlay;
mod process;
pub mod rewasd;
mod sendinput;
mod switch2;
mod system;
mod vigem;
mod xinput;

pub use devnode::physical_xusb_instances;
pub use hidhide::HidHide;
pub use magnifier::{Magnifier, MagnifierView};
pub use osinfo::os_version;

use crate::Platform;
use crate::joycon::JoyConCombiner;

pub fn platform() -> Platform {
    // Before any backend enumerates: un-hide pads a killed earlier run left hidden.
    hidhide::recover_stale();
    Platform {
        gamepads: vec![
            Box::new(xinput::XInputBackend::new()),
            Box::new(JoyConCombiner::new(hid::HidBackend::new())),
        ],
        virtual_pads: Box::new(vigem::VigemFactory::new()),
        kbm_out: Box::new(sendinput::SendInputOutput::default()),
        kbm_capture: Box::new(capture::HookCapture::default()),
        hider: Box::new(hidhide::HidHide::new()),
        foreground: Box::new(foreground::Foreground),
        system: Box::new(system::WinSystem),
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

/// Owned Win32 handle closed on drop.
pub(crate) struct OwnedHandle(pub windows_sys::Win32::Foundation::HANDLE);

// SAFETY: Win32 file/device handles may be used from any thread.
unsafe impl Send for OwnedHandle {}
unsafe impl Sync for OwnedHandle {}

impl OwnedHandle {
    pub fn valid(&self) -> bool {
        !self.0.is_null() && self.0 != windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if self.valid() {
            // SAFETY: we own the handle.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
        }
    }
}
