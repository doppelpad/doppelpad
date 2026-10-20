//! Raw HID controllers XInput cannot see: DualShock 4, DualSense and Nintendo Switch pads.
//!
//! A scanner thread re-enumerates HID interfaces every couple of seconds and starts one
//! thread per supported controller. That thread owns the device: it runs the driver's
//! start-up, reads reports with overlapped I/O, and sends rumble and keep-alives. `poll`
//! returns the latest parsed state and `set_rumble` only hands a value over, so device
//! I/O never stalls the engine loop.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use dp_core::pad::{PadState, Rgb, Rumble};
use windows_sys::Win32::Devices::HumanInterfaceDevice::HidD_GetHidGuid;
use windows_sys::core::GUID;

use super::devnode::{InterfaceEntry, enumerate_interfaces, is_vigem_device};
use super::hidio::{HidIo, Wait, Waker};
use crate::nintendo::switch1::{Link, Switch1Driver};
use crate::nintendo::wired::WiredDriver;
use crate::nintendo::{self, Model};
use crate::padio::{InitError, PadDriver};
use crate::sony::{self, SonyDriver, SonyModel};
use crate::{Battery, DeviceId, DeviceInfo, GamepadBackend, JoyConSide, Leds, PadSnapshot};

const RESCAN: Duration = Duration::from_secs(2);
/// Interfaces that are not controllers we handle are not reopened for this long.
const SKIP_FOREIGN: Duration = Duration::from_secs(30);
/// A controller that did not start (e.g. an empty charging-grip slot) is retried after this.
const RETRY_START: Duration = Duration::from_secs(4);
/// Longest wait between two driver services when nothing happens.
const IDLE_WAIT: Duration = Duration::from_millis(250);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum DriverKind {
    Sony(SonyModel),
    Nintendo(Model),
}

/// Which driver handles a HID top-level collection, if any.
fn driver_kind(vid: u16, pid: u16, usage_page: u16, usage: u16) -> Option<DriverKind> {
    if usage_page != 0x01 {
        return None;
    }
    if let Some(m) = sony::model_for(vid, pid) {
        return (usage == 0x05).then_some(DriverKind::Sony(m));
    }
    // Nintendo pads are game pads over Bluetooth; wired ones may report a joystick.
    nintendo::model_for(vid, pid)
        .filter(|_| matches!(usage, 0x04 | 0x05))
        .map(DriverKind::Nintendo)
}

fn is_bluetooth(entry: &InterfaceEntry) -> bool {
    entry.instance_id.to_ascii_uppercase().contains("BTH")
        || entry
            .path
            .to_ascii_lowercase()
            .contains("00001124-0000-1000-8000-00805f9b34fb")
}

/// Charging-grip slot of a HID interface: interface 1 holds the left Joy-Con.
fn grip_slot(instance_id: &str) -> Option<JoyConSide> {
    let id = instance_id.to_ascii_uppercase();
    if id.contains("&MI_01") {
        Some(JoyConSide::Left)
    } else if id.contains("&MI_00") {
        Some(JoyConSide::Right)
    } else {
        None
    }
}

/// What the scanner knows about a device before its driver starts.
#[derive(Clone, Debug)]
struct Base {
    path: String,
    instance_id: String,
    devinst: u32,
    vid: u16,
    pid: u16,
    bluetooth: bool,
    kind: DriverKind,
    feature_len: usize,
}

impl Base {
    fn transport(&self) -> &'static str {
        match self.kind {
            _ if self.bluetooth => "Bluetooth",
            DriverKind::Nintendo(Model::Switch1) if self.pid == 0x200E => "USB grip",
            _ => "USB",
        }
    }

    fn driver(&self) -> Box<dyn PadDriver> {
        match self.kind {
            DriverKind::Sony(m) => Box::new(SonyDriver::new(m, self.bluetooth, self.feature_len)),
            DriverKind::Nintendo(Model::Switch1) => Box::new(Switch1Driver::new(Link {
                bluetooth: self.bluetooth,
                vid: self.vid,
                pid: self.pid,
                grip_slot: if self.pid == 0x200E {
                    grip_slot(&self.instance_id)
                } else {
                    None
                },
            })),
            DriverKind::Nintendo(Model::Wired) => Box::new(WiredDriver::new(self.vid)),
            DriverKind::Nintendo(Model::Switch2(m)) => super::switch2::driver(m, self.devinst),
        }
    }
}

