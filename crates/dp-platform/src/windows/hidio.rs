//! Overlapped HID device handle.
//!
//! One read is always pending; [`HidIo::wait`] waits for it, for a timeout, or for the
//! [`Waker`] that another thread can signal. Writes and feature reports use their own
//! `OVERLAPPED`, so they never queue behind the pending read the way they would on a
//! synchronous handle.

use std::ffi::c_void;
use std::io;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::time::Duration;

use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HIDD_ATTRIBUTES, HIDP_CAPS, HIDP_STATUS_SUCCESS, HidD_FreePreparsedData, HidD_GetAttributes,
    HidD_GetPreparsedData, HidD_GetProductString, HidD_GetSerialNumberString, HidD_SetFeature,
    HidP_GetCaps,
};
use windows_sys::Win32::Foundation::{
    ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadFile,
    WriteFile,
};
use windows_sys::Win32::System::IO::{
    CancelIoEx, DeviceIoControl, GetOverlappedResult, OVERLAPPED,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, SetEvent, WaitForMultipleObjects, WaitForSingleObject,
};

use super::{OwnedHandle, from_wide, wide};
use crate::padio::ReportIo;

/// `CTL_CODE(FILE_DEVICE_KEYBOARD, 100, METHOD_OUT_DIRECT, FILE_ANY_ACCESS)`.
const IOCTL_HID_GET_FEATURE: u32 = 0x000B_0192;
/// Bluetooth writes can take a while; give up after this.
const WRITE_TIMEOUT_MS: u32 = 1000;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct HidCaps {
    pub usage_page: u16,
    pub usage: u16,
    pub input_len: usize,
    pub output_len: usize,
    pub feature_len: usize,
}

/// Wakes a thread blocked in [`HidIo::wait`].
pub struct Waker(OwnedHandle);

impl Waker {
    pub fn wake(&self) {
        // SAFETY: valid event handle.
        unsafe { SetEvent(self.0.raw()) };
    }
}

fn event(manual_reset: bool) -> io::Result<OwnedHandle> {
    // SAFETY: plain unnamed event.
    OwnedHandle::from_null_ret(unsafe { CreateEventW(null(), i32::from(manual_reset), 0, null()) })
}

/// The pending read: lives on the heap so its address stays fixed while the kernel
/// writes into it.
struct PendingRead {
    ov: OVERLAPPED,
    buf: Vec<u8>,
    event: OwnedHandle,
}

pub enum Wait {
    /// A report of this many bytes is in [`HidIo::report`].
    Report(usize),
    Woken,
    Timeout,
}

pub struct HidIo {
    handle: OwnedHandle,
    read: Box<PendingRead>,
    pending: bool,
    last_len: usize,
    waker: Arc<Waker>,
    caps: HidCaps,
}

// SAFETY: the OVERLAPPED only holds our own event handle; the whole HidIo moves to the
// device thread once and is used only there.
unsafe impl Send for HidIo {}

