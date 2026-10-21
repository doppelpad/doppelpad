//! Virtual gamepads through the ViGEmBus driver.
//!
//! The Xbox 360 pad uses the `vigem-client` crate. The DualShock 4 talks to the bus
//! directly, because the crate has neither the full input report (touchpad, motion) nor
//! the DualShock 4 output notifications (rumble, light bar). The request layouts are the
//! ones in ViGEmClient's public headers.

use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use dp_core::pad::{Axis, PadButton, PadState, Rgb, Rumble, axis_to_i16};
use vigem_client::{Client, TargetId, XButtons, XGamepad, Xbox360Wired};
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_IO_PENDING, ERROR_OPERATION_ABORTED, GetLastError, HANDLE,
};
use windows_sys::Win32::System::IO::{
    CancelIoEx, DeviceIoControl, GetOverlappedResult, OVERLAPPED,
};
use windows_sys::Win32::System::Threading::CreateEventW;

use super::OwnedHandle;
use crate::sony::{DS4_REPORT_LEN, Ds4Calibration, Ds4ReportBuilder};
use crate::{PlatformError, Result, VirtualPad, VirtualPadFactory, VirtualPadKind};

fn vigem_err(e: vigem_client::Error) -> PlatformError {
    match e {
        vigem_client::Error::BusNotFound => PlatformError::DriverMissing(
            "ViGEmBus",
            "install it from https://github.com/nefarius/ViGEmBus/releases".into(),
        ),
        other => PlatformError::Other(format!("ViGEmBus: {other}")),
    }
}

pub struct VigemFactory;

impl VigemFactory {
    pub fn new() -> Self {
        VigemFactory
    }
}

impl VirtualPadFactory for VigemFactory {
    fn available(&self) -> Result<()> {
        Client::connect().map(drop).map_err(vigem_err)
    }

    fn create(&mut self, kind: VirtualPadKind) -> Result<Box<dyn VirtualPad>> {
        let client = Client::connect().map_err(vigem_err)?;
        match kind {
            VirtualPadKind::Xbox360 => {
                let mut target = Xbox360Wired::new(client, TargetId::XBOX360_WIRED);
                target.plugin().map_err(vigem_err)?;
                target.wait_ready().map_err(vigem_err)?;
                let slot = target.get_user_index().ok();
                let rumble = Arc::new(Mutex::new(None));
                // Rumble notifications arrive on a dedicated thread, which exits when the target is unplugged.
                match target.request_notification() {
                    Ok(req) => {
                        let sink = rumble.clone();
                        req.spawn_thread(move |_, n| {
                            *sink.lock().unwrap() = Some(Rumble {
                                low: f32::from(n.large_motor) / 255.0,
                                high: f32::from(n.small_motor) / 255.0,
                            });
                        });
                    }
                    Err(e) => tracing::warn!("no rumble feedback from virtual Xbox 360 pad: {e}"),
                }
                tracing::info!(?slot, "virtual Xbox 360 controller plugged in");
                Ok(Box::new(X360Pad {
                    target,
                    slot,
                    rumble,
                }))
            }
            VirtualPadKind::DualShock4 => {
                let pad = Ds4Pad::plug_in(client)?;
                tracing::info!(
                    serial = pad.serial,
                    "virtual DualShock 4 controller plugged in"
                );
                Ok(Box::new(pad))
            }
        }
    }
}

struct X360Pad {
    target: Xbox360Wired<Client>,
    slot: Option<u32>,
    rumble: Arc<Mutex<Option<Rumble>>>,
}

const X360_BUTTONS: [(PadButton, u16); 15] = [
    (PadButton::DpadUp, XButtons::UP),
    (PadButton::DpadDown, XButtons::DOWN),
    (PadButton::DpadLeft, XButtons::LEFT),
    (PadButton::DpadRight, XButtons::RIGHT),
    (PadButton::Start, XButtons::START),
    (PadButton::Back, XButtons::BACK),
    (PadButton::LS, XButtons::LTHUMB),
    (PadButton::RS, XButtons::RTHUMB),
    (PadButton::LB, XButtons::LB),
    (PadButton::RB, XButtons::RB),
    (PadButton::Guide, XButtons::GUIDE),
    (PadButton::A, XButtons::A),
    (PadButton::B, XButtons::B),
    (PadButton::X, XButtons::X),
    (PadButton::Y, XButtons::Y),
];

