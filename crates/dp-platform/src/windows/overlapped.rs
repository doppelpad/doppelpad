//! Overlapped (asynchronous) device I/O, run to completion.

use std::io;
use std::ptr::null;
use std::time::Duration;

use windows_sys::Win32::Foundation::{ERROR_IO_PENDING, GetLastError, HANDLE, WAIT_TIMEOUT};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

use super::OwnedHandle;

/// Whether an event stays signalled after a wait (manual) or resets itself (auto).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EventReset {
    Auto,
    Manual,
}

/// An unnamed event, not signalled.
pub(crate) fn event(reset: EventReset) -> io::Result<OwnedHandle> {
    let manual = i32::from(reset == EventReset::Manual);
    // SAFETY: plain unnamed event.
    OwnedHandle::from_null_ret(unsafe { CreateEventW(null(), manual, 0, null()) })
}

/// Run one overlapped operation on `handle` that signals `event` and wait until it is
/// done; after `timeout` (none: no limit) it is cancelled. `start` makes the call with the
/// `OVERLAPPED` it gets and returns the call's result. The bytes transferred.
///
/// # Safety
/// Whatever `start` hands the call (buffers, request structs) must stay valid until this
/// returns; it does not return before the operation has finished.
pub(crate) unsafe fn run_overlapped(
    handle: HANDLE,
    event: &OwnedHandle,
    timeout: Option<Duration>,
    start: impl FnOnce(*mut OVERLAPPED) -> i32,
) -> io::Result<u32> {
    let mut ov = OVERLAPPED {
        hEvent: event.raw(),
        ..Default::default()
    };
    // SAFETY: reads the calling thread's last error.
    if start(&mut ov) == 0 && unsafe { GetLastError() } != ERROR_IO_PENDING {
        return Err(io::Error::last_os_error());
    }
    if let Some(t) = timeout {
        let ms = super::win_ms(t);
        // SAFETY: the operation signals this event.
        if unsafe { WaitForSingleObject(event.raw(), ms) } == WAIT_TIMEOUT {
            // SAFETY: cancels the operation `ov` describes; it is waited for below.
            unsafe { CancelIoEx(handle, &ov) };
        }
    }
    let mut n = 0u32;
    // SAFETY: `ov` describes an operation started on `handle`; this waits for its end.
    if unsafe { GetOverlappedResult(handle, &ov, &mut n, 1) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(n)
}