enum DevState {
    Starting,
    Ready {
        info: Arc<DeviceInfo>,
        pad: Option<PadState>,
        battery: Option<Battery>,
        notice: Option<String>,
    },
    /// The driver could not start; `problem` explains why when the user can act on it.
    Failed {
        problem: Option<String>,
    },
}

struct Shared {
    state: Mutex<DevState>,
    rumble: Mutex<Rumble>,
    lightbar: Mutex<Option<Rgb>>,
    leds: Mutex<Leds>,
    waker: Arc<Waker>,
    stop: AtomicBool,
    done: AtomicBool,
}

struct Device {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Device {
    fn stop(&self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.waker.wake();
    }
}

type DeviceMap = Arc<Mutex<HashMap<String, Device>>>;
type Problems = Arc<Mutex<HashMap<String, String>>>;

pub struct HidBackend {
    devices: DeviceMap,
    problems: Problems,
    stop: Arc<AtomicBool>,
    /// Wakes the scanner for an immediate scan that also retries skipped interfaces.
    rescan: crossbeam_channel::Sender<()>,
}

struct Scanner {
    devices: DeviceMap,
    problems: Problems,
    stop: Arc<AtomicBool>,
    /// Paths not to reopen before the given time.
    skip: HashMap<String, Instant>,
}

impl HidBackend {
    pub fn new() -> Self {
        let devices: DeviceMap = Arc::default();
        let problems: Problems = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (rescan, rescan_rx) = crossbeam_channel::bounded(1);
        let mut scanner = Scanner {
            devices: devices.clone(),
            problems: problems.clone(),
            stop: stop.clone(),
            skip: HashMap::new(),
        };
        std::thread::Builder::new()
            .name("dp-hid-scan".into())
            .spawn(move || {
                while !scanner.stop.load(Ordering::Relaxed) {
                    scanner.scan();
                    if rescan_rx.recv_timeout(RESCAN).is_ok() {
                        scanner.skip.clear();
                    }
                }
            })
            .expect("spawn HID scanner");
        HidBackend {
            devices,
            problems,
            stop,
            rescan,
        }
    }
}

impl HidBackend {
    /// Store an output value for a device's thread and wake it if the value changed.
    fn hand_over<T: PartialEq>(&self, id: &DeviceId, slot: impl Fn(&Shared) -> &Mutex<T>, v: T) {
        for d in self.devices.lock().unwrap().values() {
            let matches = matches!(
                &*d.shared.state.lock().unwrap(),
                DevState::Ready { info, .. } if &info.id == id
            );
            if matches {
                let mut current = slot(&d.shared).lock().unwrap();
                if *current != v {
                    *current = v;
                    d.shared.waker.wake();
                }
                return;
            }
        }
    }
}

impl Drop for HidBackend {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let devices: Vec<Device> = self
            .devices
            .lock()
            .unwrap()
            .drain()
            .map(|(_, d)| d)
            .collect();
        for d in &devices {
            d.stop();
        }
        // Let each driver restore its device (report mode, rumble off) before exit.
        for mut d in devices {
            if let Some(t) = d.thread.take() {
                let _ = t.join();
            }
        }
    }
}

