//! Virtual gamepads through the ViGEmBus driver.
//!
//! The Xbox 360 pad uses the `vigem-client` crate. The DualShock 4 talks to the bus
//! directly, because the crate has neither the full input report (touchpad, motion) nor
//! the DualShock 4 output notifications (rumble, light bar). The request layouts are the
//! ones in ViGEmClient's public headers.

use std::ffi::c_void;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use dp_core::pad::{Axis, PadState, Rgb, Rumble, axis_to_i16, unit_to_u8_round};
use vigem_client::{Client, TargetId, XButtons, XGamepad, Xbox360Wired};
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_OPERATION_ABORTED, HANDLE};
use windows_sys::Win32::System::IO::{CancelIoEx, DeviceIoControl};

use super::OwnedHandle;
use super::overlapped::{self, EventReset, run_overlapped};
use super::xinput::XUSB_BUTTONS;
use crate::sony::{DS4_REPORT_LEN, DS4_V1_PID, Ds4Calibration, Ds4ReportBuilder, SONY_VID};
use crate::{PlatformError, Result, VirtualPad, VirtualPadFactory, VirtualPadKind};

fn vigem_err(e: vigem_client::Error) -> PlatformError {
    match e {
        vigem_client::Error::BusNotFound => PlatformError::DriverMissing(
            "ViGEmBus",
            format!("install it from {}", crate::VIGEM_RELEASES),
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
                            *sink.lock().unwrap() =
                                Some(Rumble::from_motors_u8(n.large_motor, n.small_motor));
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

pub(super) fn to_xgamepad(s: &PadState) -> XGamepad {
    let mut raw = 0u16;
    for (b, bit) in XUSB_BUTTONS {
        if s.pressed(b) {
            raw |= bit;
        }
    }
    XGamepad {
        buttons: XButtons { raw },
        left_trigger: unit_to_u8_round(s.axis(Axis::LeftTrigger)),
        right_trigger: unit_to_u8_round(s.axis(Axis::RightTrigger)),
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

/// `size_of::<T>()` as the `u32` the driver structs carry.
fn size_u32<T>() -> u32 {
    size_of::<T>() as u32
}

/// Run one buffered request on the overlapped bus handle and wait for it. `out` says
/// whether the driver writes its answer back into `req`.
fn bus_request<T>(
    bus: HANDLE,
    event: &OwnedHandle,
    code: u32,
    req: &mut T,
    out: bool,
) -> io::Result<()> {
    let len = size_u32::<T>();
    let ptr: *mut c_void = (req as *mut T).cast();
    let start = |ov| {
        // SAFETY: valid bus handle, request buffer and OVERLAPPED.
        unsafe {
            DeviceIoControl(
                bus,
                code,
                ptr,
                len,
                if out { ptr } else { null_mut() },
                if out { len } else { 0 },
                null_mut(),
                ov,
            )
        }
    };
    // SAFETY: `req` lives until the request has finished.
    unsafe { run_overlapped(bus, event, None, start) }.map(drop)
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
        let event = overlapped::event(EventReset::Auto).map_err(|source| PlatformError::Os {
            context: "CreateEventW",
            source,
        })?;
        let bus = client.as_raw_handle() as HANDLE;
        let mut serial = 0;
        let mut last_err = None;
        for candidate in 1..=MAX_SERIAL {
            let mut req = PluginTarget {
                size: size_u32::<PluginTarget>(),
                serial: candidate,
                target_type: TARGET_TYPE_DUALSHOCK4,
                vendor: SONY_VID,
                product: DS4_V1_PID,
            };
            match bus_request(bus, &event, IOCTL_PLUGIN_TARGET, &mut req, false) {
                Ok(()) => {
                    serial = candidate;
                    break;
                }
                Err(e) => last_err = Some(e),
            }
        }
        if serial == 0 {
            let why = last_err.map(|e| e.to_string()).unwrap_or_default();
            return Err(PlatformError::Other(format!(
                "ViGEmBus: cannot plug in a DualShock 4: {why}"
            )));
        }
        let mut wait = SerialRequest {
            size: size_u32::<SerialRequest>(),
            serial,
        };
        // Buses before 1.17 do not know this request; the pad works without it.
        if let Err(e) = bus_request(bus, &event, IOCTL_WAIT_DEVICE_READY, &mut wait, false) {
            tracing::debug!("ViGEmBus wait-ready failed: {e}");
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
                let Ok(event) = overlapped::event(EventReset::Auto) else {
                    return;
                };
                let handle = thread_bus.as_raw_handle() as HANDLE;
                while !stop.load(Ordering::Relaxed) {
                    let mut n = Ds4Notification {
                        size: size_u32::<Ds4Notification>(),
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
                            f.rumble = Some(Rumble::from_motors_u8(n.large_motor, n.small_motor));
                            f.lightbar = Some(Rgb {
                                r: n.red,
                                g: n.green,
                                b: n.blue,
                            });
                        }
                        // Unplugged, or cancelled by `drop`.
                        Err(e)
                            if matches!(
                                e.raw_os_error().map(|c| c as u32),
                                Some(ERROR_OPERATION_ABORTED | ERROR_ACCESS_DENIED)
                            ) =>
                        {
                            break;
                        }
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
            size: size_u32::<Ds4SubmitReportEx>(),
            serial: self.serial,
            report: self.builder.build(state, elapsed),
        };
        let bus = self.client.as_raw_handle() as HANDLE;
        bus_request(bus, &self.event, IOCTL_DS4_SUBMIT_REPORT, &mut req, false)
            .map_err(|e| PlatformError::Other(format!("ViGEmBus: DualShock 4 update failed: {e}")))
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
            size: size_u32::<SerialRequest>(),
            serial: self.serial,
        };
        let bus = self.client.as_raw_handle() as HANDLE;
        if let Err(e) = bus_request(bus, &self.event, IOCTL_UNPLUG_TARGET, &mut req, false) {
            tracing::warn!("cannot unplug the virtual DualShock 4: {e}");
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
        assert_eq!(size_u32::<PluginTarget>(), 16);
        assert_eq!(size_u32::<SerialRequest>(), 8);
        assert_eq!(size_u32::<Ds4SubmitReportEx>(), 71);
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
