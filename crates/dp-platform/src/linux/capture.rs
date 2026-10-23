//! Keyboard / mouse capture by grabbing the devices.
//!
//! Every keyboard and relative mouse is grabbed (`EVIOCGRAB`), so nothing else sees its
//! events any more, and gets its own uinput *passthrough* device with the same
//! capabilities. Each event is reported to the engine; everything the [`CaptureFilter`]
//! does not suppress is re-injected through the passthrough device, frame by frame.
//!
//! Unlike the Windows hooks this works per device and also hides swallowed input from games
//! reading the devices directly. Limitations: keyboard LEDs (Caps/Num Lock) are not mirrored
//! back to grabbed keyboards, and touchpads / tablets (absolute pointers) are not captured.

use std::collections::HashMap;
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use dp_core::keys::Key;
use evdev::uinput::VirtualDevice;
use evdev::{Device, InputEvent, UinputAbsSetup};

use super::codes::*;
use super::dev::{
    Fetch, NodeTracker, check_uinput, event_nodes, fetch, has_abs, has_key, has_rel, is_own,
    own_phys, set_grab, uinput_name,
};
use super::kbmmap::Passthrough;
use crate::scan::{RESCAN, Rescanner};
use crate::{CaptureFilter, KbmCapture, KbmEvent, PlatformError, Result};

/// How long to wait for keys held at capture start to be released before grabbing anyway.
const RELEASE_WAIT: Duration = Duration::from_secs(5);
const PASSTHROUGH_SETTLE: Duration = Duration::from_millis(200);

#[derive(Default)]
pub struct GrabCapture {
    running: Option<Running>,
}