impl Scanner {
    fn scan(&mut self) {
        let mut guid: GUID = unsafe { std::mem::zeroed() };
        // SAFETY: valid out-pointer.
        unsafe { HidD_GetHidGuid(&mut guid) };
        let now = Instant::now();
        self.skip.retain(|_, until| now < *until);
        let present = enumerate_interfaces(&guid);
        let is_present = |path: &str| present.iter().any(|e| e.path == path);

        {
            let mut problems = self.problems.lock().unwrap();
            problems.retain(|path, _| is_present(path));
            self.devices.lock().unwrap().retain(|path, d| {
                let gone = !is_present(path);
                if gone {
                    d.stop();
                }
                if !d.shared.done.load(Ordering::Relaxed) && !gone {
                    return true;
                }
                match &*d.shared.state.lock().unwrap() {
                    DevState::Failed { problem } => {
                        if let Some(p) = problem {
                            problems.insert(path.clone(), p.clone());
                        }
                        self.skip.insert(path.clone(), now + RETRY_START);
                    }
                    DevState::Ready { info, .. } => {
                        tracing::info!(name = %info.name, "HID controller disconnected");
                    }
                    DevState::Starting => {}
                }
                false
            });
        }

        for entry in present {
            if self.stop.load(Ordering::Relaxed) {
                return;
            }
            if self.skip.contains_key(&entry.path)
                || self.devices.lock().unwrap().contains_key(&entry.path)
            {
                continue;
            }
            match identify(&entry) {
                Ok(Some((io, base))) => {
                    self.problems.lock().unwrap().remove(&entry.path);
                    let device = spawn_device(io, base);
                    self.devices.lock().unwrap().insert(entry.path, device);
                }
                Ok(None) => {
                    self.skip.insert(entry.path, now + SKIP_FOREIGN);
                }
                Err(e) => {
                    tracing::debug!(path = %entry.path, "cannot open HID device: {e}");
                    self.skip.insert(entry.path, now + SKIP_FOREIGN);
                }
            }
        }
    }
}

/// Open an interface and decide whether one of our drivers handles it.
fn identify(entry: &InterfaceEntry) -> std::io::Result<Option<(HidIo, Base)>> {
    let io = HidIo::open(&entry.path)?;
    let Some((vid, pid)) = io.ids() else {
        return Ok(None);
    };
    let caps = io.caps();
    let Some(kind) = driver_kind(vid, pid, caps.usage_page, caps.usage) else {
        return Ok(None);
    };
    if is_vigem_device(entry.devinst) {
        return Ok(None); // our own (or someone else's) virtual DS4
    }
    let bluetooth = is_bluetooth(entry);
    if !bluetooth && matches!(kind, DriverKind::Nintendo(_)) && nintendo::usb_unsupported(vid, pid)
    {
        return Ok(None);
    }
    Ok(Some((
        io,
        Base {
            path: entry.path.clone(),
            instance_id: entry.instance_id.clone(),
            devinst: entry.devinst,
            vid,
            pid,
            bluetooth,
            kind,
            feature_len: caps.feature_len,
        },
    )))
}

fn spawn_device(io: HidIo, base: Base) -> Device {
    let shared = Arc::new(Shared {
        state: Mutex::new(DevState::Starting),
        rumble: Mutex::new(Rumble::default()),
        lightbar: Mutex::new(None),
        leds: Mutex::new(Leds::default()),
        waker: io.waker(),
        stop: AtomicBool::new(false),
        done: AtomicBool::new(false),
    });
    let thread_shared = shared.clone();
    let thread = std::thread::Builder::new()
        .name("dp-hid-device".into())
        .spawn(move || {
            run_device(io, &base, &thread_shared);
            thread_shared.done.store(true, Ordering::Relaxed);
        })
        .expect("spawn HID device thread");
    Device {
        shared,
        thread: Some(thread),
    }
}

/// The device thread: start the driver, then read, parse and service until stopped or
/// the device goes away.
fn run_device(mut io: HidIo, base: &Base, shared: &Shared) {
    let mut driver = base.driver();
    let identity = match driver.init(&mut io) {
        Ok(id) => id,
        Err(e) => {
            tracing::info!(path = %base.path, "controller did not start: {e}");
            let problem = match e {
                InitError::Blocked(why) => Some(why),
                _ => None,
            };
            *shared.state.lock().unwrap() = DevState::Failed { problem };
            return;
        }
    };
    let info = Arc::new(DeviceInfo {
        id: DeviceId(format!("hid:{}", base.instance_id)),
        name: format!("{} ({})", identity.name, base.transport()),
        family: identity.family,
        vendor_id: base.vid,
        product_id: base.pid,
        backend: "hid".into(),
        instance_paths: vec![base.instance_id.clone()],
        joycon: identity.joycon,
        has_motion: identity.has_motion,
        has_touchpad: identity.has_touchpad,
        has_rumble: identity.has_rumble,
    });
    tracing::info!(name = %info.name, id = %info.id, "HID controller connected");
    let name = info.name.clone();
    let notice = identity.notice.map(|n| format!("{name}: {n}"));
    *shared.state.lock().unwrap() = DevState::Ready {
        info,
        pad: None,
        battery: None,
        notice,
    };

    while !shared.stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        let timeout = driver
            .next_deadline()
            .map_or(IDLE_WAIT, |d| d.saturating_duration_since(now))
            .min(IDLE_WAIT);
        match io.wait(timeout) {
            Ok(Wait::Report(_)) => {
                tracing::trace!(report = ?io.report(), "HID report");
                if let Some(pad) = driver.parse(io.report(), Instant::now())
                    && let DevState::Ready {
                        pad: p, battery, ..
                    } = &mut *shared.state.lock().unwrap()
                {
                    *p = Some(pad);
                    *battery = driver.battery();
                }
            }
            Ok(Wait::Woken | Wait::Timeout) => {}
            Err(e) => {
                tracing::debug!(path = %base.path, "HID read failed: {e}");
                return; // unplugged: nothing to restore
            }
        }
        let rumble = *shared.rumble.lock().unwrap();
        driver.set_lightbar(*shared.lightbar.lock().unwrap());
        driver.set_leds(*shared.leds.lock().unwrap());
        if let Err(e) = driver.service(&mut io, rumble) {
            tracing::debug!(path = %base.path, "HID write failed: {e}");
        }
        if let Some(changed) = driver.take_notice_change()
            && let DevState::Ready { notice, .. } = &mut *shared.state.lock().unwrap()
        {
            *notice = changed.map(|n| format!("{name}: {n}"));
        }
    }
    driver.shutdown(&mut io);
}

