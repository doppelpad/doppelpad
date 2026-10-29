//! Overlapped HID device handle.
//!
//! One read is always pending; [`HidHandle::wait`] waits for it, for a timeout, or for the
//! [`Waker`] that another thread can signal. Writes and feature reports (read and sent) use
//! their own `OVERLAPPED` and give up after [`WRITE_TIMEOUT`], so they never queue behind the
//! pending read the way they would on a synchronous handle, and never block for good.

use std::ffi::c_void;
use std::io;
use std::ptr::null_mut;
use std::sync::Arc;
use std::time::Duration;

use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HIDD_ATTRIBUTES, HidD_GetAttributes, HidD_GetProductString, HidD_GetSerialNumberString,
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
use windows_sys::Win32::System::Threading::{SetEvent, WaitForMultipleObjects};

use super::hidparse::PreparsedData;
use super::overlapped::{EventReset, event, run_overlapped};
use super::{OwnedHandle, from_wide, wide};
use crate::hidhost::{HidHandle, Wait, Wake, read_via_wait};
use crate::padio::ReportIo;

/// `CTL_CODE(FILE_DEVICE_KEYBOARD, 100, METHOD_OUT_DIRECT, FILE_ANY_ACCESS)`.
const IOCTL_HID_GET_FEATURE: u32 = 0x000B_0192;
/// `CTL_CODE(FILE_DEVICE_KEYBOARD, 100, METHOD_IN_DIRECT, FILE_ANY_ACCESS)`.
const IOCTL_HID_SET_FEATURE: u32 = 0x000B_0191;
/// Bluetooth writes can take a while; give up after this.
const WRITE_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct HidCaps {
    pub usage_page: u16,
    pub usage: u16,
    pub input_len: usize,
    pub output_len: usize,
    pub feature_len: usize,
}

/// Wakes a thread blocked in [`HidHandle::wait`].
pub struct Waker(OwnedHandle);

impl Wake for Waker {
    fn wake(&self) {
        // SAFETY: valid event handle.
        unsafe { SetEvent(self.0.raw()) };
    }
}

/// The pending read: lives on the heap so its address stays fixed while the kernel
/// writes into it.
struct PendingRead {
    ov: OVERLAPPED,
    buf: Vec<u8>,
    event: OwnedHandle,
}