struct Captured {
    alive: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

type Devices = Arc<Mutex<HashMap<PathBuf, Captured>>>;

/// A started capture: its stop flag, its device threads and the scanner adding to them.
struct Running {
    stop: Arc<AtomicBool>,
    devices: Devices,
    scanner: Rescanner,
}

struct Scanner {
    tx: Sender<KbmEvent>,
    filter: Arc<CaptureFilter>,
    stop: Arc<AtomicBool>,
    devices: Devices,
    /// Nodes that are not keyboards or mice.
    nodes: NodeTracker,
}

/// Keyboards (any key the engine knows) and relative mice; not our own devices, gamepads,
/// joysticks, absolute pointers, or ACPI power / sleep buttons (which logind watches).
fn capturable(dev: &Device) -> bool {
    if is_own(dev) {
        return false;
    }
    let absolute = has_abs(dev, ABS_X) || has_abs(dev, ABS_MT_POSITION_X);
    if absolute || has_key(dev, BTN_SOUTH) || has_key(dev, BTN_JOYSTICK) {
        return false;
    }
    let keyboard = dev.supported_keys().is_some_and(|k| {
        k.iter().any(|c| {
            Key::from_evdev(c.code())
                .is_some_and(|key| !matches!(key, Key::Power | Key::Sleep | Key::Wake))
        })
    });
    let mouse = has_key(dev, BTN_LEFT) && has_rel(dev, REL_X) && has_rel(dev, REL_Y);
    keyboard || mouse
}

/// A uinput device with the same capabilities and ids as `dev`.
fn passthrough_for(dev: &Device) -> io::Result<VirtualDevice> {
    let name = uinput_name(&format!(
        "Doppelpad passthrough: {}",
        dev.name().unwrap_or("input")
    ));
    let mut builder = VirtualDevice::builder()?
        .name(&name)
        .input_id(dev.input_id())
        .with_phys(&own_phys("passthrough"))?
        .with_properties(dev.properties())?;
    if let Some(keys) = dev.supported_keys() {
        builder = builder.with_keys(keys)?;
    }
    if let Some(rel) = dev.supported_relative_axes() {
        builder = builder.with_relative_axes(rel)?;
    }
    for (code, info) in dev.get_absinfo()? {
        builder = builder.with_absolute_axis(&UinputAbsSetup::new(code, info))?;
    }
    if let Some(msc) = dev.misc_properties() {
        builder = builder.with_msc(msc)?;
    }
    if let Some(sw) = dev.supported_switches() {
        builder = builder.with_switches(sw)?;
    }
    builder.build()
}

impl Scanner {
    /// Returns how many nodes could not be opened for lack of permission.
    fn scan(&mut self) -> usize {
        let present = event_nodes();
        let now = Instant::now();
        let mut devices = self.devices.lock().unwrap();
        devices.retain(|_, c| c.alive.load(Ordering::Relaxed));
        self.nodes.prune(&present, now);
        let mut denied = 0;
        for path in present {
            if devices.contains_key(&path) || self.nodes.skips(&path) {
                continue;
            }
            let dev = match Device::open(&path) {
                Ok(dev) => dev,
                Err(e) => {
                    if e.kind() == io::ErrorKind::PermissionDenied {
                        denied += 1;
                    }
                    self.nodes.failed(path, now);
                    continue;
                }
            };
            if !capturable(&dev) {
                self.nodes.ignore(path);
                continue;
            }
            let alive = Arc::new(AtomicBool::new(true));
            let (tx, filter, stop) = (self.tx.clone(), self.filter.clone(), self.stop.clone());
            let (p, alive2) = (path.clone(), alive.clone());
            let mut dev = dev;
            match std::thread::Builder::new()
                .name("dp-capture".into())
                .spawn(move || {
                    run_capture(&p, &mut dev, &tx, &filter, &stop);
                    alive2.store(false, Ordering::Relaxed);
                }) {
                Ok(thread) => {
                    devices.insert(path, Captured { alive, thread });
                }
                Err(e) => tracing::warn!("cannot start capture thread: {e}"),
            }
        }
        denied
    }
}

/// Release `held` for everyone else reading `dev`, which is about to be grabbed: they would
/// never see those keys go up, and the desktop would repeat them until something else
/// happened. Written to the device itself, the releases reach every reader; the real ones
/// that follow are dropped by the kernel, since the keys are already up.
fn release_for_others(path: &Path, dev: &mut Device, held: &[u16]) {
    let mut events: Vec<InputEvent> = held
        .iter()
        .map(|&code| InputEvent::new(EV_KEY, code, 0))
        .collect();
    events.push(InputEvent::new(EV_SYN, SYN_REPORT, 0));
    match dev.send_events(&events) {
        Ok(()) => {
            tracing::debug!(path = %path.display(), keys = ?held, "released the keys still held");
        }
        Err(e) => {
            tracing::warn!(path = %path.display(), "keys held while capture starts may stay pressed: {e}");
        }
    }
}

fn run_capture(
    path: &Path,
    dev: &mut Device,
    tx: &Sender<KbmEvent>,
    filter: &CaptureFilter,
    stop: &AtomicBool,
) {
    let mut out = match passthrough_for(dev) {
        Ok(out) => out,
        Err(e) => {
            tracing::warn!(path = %path.display(), "cannot create passthrough device: {e}");
            return;
        }
    };
    // Give the desktop a moment to pick up the passthrough device, and wait until nothing on
    // this device is held: a key held while the grab starts would stay pressed for every
    // other client.
    let settle = Instant::now() + PASSTHROUGH_SETTLE;
    let deadline = Instant::now() + RELEASE_WAIT;
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let held: Vec<u16> = match dev.get_key_state() {
            Ok(keys) => keys.iter().map(|k| k.code()).collect(),
            Err(_) => return,
        };
        let now = Instant::now();
        if now >= settle && held.is_empty() {
            break;
        }
        if now >= deadline {
            release_for_others(path, dev, &held);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let fd = dev.as_raw_fd();
    if let Err(e) = set_grab(fd, true) {
        tracing::warn!(path = %path.display(), "cannot grab input device (another program may hold it): {e}");
        return;
    }
    tracing::info!(path = %path.display(), name = dev.name().unwrap_or(""), "capturing input device");

    let mut state = Passthrough::new(
        has_rel(dev, REL_WHEEL_HI_RES),
        has_rel(dev, REL_HWHEEL_HI_RES),
    );
    let (mut events, mut forward) = (Vec::new(), Vec::new());
    while !stop.load(Ordering::Relaxed) {
        let fetched = fetch(fd, || {
            dev.fetch_events().map(|evs| {
                evs.map(|e| (e.event_type().0, e.code(), e.value()))
                    .collect()
            })
        });
        let batch: Vec<(u16, u16, i32)> = match fetched {
            Fetch::Ready(batch) => batch,
            Fetch::Idle => continue,
            Fetch::Closed(_) => break, // unplugged
        };
        for ev in batch {
            state.event(ev, filter, &mut events, &mut forward);
            for e in events.drain(..) {
                let _ = tx.try_send(e);
            }
            if !forward.is_empty() {
                let frame: Vec<InputEvent> = forward
                    .drain(..)
                    .map(|(ty, code, value)| InputEvent::new(ty, code, value))
                    .collect();
                if let Err(e) = out.emit(&frame) {
                    tracing::trace!("passthrough write failed: {e}");
                }
            }
        }
    }
    let _ = set_grab(fd, false);
    tracing::debug!(path = %path.display(), "released input device");
}

impl KbmCapture for GrabCapture {
    fn start(&mut self, filter: Arc<CaptureFilter>) -> Result<Receiver<KbmEvent>> {
        self.stop();
        check_uinput()?;
        let (tx, rx) = crossbeam_channel::bounded(4096);
        let stop = Arc::new(AtomicBool::new(false));
        let devices = Devices::default();
        let mut scan = Scanner {
            tx,
            filter,
            stop: stop.clone(),
            devices: devices.clone(),
            nodes: NodeTracker::default(),
        };
        let denied = scan.scan();
        if devices.lock().unwrap().is_empty() && denied > 0 {
            return Err(PlatformError::Other(
                "no permission to read /dev/input/event*; add your user to the `input` group \
                 (see docs/LINUX.md)"
                    .into(),
            ));
        }
        let scanner = Rescanner::spawn("dp-capture-scan", RESCAN, move |_| {
            scan.scan();
        });
        self.running = Some(Running {
            stop,
            devices,
            scanner,
        });
        Ok(rx)
    }

    fn stop(&mut self) {
        if let Some(mut r) = self.running.take() {
            r.stop.store(true, Ordering::Relaxed);
            r.scanner.stop();
            for (_, c) in r.devices.lock().unwrap().drain() {
                let _ = c.thread.join();
            }
        }
    }
}

impl Drop for GrabCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