impl GamepadBackend for HidBackend {
    fn name(&self) -> &'static str {
        "hid"
    }

    fn poll(&mut self, out: &mut Vec<PadSnapshot>) {
        for d in self.devices.lock().unwrap().values() {
            if let DevState::Ready {
                info,
                pad: Some(state),
                battery,
                ..
            } = &*d.shared.state.lock().unwrap()
            {
                out.push(PadSnapshot {
                    info: info.clone(),
                    state: *state,
                    battery: *battery,
                });
            }
        }
    }

    fn set_rumble(&mut self, id: &DeviceId, rumble: Rumble) {
        self.hand_over(id, |s| &s.rumble, rumble);
    }

    fn set_lightbar(&mut self, id: &DeviceId, color: Option<Rgb>) {
        self.hand_over(id, |s| &s.lightbar, color);
    }

    fn set_leds(&mut self, id: &DeviceId, leds: Leds) {
        self.hand_over(id, |s| &s.leds, leds);
    }

    fn problems(&self) -> Vec<String> {
        let mut out: Vec<String> = self.problems.lock().unwrap().values().cloned().collect();
        for d in self.devices.lock().unwrap().values() {
            if let DevState::Ready {
                notice: Some(n), ..
            } = &*d.shared.state.lock().unwrap()
            {
                out.push(n.clone());
            }
        }
        out.sort();
        out
    }

    fn rescan(&mut self) {
        let _ = self.rescan.try_send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nintendo::Switch2Model;

    #[test]
    fn driver_kinds() {
        assert_eq!(
            driver_kind(0x054C, 0x0CE6, 1, 5),
            Some(DriverKind::Sony(SonyModel::DualSense))
        );
        assert_eq!(
            driver_kind(0x054C, 0x0CE6, 1, 4),
            None,
            "Sony needs the game pad collection"
        );
        assert_eq!(
            driver_kind(0x057E, 0x2009, 1, 4),
            Some(DriverKind::Nintendo(Model::Switch1))
        );
        assert_eq!(
            driver_kind(0x057E, 0x2069, 1, 5),
            Some(DriverKind::Nintendo(Model::Switch2(Switch2Model::Pro)))
        );
        assert_eq!(
            driver_kind(0x057E, 0x2009, 0x0C, 1),
            None,
            "consumer collection"
        );
        assert_eq!(driver_kind(0x046D, 0xC52B, 1, 6), None);
    }

    #[test]
    fn grip_slots() {
        assert_eq!(
            grip_slot(r"HID\VID_057E&PID_200E&MI_01\8&1A2B"),
            Some(JoyConSide::Left)
        );
        assert_eq!(
            grip_slot(r"HID\VID_057E&PID_200E&MI_00\8&1A2B"),
            Some(JoyConSide::Right)
        );
        assert_eq!(grip_slot(r"HID\VID_057E&PID_2009\8&1A2B"), None);
    }
}
