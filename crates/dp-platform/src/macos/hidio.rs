//! An IOKit HID device, driven by the thread that owns it.
//!
//! Input reports arrive through a callback on that thread's run loop: [`HidHandle::wait`]
//! runs the loop until a report is queued, the time is up, or the [`Waker`] signals the
//! loop from another thread. Output and feature reports are sent synchronously.
//!
//! Reports cross this boundary the way the drivers know them from Windows: the first byte is
//! the report ID, 0 for devices that do not number their reports (IOKit leaves the byte out
//! for those).

use std::collections::VecDeque;
use std::ffi::c_void;
use std::io;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use objc2_core_foundation::{
    CFIndex, CFRetained, CFRunLoop, CFRunLoopSource, CFRunLoopSourceContext, kCFRunLoopDefaultMode,
};
use objc2_io_kit::{
    IOHIDDevice, IOHIDReportType, IOReturn, kIOHIDOptionsTypeNone, kIOHIDOptionsTypeSeizeDevice,
    kIOReturnSuccess,
};

use super::props::Props;
use crate::hidhost::{HidHandle, Wait, Wake, lock, read_via_wait};
use crate::padio::ReportIo;

/// Reports kept while nobody reads; older ones are dropped beyond this.
const QUEUE: usize = 64;

/// Where the report callback leaves its reports. Lives on the heap at a fixed address,
/// which IOKit holds as the callback's context.
struct Inbox {
    /// IOKit copies each report here before the callback runs.
    buf: Vec<u8>,
    queue: VecDeque<Vec<u8>>,
    removed: bool,
}

/// The run loop of the thread that owns a device, once it runs, and the source that wakes it.
struct Loop {
    run_loop: CFRetained<CFRunLoop>,
    source: CFRetained<CFRunLoopSource>,
}

// SAFETY: CFRunLoopSourceSignal and CFRunLoopWakeUp may be called from any thread; nothing
// else is done with these from other threads.
unsafe impl Send for Loop {}
// SAFETY: as above.
unsafe impl Sync for Loop {}

/// Wakes a device thread blocked in [`HidHandle::wait`].
#[derive(Default)]
pub struct Waker {
    woken: AtomicBool,
    /// Set once the device thread has scheduled the device on its run loop.
    target: Mutex<Option<Loop>>,
}

impl Wake for Waker {
    fn wake(&self) {
        self.woken.store(true, Ordering::Release);
        if let Some(l) = &*lock(&self.target) {
            l.source.signal();
            l.run_loop.wake_up();
        }
    }
}

pub struct HidIo {
    device: CFRetained<IOHIDDevice>,
    /// Owned through this pointer alone, and freed in `Drop` once the device is closed: IOKit
    /// holds it as the callbacks' context and writes through it while the run loop runs, so no
    /// `Box` or `&mut` may stand for it across a run (see [`HidIo::inbox`]).
    inbox: NonNull<Inbox>,
    /// The report the last [`Wait::Report`] delivered, report ID first.
    report: Vec<u8>,
    /// The device numbers its reports (its descriptor has report IDs).
    numbered: bool,
    output_len: usize,
    feature_len: usize,
    exclusive: bool,
    scheduled: bool,
    waker: Arc<Waker>,
}

// SAFETY: the device is opened on the scanner thread and then moves to its own thread once,
// where it is scheduled and used alone, and so is the inbox its callbacks fill there.
// IOHIDDevice calls may come from any thread.
unsafe impl Send for HidIo {}

impl HidIo {
    /// The inbox, between two runs of the run loop.
    fn inbox(&mut self) -> &mut Inbox {
        // SAFETY: the callbacks reach the inbox only while the run loop runs, on this thread,
        // and the borrow ends before `wait` runs it again (`&mut self` keeps it to one).
        unsafe { &mut *self.inbox.as_ptr() }
    }