impl HidIo {
    pub fn open(path: &str) -> io::Result<HidIo> {
        let wpath = wide(path);
        let open = |access| {
            // SAFETY: standard CreateFileW call; ownership moves into OwnedHandle.
            OwnedHandle::from_invalid_ret(unsafe {
                CreateFileW(
                    wpath.as_ptr(),
                    access,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    null_mut(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    null_mut(),
                )
            })
        };
        let handle = match open(GENERIC_READ | GENERIC_WRITE) {
            Ok(h) => h,
            // Some interfaces refuse write access; reading is still useful.
            Err(_) => open(GENERIC_READ)?,
        };
        let caps = read_caps(handle.raw()).unwrap_or_default();
        Ok(HidIo {
            read: Box::new(PendingRead {
                // SAFETY: OVERLAPPED is plain data; all-zero is its initial state.
                ov: unsafe { std::mem::zeroed() },
                buf: vec![0u8; caps.input_len.max(64)],
                event: event(true)?,
            }),
            handle,
            pending: false,
            last_len: 0,
            waker: Arc::new(Waker(event(false)?)),
            caps,
        })
    }

    pub fn caps(&self) -> HidCaps {
        self.caps
    }

    /// The serial number string (over Bluetooth, Sony pads report their address here).
    pub fn serial(&self) -> Option<String> {
        self.hid_string(HidD_GetSerialNumberString)
    }

    /// The product name the device reports.
    pub fn product(&self) -> Option<String> {
        self.hid_string(HidD_GetProductString)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    }

    /// A string the HID driver tells (up to 127 characters), unless it is empty.
    fn hid_string(
        &self,
        query: unsafe extern "system" fn(HANDLE, *mut c_void, u32) -> bool,
    ) -> Option<String> {
        let mut buf = [0u16; 128];
        // SAFETY: valid handle; the buffer length is given in bytes.
        let ok = unsafe {
            query(
                self.handle.raw(),
                buf.as_mut_ptr().cast(),
                size_of_val(&buf) as u32,
            )
        };
        let s = from_wide(&buf);
        (ok && !s.is_empty()).then_some(s)
    }

    /// A decoder for the device's input reports (generic pads).
    pub fn decoder(&self) -> Option<super::hidparse::HidDecoder> {
        super::hidparse::HidDecoder::new(self.handle.raw())
    }

    /// Vendor ID, product ID.
    pub fn ids(&self) -> Option<(u16, u16)> {
        // SAFETY: zeroed plain struct; Size set as the API requires.
        let mut attrs: HIDD_ATTRIBUTES = unsafe { std::mem::zeroed() };
        attrs.Size = std::mem::size_of::<HIDD_ATTRIBUTES>() as u32;
        // SAFETY: valid handle and out-pointer.
        unsafe { HidD_GetAttributes(self.handle.raw(), &mut attrs) }
            .then_some((attrs.VendorID, attrs.ProductID))
    }

    pub fn waker(&self) -> Arc<Waker> {
        self.waker.clone()
    }

    /// The report completed by the last [`Wait::Report`].
    pub fn report(&self) -> &[u8] {
        &self.read.buf[..self.last_len]
    }

    fn start_read(&mut self) -> io::Result<()> {
        if self.pending {
            return Ok(());
        }
        let r = &mut *self.read;
        // SAFETY: zeroed OVERLAPPED with our event; buffer and OVERLAPPED stay alive and
        // in place (boxed) until the read completes or is cancelled in Drop.
        unsafe {
            r.ov = std::mem::zeroed();
            r.ov.hEvent = r.event.raw();
            if ReadFile(
                self.handle.raw(),
                r.buf.as_mut_ptr(),
                r.buf.len() as u32,
                null_mut(),
                &mut r.ov,
            ) == 0
                && GetLastError() != ERROR_IO_PENDING
            {
                return Err(io::Error::last_os_error());
            }
        }
        self.pending = true;
        Ok(())
    }

    /// Wait for the next report, a wake-up, or the timeout.
    pub fn wait(&mut self, timeout: Duration) -> io::Result<Wait> {
        self.start_read()?;
        let handles: [HANDLE; 2] = [self.read.event.raw(), self.waker.0.raw()];
        let ms = timeout.as_millis().min(u128::from(u32::MAX) - 1) as u32;
        // SAFETY: two valid event handles.
        let rc = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, ms) };
        match rc {
            r if r == WAIT_OBJECT_0 => {
                let mut n = 0u32;
                self.pending = false;
                // SAFETY: the read signalled completion; OVERLAPPED is ours.
                if unsafe { GetOverlappedResult(self.handle.raw(), &self.read.ov, &mut n, 0) } == 0
                {
                    return Err(io::Error::last_os_error());
                }
                self.last_len = n as usize;
                Ok(Wait::Report(n as usize))
            }
            r if r == WAIT_OBJECT_0 + 1 => Ok(Wait::Woken),
            WAIT_TIMEOUT => Ok(Wait::Timeout),
            _ => Err(io::Error::last_os_error()),
        }
    }

    /// Run one overlapped operation to completion (or cancel it after the timeout).
    fn complete(&self, ov: &mut OVERLAPPED, started: i32) -> io::Result<usize> {
        // SAFETY: `ov` belongs to an operation started on our handle; we wait for it to
        // finish (or cancel it) before returning, so its buffer outlives it.
        unsafe {
            if started == 0 && GetLastError() != ERROR_IO_PENDING {
                return Err(io::Error::last_os_error());
            }
            if WaitForSingleObject(ov.hEvent, WRITE_TIMEOUT_MS) == WAIT_TIMEOUT {
                CancelIoEx(self.handle.raw(), ov);
            }
            let mut n = 0u32;
            if GetOverlappedResult(self.handle.raw(), ov, &mut n, 1) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(n as usize)
        }
    }

    /// Send an output report, zero-padded to the device's output report length.
    pub fn write_report(&self, report: &[u8]) -> io::Result<()> {
        let mut buf = report.to_vec();
        if buf.len() < self.caps.output_len {
            buf.resize(self.caps.output_len, 0);
        }
        let ev = event(true)?;
        // SAFETY: a zeroed OVERLAPPED is the documented initial state.
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        ov.hEvent = ev.0;
        // SAFETY: `buf` and `ov` (with our event) live until `complete` has waited for the
        // write to finish.
        let started = unsafe {
            WriteFile(
                self.handle.raw(),
                buf.as_ptr(),
                buf.len() as u32,
                null_mut(),
                &mut ov,
            )
        };
        self.complete(&mut ov, started).map(drop)
    }

    /// Read a feature report into `buf` (`buf[0]` = report ID).
    pub fn feature(&self, buf: &mut [u8]) -> io::Result<usize> {
        let ev = event(true)?;
        // SAFETY: a zeroed OVERLAPPED is the documented initial state.
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        ov.hEvent = ev.0;
        // SAFETY: as in `write_report`; `buf` and `ov` outlive the operation.
        let started = unsafe {
            DeviceIoControl(
                self.handle.raw(),
                IOCTL_HID_GET_FEATURE,
                buf.as_ptr().cast(),
                buf.len() as u32,
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
                null_mut(),
                &mut ov,
            )
        };
        self.complete(&mut ov, started)
    }
}