pub fn to_xgamepad(s: &PadState) -> XGamepad {
    let mut raw = 0u16;
    for (b, bit) in X360_BUTTONS {
        if s.pressed(b) {
            raw |= bit;
        }
    }
    XGamepad {
        buttons: XButtons { raw },
        left_trigger: (s.axis(Axis::LeftTrigger).clamp(0.0, 1.0) * 255.0).round() as u8,
        right_trigger: (s.axis(Axis::RightTrigger).clamp(0.0, 1.0) * 255.0).round() as u8,
        thumb_lx: axis_to_i16(s.axis(Axis::LeftX)),
        thumb_ly: axis_to_i16(s.axis(Axis::LeftY)),
        thumb_rx: axis_to_i16(s.axis(Axis::RightX)),
        thumb_ry: axis_to_i16(s.axis(Axis::RightY)),
    }
}

impl VirtualPad for X360Pad {
    fn kind(&self) -> VirtualPadKind {
        VirtualPadKind::Xbox360
    }
    fn update(&mut self, state: &PadState) -> Result<()> {
        self.target.update(&to_xgamepad(state)).map_err(vigem_err)
    }
    fn take_rumble(&mut self) -> Option<Rumble> {
        self.rumble.lock().unwrap().take()
    }
    fn xinput_slot(&self) -> Option<u32> {
        self.slot
    }
}

impl Drop for X360Pad {
    fn drop(&mut self) {
        let _ = self.target.unplug();
    }
}

// Bus requests: CTL_CODE(FILE_DEVICE_BUS_EXTENDER, 0x801 + n, METHOD_BUFFERED, access).
const IOCTL_PLUGIN_TARGET: u32 = 0x2A_A004;
const IOCTL_UNPLUG_TARGET: u32 = 0x2A_A008;
const IOCTL_WAIT_DEVICE_READY: u32 = 0x2A_A010;
const IOCTL_DS4_SUBMIT_REPORT: u32 = 0x2A_A80C;
const IOCTL_DS4_REQUEST_NOTIFICATION: u32 = 0x2A_A810;

const TARGET_TYPE_DUALSHOCK4: i32 = 2;
const DS4_VID: u16 = 0x054C;
const DS4_PID: u16 = 0x05C4;
/// Serial numbers tried when plugging in; the bus refuses ones that are taken.
const MAX_SERIAL: u32 = 1024;

/// The calibration report the bus answers for every virtual DualShock 4 (fixed values
/// taken from a real controller). Games that calibrate apply it to our motion data.
const VIGEM_DS4_CALIBRATION: [u8; 37] = [
    0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x87, 0x22, 0x7B, 0xDD, 0xB2, 0x22, 0x47, 0xDD, 0xBD,
    0x22, 0x43, 0xDD, 0x1C, 0x02, 0x1C, 0x02, 0x7F, 0x1E, 0x2E, 0xDF, 0x60, 0x1F, 0x4C, 0xE0, 0x3A,
    0x1D, 0xC6, 0xDE, 0x08, 0x00,
];

#[repr(C)]
struct PluginTarget {
    size: u32,
    serial: u32,
    target_type: i32,
    vendor: u16,
    product: u16,
}

/// Unplug and wait-ready requests.
#[repr(C)]
struct SerialRequest {
    size: u32,
    serial: u32,
}

#[repr(C, packed)]
struct Ds4SubmitReportEx {
    size: u32,
    serial: u32,
    report: [u8; DS4_REPORT_LEN],
}

#[repr(C)]
struct Ds4Notification {
    size: u32,
    serial: u32,
    small_motor: u8,
    large_motor: u8,
    red: u8,
    green: u8,
    blue: u8,
}

fn size_of<T>() -> u32 {
    std::mem::size_of::<T>() as u32
}

