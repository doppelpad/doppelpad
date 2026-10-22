//! The OS timer resolution the engine loop needs.

/// Raise the OS timer resolution for the lifetime of the guard so 1 ms sleeps are accurate
/// (Windows; other systems already sleep that precisely).
pub struct TimerResolution;

impl TimerResolution {
    pub fn high() -> Self {
        #[cfg(windows)]
        // SAFETY: plain winmm call, undone in Drop.
        unsafe {
            windows_sys::Win32::Media::timeBeginPeriod(1);
        }
        TimerResolution
    }
}

impl Drop for TimerResolution {
    fn drop(&mut self) {
        #[cfg(windows)]
        // SAFETY: matches timeBeginPeriod in `high`.
        unsafe {
            windows_sys::Win32::Media::timeEndPeriod(1);
        }
    }
}