impl HidIo {
    /// Send a feature report, padded to the device's feature length.
    pub fn set_feature(&self, report: &[u8]) -> io::Result<()> {
        let mut buf = report.to_vec();
        buf.resize(buf.len().max(self.caps().feature_len), 0);
        // SAFETY: `buf` is valid for its length during the call.
        if unsafe { HidD_SetFeature(self.handle.raw(), buf.as_mut_ptr().cast(), buf.len() as u32) }
        {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

impl ReportIo for HidIo {
    fn write(&mut self, report: &[u8]) -> io::Result<()> {
        self.write_report(report)
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match self.wait(left)? {
                Wait::Report(n) => {
                    let n = n.min(buf.len());
                    buf[..n].copy_from_slice(&self.read.buf[..n]);
                    return Ok(Some(n));
                }
                // Wake-ups only matter to the run loop; keep waiting.
                Wait::Woken if !left.is_zero() => {}
                _ => return Ok(None),
            }
        }
    }

    fn get_feature(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.feature(buf)
    }

    fn set_feature(&mut self, buf: &[u8]) -> io::Result<()> {
        HidIo::set_feature(self, buf)
    }
}

impl Drop for HidIo {
    fn drop(&mut self) {
        if self.pending {
            let mut n = 0u32;
            // SAFETY: cancel our own read and wait until the kernel is done with the
            // buffer before it is freed.
            unsafe {
                CancelIoEx(self.handle.raw(), &self.read.ov);
                GetOverlappedResult(self.handle.raw(), &self.read.ov, &mut n, 1);
            }
        }
    }
}

fn read_caps(handle: HANDLE) -> Option<HidCaps> {
    let mut pp = 0;
    // SAFETY: preparsed data is freed below.
    if !unsafe { HidD_GetPreparsedData(handle, &mut pp) } {
        return None;
    }
    // SAFETY: plain struct filled by HidP_GetCaps.
    let mut caps: HIDP_CAPS = unsafe { std::mem::zeroed() };
    // SAFETY: `pp` is the preparsed data read above, `caps` a valid out-pointer.
    let status = unsafe { HidP_GetCaps(pp, &mut caps) };
    // SAFETY: `pp` came from HidD_GetPreparsedData and is not used afterwards.
    unsafe { HidD_FreePreparsedData(pp) };
    (status == HIDP_STATUS_SUCCESS).then_some(HidCaps {
        usage_page: caps.UsagePage,
        usage: caps.Usage,
        input_len: caps.InputReportByteLength as usize,
        output_len: caps.OutputReportByteLength as usize,
        feature_len: caps.FeatureReportByteLength as usize,
    })
}