/// Run one buffered request on the overlapped bus handle and wait for it. `out` says
/// whether the driver writes its answer back into `req`. Returns the Win32 error code.
fn bus_request<T>(
    bus: HANDLE,
    event: &OwnedHandle,
    code: u32,
    req: &mut T,
    out: bool,
) -> std::result::Result<(), u32> {
    let len = size_of::<T>();
    let ptr: *mut c_void = (req as *mut T).cast();
    // SAFETY: `req` lives until the request finished (we wait for it); the event is ours.
    unsafe {
        let mut ov: OVERLAPPED = std::mem::zeroed();
        ov.hEvent = event.0;
        let started = DeviceIoControl(
            bus,
            code,
            ptr,
            len,
            if out { ptr } else { null_mut() },
            if out { len } else { 0 },
            null_mut(),
            &mut ov,
        );
        if started == 0 {
            let err = GetLastError();
            if err != ERROR_IO_PENDING {
                return Err(err);
            }
        }
        let mut n = 0u32;
        if GetOverlappedResult(bus, &ov, &mut n, 1) == 0 {
            return Err(GetLastError());
        }
    }
    Ok(())
}

fn new_event() -> Result<OwnedHandle> {
    // SAFETY: plain unnamed auto-reset event.
    let h = OwnedHandle(unsafe { CreateEventW(null(), 0, 0, null()) });
    if h.valid() {
        Ok(h)
    } else {
        Err(PlatformError::last_os("CreateEventW"))
    }
}

/// What the game last sent to the virtual DualShock 4.
#[derive(Default)]
struct Ds4Feedback {
    rumble: Option<Rumble>,
    lightbar: Option<Rgb>,
}

struct Ds4Pad {
    client: Client,
    event: OwnedHandle,
    serial: u32,
    builder: Ds4ReportBuilder,
    created: Instant,
    feedback: Arc<Mutex<Ds4Feedback>>,
    /// The notification thread's own bus handle, so `drop` can cancel its request.
    notify: Option<Arc<Client>>,
    stop: Arc<AtomicBool>,
}

impl Ds4Pad {
    fn plug_in(client: Client) -> Result<Ds4Pad> {
        let event = new_event()?;
        let bus = client.as_raw_handle() as HANDLE;
        let mut serial = 0;
        let mut last_err = 0;
        for candidate in 1..=MAX_SERIAL {
            let mut req = PluginTarget {
                size: size_of::<PluginTarget>(),
                serial: candidate,
                target_type: TARGET_TYPE_DUALSHOCK4,
                vendor: DS4_VID,
                product: DS4_PID,
            };
            match bus_request(bus, &event, IOCTL_PLUGIN_TARGET, &mut req, false) {
                Ok(()) => {
                    serial = candidate;
                    break;
                }
                Err(e) => last_err = e,
            }
        }
        if serial == 0 {
            return Err(PlatformError::Other(format!(
                "ViGEmBus: cannot plug in a DualShock 4 (error {last_err})"
            )));
        }
        let mut wait = SerialRequest {
            size: size_of::<SerialRequest>(),
            serial,
        };
        // Buses before 1.17 do not know this request; the pad works without it.
        if let Err(e) = bus_request(bus, &event, IOCTL_WAIT_DEVICE_READY, &mut wait, false) {
            tracing::debug!("ViGEmBus wait-ready failed with error {e}");
        }
        let calibration =
            Ds4Calibration::from_usb_feature(&VIGEM_DS4_CALIBRATION).unwrap_or_default();
        let mut pad = Ds4Pad {
            client,
            event,
            serial,
            builder: Ds4ReportBuilder::new(calibration),
            created: Instant::now(),
            feedback: Arc::default(),
            notify: None,
            stop: Arc::new(AtomicBool::new(false)),
        };
        match pad.client.try_clone() {
            Ok(c) => pad.spawn_notifications(Arc::new(c)),
            Err(e) => tracing::warn!("no rumble feedback from virtual DualShock 4: {e}"),
        }
        Ok(pad)
    }