    /// Open `device` for sharing with other programs.
    pub fn open(device: CFRetained<IOHIDDevice>, props: &Props) -> io::Result<HidIo> {
        check(device.open(kIOHIDOptionsTypeNone), "open")?;
        let input_len = props.max_input.max(64);
        let inbox = NonNull::from(Box::leak(Box::new(Inbox {
            buf: vec![0; input_len],
            queue: VecDeque::new(),
            removed: false,
        })));
        // SAFETY: the inbox was just made and nothing else reaches it yet.
        let buf = unsafe { &mut (*inbox.as_ptr()).buf };
        let (report_buf, report_len) = (NonNull::from(&mut buf[..]).cast::<u8>(), buf.len());
        // SAFETY: the buffer and the inbox stay at their heap addresses until `Drop` closes
        // the device, after which IOKit calls back no more, and frees them only then.
        unsafe {
            device.register_input_report_callback(
                report_buf,
                report_len as CFIndex,
                Some(on_report),
                inbox.as_ptr().cast(),
            );
            device.register_removal_callback(Some(on_removed), inbox.as_ptr().cast());
        }
        Ok(HidIo {
            device,
            inbox,
            report: Vec::new(),
            numbered: props.numbered,
            output_len: props.max_output,
            feature_len: props.max_feature,
            exclusive: false,
            scheduled: false,
            waker: Arc::default(),
        })
    }

    /// Deliver input reports on the calling thread's run loop from now on.
    fn schedule(&mut self) -> io::Result<()> {
        if self.scheduled {
            return Ok(());
        }
        let run_loop = CFRunLoop::current().ok_or_else(|| io::Error::other("no run loop"))?;
        let mut context = CFRunLoopSourceContext {
            version: 0,
            info: std::ptr::null_mut(),
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: Some(on_wake),
        };
        // SAFETY: the context is copied by CFRunLoopSourceCreate; its callbacks use no info.
        let source = unsafe { CFRunLoopSource::new(None, 0, &mut context) }
            .ok_or_else(|| io::Error::other("cannot create a run loop source"))?;
        // SAFETY: the default mode is a constant CoreFoundation provides.
        let mode = unsafe { kCFRunLoopDefaultMode };
        run_loop.add_source(Some(&source), mode);
        // SAFETY: the run loop is this thread's, which drives the device from now on.
        unsafe { self.device.schedule_with_run_loop(&run_loop, mode.unwrap()) };
        *lock(&self.waker.target) = Some(Loop { run_loop, source });
        self.scheduled = true;
        Ok(())
    }

    /// Send a report of `kind`, `report[0]` being its ID, padded to `len`.
    fn send(&self, kind: IOHIDReportType, report: &[u8], len: usize) -> io::Result<()> {
        let mut buf = report.to_vec();
        if buf.len() < len {
            buf.resize(len, 0);
        }
        let id = buf.first().copied().unwrap_or(0);
        let body = if self.numbered {
            &mut buf[..]
        } else {
            &mut buf[1..]
        };
        let Some(ptr) = NonNull::new(body.as_mut_ptr()) else {
            return Ok(());
        };
        // SAFETY: the buffer is valid for its length during the call.
        let rc = unsafe {
            self.device
                .set_report(kind, CFIndex::from(id), ptr, body.len() as CFIndex)
        };
        check(rc, "send report")
    }
}

impl ReportIo for HidIo {
    fn write(&mut self, report: &[u8]) -> io::Result<()> {
        self.send(
            IOHIDReportType::Output,
            report,
            self.output_len + usize::from(!self.numbered),
        )
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        read_via_wait(self, buf, timeout)
    }

    fn get_feature(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let id = buf.first().copied().unwrap_or(0);
        let skip = usize::from(!self.numbered);
        let Some(body) = buf.get_mut(skip..) else {
            return Ok(0);
        };
        let mut len = body.len() as CFIndex;
        let Some(ptr) = NonNull::new(body.as_mut_ptr()) else {
            return Ok(0);
        };
        // SAFETY: the buffer is valid for `len` bytes during the call; IOKit writes `len`.
        let rc = unsafe {
            self.device.report(
                IOHIDReportType::Feature,
                CFIndex::from(id),
                ptr,
                NonNull::from(&mut len),
            )
        };
        check(rc, "read feature report")?;
        Ok(usize::try_from(len).unwrap_or(0) + skip)
    }

    fn set_feature(&mut self, buf: &[u8]) -> io::Result<()> {
        self.send(
            IOHIDReportType::Feature,
            buf,
            self.feature_len + usize::from(!self.numbered),
        )
    }
}

impl HidHandle for HidIo {
    fn wait(&mut self, timeout: Duration) -> io::Result<Wait> {
        self.schedule()?;
        let deadline = Instant::now() + timeout;
        loop {
            let (removed, next) = {
                let inbox = self.inbox();
                (inbox.removed, inbox.queue.pop_front())
            };
            if removed {
                return Err(io::ErrorKind::NotConnected.into());
            }
            if let Some(r) = next {
                self.report.clear();
                if !self.numbered {
                    self.report.push(0);
                }
                self.report.extend_from_slice(&r);
                return Ok(Wait::Report);
            }
            if self.waker.woken.swap(false, Ordering::Acquire) {
                return Ok(Wait::Woken);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(Wait::Timeout);
            }
            // SAFETY: the default mode is a constant CoreFoundation provides.
            let mode = unsafe { kCFRunLoopDefaultMode };
            CFRunLoop::run_in_mode(mode, left.as_secs_f64(), true);
        }
    }