pub struct HidIo {
    handle: OwnedHandle,
    read: Box<PendingRead>,
    pending: bool,
    last_len: usize,
    waker: Arc<Waker>,
    /// Signalled by writes and feature reads, which run one at a time.
    io_event: OwnedHandle,
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
                ov: OVERLAPPED::default(),
                buf: vec![0u8; caps.input_len.max(64)],
                event: event(EventReset::Manual)?,
            }),
            handle,
            pending: false,
            last_len: 0,
            waker: Arc::new(Waker(event(EventReset::Auto)?)),
            io_event: event(EventReset::Manual)?,
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
        let mut attrs = HIDD_ATTRIBUTES {
            Size: size_of::<HIDD_ATTRIBUTES>() as u32,
            ..Default::default()
        };
        // SAFETY: valid handle and out-pointer.
        unsafe { HidD_GetAttributes(self.handle.raw(), &mut attrs) }
            .then_some((attrs.VendorID, attrs.ProductID))
    }

    fn start_read(&mut self) -> io::Result<()> {
        if self.pending {
            return Ok(());
        }
        let r = &mut *self.read;
        r.ov = OVERLAPPED {
            hEvent: r.event.raw(),
            ..Default::default()
        };
        // SAFETY: buffer and OVERLAPPED stay alive and in place (boxed) until the read
        // completes or is cancelled in Drop.
        let started = unsafe {
            ReadFile(
                self.handle.raw(),
                r.buf.as_mut_ptr(),
                r.buf.len() as u32,
                null_mut(),
                &mut r.ov,
            )
        };
        // SAFETY: reads the calling thread's last error.
        if started == 0 && unsafe { GetLastError() } != ERROR_IO_PENDING {
            return Err(io::Error::last_os_error());
        }
        self.pending = true;
        Ok(())
    }

    /// Wait for the next report, a wake-up, or the timeout.
    fn wait_report(&mut self, timeout: Duration) -> io::Result<Wait> {
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
                Ok(Wait::Report)
            }
            r if r == WAIT_OBJECT_0 + 1 => Ok(Wait::Woken),
            WAIT_TIMEOUT => Ok(Wait::Timeout),
            _ => Err(io::Error::last_os_error()),
        }
    }

    /// Send an output report, zero-padded to the device's output report length.
    pub fn write_report(&self, report: &[u8]) -> io::Result<()> {
        let mut buf = report.to_vec();
        if buf.len() < self.caps.output_len {
            buf.resize(self.caps.output_len, 0);
        }
        let handle = self.handle.raw();
        let start = |ov| {
            // SAFETY: valid handle, buffer and OVERLAPPED.
            unsafe { WriteFile(handle, buf.as_ptr(), buf.len() as u32, null_mut(), ov) }
        };
        // SAFETY: `buf` lives until the write has finished.
        unsafe { run_overlapped(handle, &self.io_event, Some(WRITE_TIMEOUT), start) }.map(drop)
    }

    /// Read a feature report into `buf` (`buf[0]` = report ID).
    pub fn feature(&self, buf: &mut [u8]) -> io::Result<usize> {
        let handle = self.handle.raw();
        let (ptr, len) = (buf.as_mut_ptr(), buf.len() as u32);
        let start = |ov| {
            // SAFETY: valid handle, one buffer for input and output, and OVERLAPPED.
            unsafe {
                DeviceIoControl(
                    handle,
                    IOCTL_HID_GET_FEATURE,
                    ptr.cast_const().cast(),
                    len,
                    ptr.cast(),
                    len,
                    null_mut(),
                    ov,
                )
            }
        };
        // SAFETY: `buf` lives until the request has finished.
        let n = unsafe { run_overlapped(handle, &self.io_event, Some(WRITE_TIMEOUT), start) }?;
        Ok(n as usize)
    }

    /// Send a feature report (`report[0]` = report ID), padded to the device's feature
    /// length. Overlapped, as a write: `HidD_SetFeature` would block on this handle without a
    /// time limit.
    pub fn set_feature(&self, report: &[u8]) -> io::Result<()> {
        let mut buf = report.to_vec();
        buf.resize(buf.len().max(self.caps.feature_len), 0);
        let handle = self.handle.raw();
        let (ptr, len) = (buf.as_ptr(), buf.len() as u32);
        let start = |ov| {
            // SAFETY: valid handle, the report as input buffer, and OVERLAPPED.
            unsafe {
                DeviceIoControl(
                    handle,
                    IOCTL_HID_SET_FEATURE,
                    ptr.cast(),
                    len,
                    null_mut(),
                    0,
                    null_mut(),
                    ov,
                )
            }
        };
        // SAFETY: `buf` lives until the request has finished.
        unsafe { run_overlapped(handle, &self.io_event, Some(WRITE_TIMEOUT), start) }.map(drop)
    }
}

impl ReportIo for HidIo {
    fn write(&mut self, report: &[u8]) -> io::Result<()> {
        self.write_report(report)
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        read_via_wait(self, buf, timeout)
    }

    fn get_feature(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.feature(buf)
    }

    fn set_feature(&mut self, buf: &[u8]) -> io::Result<()> {
        HidIo::set_feature(self, buf)
    }
}

impl HidHandle for HidIo {
    fn wait(&mut self, timeout: Duration) -> io::Result<Wait> {
        self.wait_report(timeout)
    }

    fn report(&self) -> &[u8] {
        &self.read.buf[..self.last_len]
    }

    fn waker(&self) -> Arc<dyn Wake> {
        self.waker.clone()
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
    let caps = PreparsedData::get(handle)?.caps()?;
    Some(HidCaps {
        usage_page: caps.UsagePage,
        usage: caps.Usage,
        input_len: caps.InputReportByteLength as usize,
        output_len: caps.OutputReportByteLength as usize,
        feature_len: caps.FeatureReportByteLength as usize,
    })
}