    /// Rumble and light bar from games arrive as answers to a pending request, which
    /// the bus aborts when the target is unplugged.
    fn spawn_notifications(&mut self, bus: Arc<Client>) {
        let serial = self.serial;
        let sink = self.feedback.clone();
        let stop = self.stop.clone();
        let thread_bus = bus.clone();
        let spawned = std::thread::Builder::new()
            .name("dp-vigem-ds4".into())
            .spawn(move || {
                let Ok(event) = new_event() else { return };
                let handle = thread_bus.as_raw_handle() as HANDLE;
                while !stop.load(Ordering::Relaxed) {
                    let mut n = Ds4Notification {
                        size: size_of::<Ds4Notification>(),
                        serial,
                        small_motor: 0,
                        large_motor: 0,
                        red: 0,
                        green: 0,
                        blue: 0,
                    };
                    match bus_request(handle, &event, IOCTL_DS4_REQUEST_NOTIFICATION, &mut n, true)
                    {
                        Ok(()) => {
                            let mut f = sink.lock().unwrap();
                            f.rumble = Some(Rumble {
                                low: f32::from(n.large_motor) / 255.0,
                                high: f32::from(n.small_motor) / 255.0,
                            });
                            f.lightbar = Some(Rgb {
                                r: n.red,
                                g: n.green,
                                b: n.blue,
                            });
                        }
                        Err(ERROR_OPERATION_ABORTED | ERROR_ACCESS_DENIED) => break,
                        Err(e) => {
                            tracing::debug!("virtual DualShock 4 notification failed: {e}");
                            break;
                        }
                    }
                }
            });
        match spawned {
            Ok(_) => self.notify = Some(bus),
            Err(e) => tracing::warn!("no rumble feedback from virtual DualShock 4: {e}"),
        }
    }
}

impl VirtualPad for Ds4Pad {
    fn kind(&self) -> VirtualPadKind {
        VirtualPadKind::DualShock4
    }

    fn update(&mut self, state: &PadState) -> Result<()> {
        let elapsed = self.created.elapsed().as_micros() as u64;
        let mut req = Ds4SubmitReportEx {
            size: size_of::<Ds4SubmitReportEx>(),
            serial: self.serial,
            report: self.builder.build(state, elapsed),
        };
        let bus = self.client.as_raw_handle() as HANDLE;
        bus_request(bus, &self.event, IOCTL_DS4_SUBMIT_REPORT, &mut req, false).map_err(|e| {
            PlatformError::Other(format!("ViGEmBus: DualShock 4 update failed (error {e})"))
        })
    }

    fn take_rumble(&mut self) -> Option<Rumble> {
        self.feedback.lock().unwrap().rumble.take()
    }

    fn take_lightbar(&mut self) -> Option<Rgb> {
        self.feedback.lock().unwrap().lightbar.take()
    }
}

impl Drop for Ds4Pad {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let mut req = SerialRequest {
            size: size_of::<SerialRequest>(),
            serial: self.serial,
        };
        let bus = self.client.as_raw_handle() as HANDLE;
        if let Err(e) = bus_request(bus, &self.event, IOCTL_UNPLUG_TARGET, &mut req, false) {
            tracing::warn!("cannot unplug the virtual DualShock 4 (error {e})");
        }
        if let Some(n) = &self.notify {
            // SAFETY: cancels whatever the notification thread has pending on its handle.
            unsafe { CancelIoEx(n.as_raw_handle() as HANDLE, null()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_layouts_match_the_bus() {
        assert_eq!(size_of::<PluginTarget>(), 16);
        assert_eq!(size_of::<SerialRequest>(), 8);
        assert_eq!(size_of::<Ds4SubmitReportEx>(), 71);
        assert_eq!(size_of::<Ds4Notification>(), 16);
    }

    #[test]
    fn bus_calibration_decodes() {
        let c = Ds4Calibration::from_usb_feature(&VIGEM_DS4_CALIBRATION).unwrap();
        assert!(c.gyro_scale.iter().all(|s| (16.0..17.0).contains(s)));
        assert!(c.accel_scale.iter().all(|s| (7900.0..8200.0).contains(s)));
    }

    #[test]
    fn x360_neutral() {
        let g = to_xgamepad(&PadState::default());
        assert_eq!(g.buttons.raw, 0);
        assert_eq!((g.thumb_lx, g.left_trigger), (0, 0));
    }
}
