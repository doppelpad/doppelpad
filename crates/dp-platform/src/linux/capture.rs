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

use std::collections::{HashMap, HashSet};
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use dp_core::keys::Key;
use evdev::uinput::VirtualDevice;
use evdev::{AbsoluteAxisCode, Device, InputEvent, KeyCode, RelativeAxisCode, UinputAbsSetup};

use super::codes::*;
use super::dev::{
    check_uinput, event_nodes, is_own, own_phys, set_grab, uinput_name, wait_readable,
};
use super::kbmmap::Passthrough;
use crate::{CaptureFilter, KbmCapture, KbmEvent, PlatformError, Result};

const RESCAN: Duration = Duration::from_secs(2);
const RETRY_FAILED: Duration = Duration::from_secs(30);
/// How long to wait for keys held at capture start to be released before grabbing anyway.
const RELEASE_WAIT: Duration = Duration::from_secs(5);
const PASSTHROUGH_SETTLE: Duration = Duration::from_millis(200);

#[derive(Default)]
pub struct GrabCapture {
    running: Option<(Arc<AtomicBool>, JoinHandle<()>)>,
}

struct Captured {
    alive: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

struct Scanner {
    tx: Sender<KbmEvent>,
    filter: Arc<CaptureFilter>,
    stop: Arc<AtomicBool>,
    devices: HashMap<PathBuf, Captured>,
    ignored: HashSet<PathBuf>,
    failed: HashMap<PathBuf, Instant>,
}

/// Keyboards (any key the engine knows) and relative mice; not our own devices, gamepads,
/// joysticks, absolute pointers, or ACPI power / sleep buttons (which logind watches).
fn capturable(dev: &Device) -> bool {
    if is_own(dev) {
        return false;
    }
    let keys = dev.supported_keys();
    let has_key = |c| keys.is_some_and(|k| k.contains(KeyCode(c)));
    let absolute = dev.supported_absolute_axes().is_some_and(|a| {
        a.contains(AbsoluteAxisCode(ABS_X)) || a.contains(AbsoluteAxisCode(ABS_MT_POSITION_X))
    });
    if absolute || has_key(BTN_SOUTH) || has_key(BTN_JOYSTICK) {
        return false;
    }
    let keyboard = keys.is_some_and(|k| {
        k.iter().any(|c| {
            Key::from_evdev(c.code())
                .is_some_and(|key| !matches!(key, Key::Power | Key::Sleep | Key::Wake))
        })
    });
    let mouse = has_key(BTN_LEFT)
        && dev.supported_relative_axes().is_some_and(|r| {
            r.contains(RelativeAxisCode(REL_X)) && r.contains(RelativeAxisCode(REL_Y))
        });
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

fn has_rel(dev: &Device, code: u16) -> bool {
    dev.supported_relative_axes()
        .is_some_and(|r| r.contains(RelativeAxisCode(code)))
}

impl Scanner {
    /// Returns how many nodes could not be opened for lack of permission.
    fn scan(&mut self) -> usize {
        let present = event_nodes();
        let now = Instant::now();
        self.devices.retain(|_, c| c.alive.load(Ordering::Relaxed));
        self.ignored.retain(|p| present.contains(p));
        self.failed
            .retain(|p, t| present.contains(p) && now - *t < RETRY_FAILED);
        let mut denied = 0;
        for path in present {
            if self.devices.contains_key(&path)
                || self.ignored.contains(&path)
                || self.failed.contains_key(&path)
            {
                continue;
            }
            let dev = match Device::open(&path) {
                Ok(dev) => dev,
                Err(e) => {
                    if e.kind() == io::ErrorKind::PermissionDenied {
                        denied += 1;
                    }
                    self.failed.insert(path, now);
                    continue;
                }
            };
            if !capturable(&dev) {
                self.ignored.insert(path);
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
                    self.devices.insert(path, Captured { alive, thread });
                }
                Err(e) => tracing::warn!("cannot start capture thread: {e}"),
            }
        }
        denied
    }

    fn run(mut self) {
        while !self.stop.load(Ordering::Relaxed) {
            let until = Instant::now() + RESCAN;
            while Instant::now() < until && !self.stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
            }
            if !self.stop.load(Ordering::Relaxed) {
                self.scan();
            }
        }
        for (_, c) in self.devices.drain() {
            let _ = c.thread.join();
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
        let held = match dev.get_key_state() {
            Ok(keys) => keys.iter().next().is_some(),
            Err(_) => return,
        };
        let now = Instant::now();
        if now >= deadline || (now >= settle && !held) {
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
        match wait_readable(fd, 100) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(_) => break,
        }
        let batch: Vec<(u16, u16, i32)> = match dev.fetch_events() {
            Ok(evs) => evs
                .map(|e| (e.event_type().0, e.code(), e.value()))
                .collect(),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
            Err(_) => break, // unplugged
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
        let mut scanner = Scanner {
            tx,
            filter,
            stop: stop.clone(),
            devices: HashMap::new(),
            ignored: HashSet::new(),
            failed: HashMap::new(),
        };
        let denied = scanner.scan();
        if scanner.devices.is_empty() && denied > 0 {
            return Err(PlatformError::Other(
                "no permission to read /dev/input/event*; add your user to the `input` group \
                 (see docs/LINUX.md)"
                    .into(),
            ));
        }
        let thread = std::thread::Builder::new()
            .name("dp-capture-scan".into())
            .spawn(move || scanner.run())
            .map_err(|e| PlatformError::Other(e.to_string()))?;
        self.running = Some((stop, thread));
        Ok(rx)
    }

    fn stop(&mut self) {
        if let Some((stop, thread)) = self.running.take() {
            stop.store(true, Ordering::Relaxed);
            let _ = thread.join();
        }
    }
}

impl Drop for GrabCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