    fn report(&self) -> &[u8] {
        &self.report
    }

    fn waker(&self) -> Arc<dyn Wake> {
        self.waker.clone()
    }

    /// Seizing a device keeps its reports from every other program, the GameController
    /// framework included, until it is closed: how macOS hides a controller.
    fn set_exclusive(&mut self, on: bool) -> io::Result<()> {
        if on == self.exclusive {
            return Ok(());
        }
        self.device.close(kIOHIDOptionsTypeNone);
        let options = if on {
            kIOHIDOptionsTypeSeizeDevice
        } else {
            kIOHIDOptionsTypeNone
        };
        match check(self.device.open(options), "reopen") {
            Ok(()) => {
                self.exclusive = on;
                Ok(())
            }
            Err(e) => {
                // Shared at least, so that reading goes on.
                check(self.device.open(kIOHIDOptionsTypeNone), "reopen")?;
                self.exclusive = false;
                Err(e)
            }
        }
    }
}

impl Drop for HidIo {
    fn drop(&mut self) {
        let scheduled = lock(&self.waker.target).take();
        if let Some(l) = scheduled {
            // SAFETY: the device was scheduled on this run loop, on this thread.
            unsafe {
                self.device
                    .unschedule_from_run_loop(&l.run_loop, kCFRunLoopDefaultMode.unwrap());
            }
            l.source.invalidate();
        }
        self.device.close(kIOHIDOptionsTypeNone);
        // SAFETY: made from a `Box` in `open`; the device is closed and unscheduled, so IOKit
        // calls back no more.
        drop(unsafe { Box::from_raw(self.inbox.as_ptr()) });
    }
}

/// IOKit's answer when another program has seized the device.
const EXCLUSIVE_ACCESS: IOReturn = 0xE000_02C5_u32 as IOReturn;

/// A failed IOKit call.
#[derive(Debug)]
struct IoKitError {
    what: &'static str,
    code: IOReturn,
}

impl std::fmt::Display for IoKitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: IOKit error {:#010x}", self.what, self.code as u32)
    }
}

impl std::error::Error for IoKitError {}

/// An IOKit result as an I/O error.
fn check(rc: IOReturn, what: &'static str) -> io::Result<()> {
    if rc == kIOReturnSuccess {
        Ok(())
    } else {
        Err(io::Error::other(IoKitError { what, code: rc }))
    }
}

/// Whether opening failed because another program keeps the device to itself.
pub fn is_busy(e: &io::Error) -> bool {
    e.get_ref()
        .and_then(|e| e.downcast_ref::<IoKitError>())
        .is_some_and(|e| e.code == EXCLUSIVE_ACCESS)
}

/// IOKit's input report callback: queue a copy of the report.
unsafe extern "C-unwind" fn on_report(
    context: *mut c_void,
    _result: IOReturn,
    _sender: *mut c_void,
    _kind: IOHIDReportType,
    _id: u32,
    report: NonNull<u8>,
    len: CFIndex,
) {
    // SAFETY: the context is the device's inbox, alive while the device is open; the
    // callback runs on the device thread, the only one that touches the inbox.
    let inbox = unsafe { &mut *context.cast::<Inbox>() };
    let len = usize::try_from(len).unwrap_or(0);
    // SAFETY: IOKit hands over `len` valid bytes.
    let bytes = unsafe { std::slice::from_raw_parts(report.as_ptr(), len) };
    if inbox.queue.len() == QUEUE {
        inbox.queue.pop_front();
    }
    inbox.queue.push_back(bytes.to_vec());
}

/// IOKit's removal callback: the device is gone.
unsafe extern "C-unwind" fn on_removed(
    context: *mut c_void,
    _result: IOReturn,
    _sender: *mut c_void,
) {
    // SAFETY: as in `on_report`.
    unsafe { (*context.cast::<Inbox>()).removed = true };
}

/// The wake-up source fired; [`Waker::woken`] already says why.
unsafe extern "C-unwind" fn on_wake(_info: *mut c_void) {}
