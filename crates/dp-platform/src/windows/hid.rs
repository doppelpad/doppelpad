//! Raw HID controllers XInput cannot see: DualShock 4, DualSense, Nintendo Switch pads, Steam
//! controllers and other HID gamepads (read through their report descriptor).
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
use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    GUID_DEVINTERFACE_HID, HID_USAGE_GENERIC_GAMEPAD, HID_USAGE_GENERIC_JOYSTICK,
    HID_USAGE_PAGE_GENERIC, HID_USAGE_PAGE_VENDOR_DEFINED_BEGIN,
};

use super::devnode::{InterfaceEntry, enumerate_interfaces, is_vigem_device};
use super::hidio::{HidIo, Wait, Waker};
use super::hidparse::HidDecoder;
use crate::generic::{GenericDriver, GenericLayout, RawInput, ReportDecoder};
use crate::nintendo::switch1::{Link, Switch1Driver};
use crate::nintendo::wired::WiredDriver;
use crate::nintendo::{self, Model};
use crate::padio::{InitError, PadDriver};
use crate::scan::{Pass, RESCAN, Rescanner};
use crate::sony::{self, SonyDriver, SonyModel};
use crate::steam::{self, SteamDriver, SteamModel};
use crate::{
    Battery, DeviceId, DeviceInfo, GamepadBackend, JoyConSide, Leds, PadSnapshot, TriggerEffects,
};

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
    Steam(SteamModel),
    /// Any other game pad or joystick, read through the HID parser.
    Generic,
}

/// Which driver handles a HID top-level collection, if any.
fn driver_kind(vid: u16, pid: u16, usage_page: u16, usage: u16) -> Option<DriverKind> {
    // Valve pads: the vendor collection; the others are their keyboard and mouse.
    if let Some(m) = steam::model_for(vid, pid) {
        return (usage_page >= HID_USAGE_PAGE_VENDOR_DEFINED_BEGIN).then_some(DriverKind::Steam(m));
    }
    if usage_page != HID_USAGE_PAGE_GENERIC {
        return None;
    }
    if let Some(m) = sony::model_for(vid, pid) {
        return (usage == HID_USAGE_GENERIC_GAMEPAD).then_some(DriverKind::Sony(m));
    }
    // The GameCube adapter has a backend of its own.
    if crate::nintendo::gcadapter::is_adapter(vid, pid) {
        return None;
    }
    let pad = matches!(
        usage,
        HID_USAGE_GENERIC_JOYSTICK | HID_USAGE_GENERIC_GAMEPAD
    );
    // Nintendo pads are game pads over Bluetooth; wired ones may report a joystick.
    if let Some(m) = nintendo::model_for(vid, pid) {
        return pad.then_some(DriverKind::Nintendo(m));
    }
    // Xbox pads are read through XInput.
    (pad && vid != crate::MICROSOFT_VID).then_some(DriverKind::Generic)
}

/// Generic pads whose reports the HID parser cannot read: nothing to report.
struct NoDecoder;

impl ReportDecoder for NoDecoder {
    fn decode(&mut self, _report: &[u8]) -> Option<RawInput> {
        None
    }
}

/// The Bluetooth HID service class, which Bluetooth HID devices carry in their paths.
const BT_HID_SERVICE: &str = "00001124-0000-1000-8000-00805f9b34fb";

fn is_bluetooth(entry: &InterfaceEntry) -> bool {
    entry.instance_id.to_ascii_uppercase().contains("BTH")
        || entry.path.to_ascii_lowercase().contains(BT_HID_SERVICE)
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
    /// The controller's Bluetooth address (Sony pads report it as their serial number).
    bt_address: Option<u64>,
}

impl Base {
    fn transport(&self) -> &'static str {
        match self.kind {
            _ if self.bluetooth => "Bluetooth",
            DriverKind::Steam(SteamModel::Receiver) => "wireless",
            DriverKind::Steam(SteamModel::Deck) => "built in",
            DriverKind::Nintendo(Model::Switch1) if self.pid == nintendo::pid::CHARGING_GRIP => {
                "USB grip"
            }
            _ => "USB",
        }
    }

    /// The driver; `decoder` is the report decoder `identify` built for a generic pad.
    fn driver(&self, io: &HidIo, decoder: Option<HidDecoder>) -> Box<dyn PadDriver> {
        match self.kind {
            DriverKind::Generic => {
                let name = io.product().unwrap_or_else(|| "Game controller".into());
                let decoder: Box<dyn ReportDecoder> = match decoder {
                    Some(d) => Box::new(d),
                    None => Box::new(NoDecoder),
                };
                Box::new(GenericDriver::new(name, self.vid, self.pid, decoder))
            }
            DriverKind::Sony(m) => Box::new(SonyDriver::new(m, self.bluetooth, self.feature_len)),
            DriverKind::Nintendo(Model::Switch1) => Box::new(Switch1Driver::new(Link {
                bluetooth: self.bluetooth,
                vid: self.vid,
                pid: self.pid,
                grip_slot: if self.pid == nintendo::pid::CHARGING_GRIP {
                    grip_slot(&self.instance_id)
                } else {
                    None
                },
            })),
            DriverKind::Nintendo(Model::Wired) => Box::new(WiredDriver::new(self.vid)),
            DriverKind::Nintendo(Model::Switch2(m)) => super::switch2::driver(m, self.devinst),
            DriverKind::Steam(m) => Box::new(SteamDriver::new(m)),
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
        /// Unmapped input of a generic pad.
        raw: Option<Arc<RawInput>>,
    },
    /// The driver could not start; `problem` explains why when the user can act on it.
    Failed {
        problem: Option<String>,
    },
}

/// What the engine wants a device to do, handed to its thread.
#[derive(Default)]
struct Outputs {
    rumble: Rumble,
    lightbar: Option<Rgb>,
    leds: Leds,
    triggers: TriggerEffects,
    /// Layouts of generic pads.
    layouts: Arc<Vec<GenericLayout>>,
}

struct Shared {
    state: Mutex<DevState>,
    outputs: Mutex<Outputs>,
    /// Turn the controller off at the next chance.
    power_off: AtomicBool,
    waker: Arc<Waker>,
    stop: AtomicBool,
    done: AtomicBool,
}

struct Device {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    bluetooth: bool,
    bt_address: Option<u64>,
}

impl Device {
    /// Whether this is the started device `id`.
    fn is(&self, id: &DeviceId) -> bool {
        matches!(
            &*self.shared.state.lock().unwrap(),
            DevState::Ready { info, .. } if &info.id == id
        )
    }

    fn stop(&self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.waker.wake();
    }
}

type DeviceMap = Arc<Mutex<HashMap<String, Device>>>;
type Problems = Arc<Mutex<HashMap<String, String>>>;
/// Layouts of generic pads, handed to each device thread.
type Layouts = Arc<Mutex<Arc<Vec<GenericLayout>>>>;

pub struct HidBackend {
    devices: DeviceMap,
    problems: Problems,
    layouts: Layouts,
    /// Woken for an immediate scan that also retries skipped interfaces.
    scanner: Rescanner,
}

struct Scanner {
    devices: DeviceMap,
    problems: Problems,
    layouts: Layouts,
    /// Paths not to reopen before the given time.
    skip: HashMap<String, Instant>,
}

impl HidBackend {
    pub fn new() -> Self {
        let devices: DeviceMap = Arc::default();
        let problems: Problems = Arc::default();
        let layouts: Layouts = Arc::default();
        let mut scan = Scanner {
            devices: devices.clone(),
            problems: problems.clone(),
            layouts: layouts.clone(),
            skip: HashMap::new(),
        };
        let scanner = Rescanner::spawn("dp-hid-scan", RESCAN, move |pass| scan.scan(pass));
        HidBackend {
            devices,
            problems,
            layouts,
            scanner,
        }
    }

    /// Store an output value for a device's thread and wake it if the value changed.
    fn hand_over<T: PartialEq>(
        &self,
        id: &DeviceId,
        slot: impl FnOnce(&mut Outputs) -> &mut T,
        v: T,
    ) {
        let devices = self.devices.lock().unwrap();
        let Some(d) = devices.values().find(|d| d.is(id)) else {
            return;
        };
        let mut outputs = d.shared.outputs.lock().unwrap();
        let current = slot(&mut outputs);
        if *current != v {
            *current = v;
            d.shared.waker.wake();
        }
    }
}

impl Drop for HidBackend {
    fn drop(&mut self) {
        // No new device may come in once the list is drained.
        self.scanner.stop();
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
    fn scan(&mut self, pass: &Pass) {
        if pass.requested {
            self.skip.clear();
        }
        let now = Instant::now();
        self.skip.retain(|_, until| now < *until);
        let present = enumerate_interfaces(&GUID_DEVINTERFACE_HID);
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
            if pass.stopping() {
                return;
            }
            if self.skip.contains_key(&entry.path)
                || self.devices.lock().unwrap().contains_key(&entry.path)
            {
                continue;
            }
            match identify(&entry) {
                Ok(Some((io, base, decoder))) => {
                    self.problems.lock().unwrap().remove(&entry.path);
                    let layouts = self.layouts.lock().unwrap().clone();
                    let device = spawn_device(io, base, decoder, layouts);
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

/// Open an interface and decide whether one of our drivers handles it (with the report
/// decoder of a generic pad).
fn identify(entry: &InterfaceEntry) -> std::io::Result<Option<(HidIo, Base, Option<HidDecoder>)>> {
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
    // XInput reads pads that also speak XInput (`IG_` in the path); the parser must be
    // able to read the others.
    let decoder = match kind {
        DriverKind::Generic if entry.path.to_ascii_uppercase().contains("&IG_") => return Ok(None),
        DriverKind::Generic => match io.decoder() {
            Some(d) => Some(d),
            None => return Ok(None),
        },
        _ => None,
    };
    let bluetooth = is_bluetooth(entry);
    if !bluetooth && matches!(kind, DriverKind::Nintendo(_)) && nintendo::usb_unsupported(vid, pid)
    {
        return Ok(None);
    }
    let bt_address = (bluetooth && matches!(kind, DriverKind::Sony(_)))
        .then(|| {
            io.serial()
                .and_then(|s| super::bluetooth::parse_address(&s))
        })
        .flatten();
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
            bt_address,
        },
        decoder,
    )))
}

fn spawn_device(
    io: HidIo,
    base: Base,
    decoder: Option<HidDecoder>,
    layouts: Arc<Vec<GenericLayout>>,
) -> Device {
    let (bluetooth, bt_address) = (base.bluetooth, base.bt_address);
    let shared = Arc::new(Shared {
        state: Mutex::new(DevState::Starting),
        outputs: Mutex::new(Outputs {
            layouts,
            ..Outputs::default()
        }),
        waker: io.waker(),
        power_off: AtomicBool::new(false),
        stop: AtomicBool::new(false),
        done: AtomicBool::new(false),
    });
    let thread_shared = shared.clone();
    let thread = std::thread::Builder::new()
        .name("dp-hid-device".into())
        .spawn(move || {
            run_device(io, &base, decoder, &thread_shared);
            thread_shared.done.store(true, Ordering::Relaxed);
        })
        .expect("spawn HID device thread");
    Device {
        shared,
        thread: Some(thread),
        bluetooth,
        bt_address,
    }
}

/// The device thread: start the driver, then read, parse and service until stopped or
/// the device goes away.
fn run_device(mut io: HidIo, base: &Base, decoder: Option<HidDecoder>, shared: &Shared) {
    let Some((mut driver, name)) = start(&mut io, base, decoder, shared) else {
        return;
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
                let parsed = driver.parse(io.report(), Instant::now());
                publish(shared, driver.as_ref(), parsed);
            }
            Ok(Wait::Woken | Wait::Timeout) => {}
            Err(e) => {
                tracing::debug!(path = %base.path, "HID read failed: {e}");
                return; // unplugged: nothing to restore
            }
        }
        let rumble = apply_outputs(driver.as_mut(), &mut io, base, shared);
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

/// Start the driver and publish the device as ready: the driver and the device's name, or
/// `None` (with the device marked failed) when it did not start.
fn start(
    io: &mut HidIo,
    base: &Base,
    decoder: Option<HidDecoder>,
    shared: &Shared,
) -> Option<(Box<dyn PadDriver>, String)> {
    let mut driver = base.driver(io, decoder);
    let identity = match driver.init(io) {
        Ok(id) => id,
        Err(e) => {
            tracing::info!(path = %base.path, "controller did not start: {e}");
            let problem = match e {
                InitError::Blocked(why) => Some(why),
                _ => None,
            };
            *shared.state.lock().unwrap() = DevState::Failed { problem };
            return None;
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
        raw: None,
    };
    Some((driver, name))
}

/// Hand a parsed report over to `poll`; an empty receiver slot reports nothing.
fn publish(shared: &Shared, driver: &dyn PadDriver, parsed: Option<PadState>) {
    let connected = driver.connected();
    let mut state = shared.state.lock().unwrap();
    let DevState::Ready {
        pad, battery, raw, ..
    } = &mut *state
    else {
        return;
    };
    if !connected {
        *pad = None;
        *battery = None;
    } else if let Some(p) = parsed {
        *pad = Some(p);
        *battery = driver.battery();
        *raw = driver.raw();
    }
}

/// Give the driver what the engine wants shown and turn the pad off if asked; the rumble
/// to service it with.
fn apply_outputs(
    driver: &mut dyn PadDriver,
    io: &mut HidIo,
    base: &Base,
    shared: &Shared,
) -> Rumble {
    let rumble = {
        let o = shared.outputs.lock().unwrap();
        driver.set_lightbar(o.lightbar);
        driver.set_leds(o.leds);
        driver.set_trigger_effects(o.triggers);
        driver.set_layouts(&o.layouts);
        o.rumble
    };
    if shared.power_off.swap(false, Ordering::Relaxed) {
        match driver.power_off(io) {
            Ok(true) => tracing::info!(path = %base.path, "controller told to turn off"),
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(path = %base.path, "turning the controller off failed: {e}");
            }
        }
    }
    rumble
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
                raw,
                ..
            } = &*d.shared.state.lock().unwrap()
            {
                out.push(PadSnapshot {
                    info: info.clone(),
                    state: *state,
                    battery: *battery,
                    raw: raw.clone(),
                });
            }
        }
    }

    fn set_rumble(&mut self, id: &DeviceId, rumble: Rumble) {
        self.hand_over(id, |o| &mut o.rumble, rumble);
    }

    fn set_lightbar(&mut self, id: &DeviceId, color: Option<Rgb>) {
        self.hand_over(id, |o| &mut o.lightbar, color);
    }

    fn set_leds(&mut self, id: &DeviceId, leds: Leds) {
        self.hand_over(id, |o| &mut o.leds, leds);
    }

    fn set_trigger_effects(&mut self, id: &DeviceId, effects: TriggerEffects) {
        self.hand_over(id, |o| &mut o.triggers, effects);
    }

    fn set_layouts(&mut self, layouts: Arc<Vec<GenericLayout>>) {
        *self.layouts.lock().unwrap() = layouts.clone();
        for d in self.devices.lock().unwrap().values() {
            d.shared.outputs.lock().unwrap().layouts = layouts.clone();
            d.shared.waker.wake();
        }
    }

    fn power_off(&mut self, id: &DeviceId) -> bool {
        for d in self.devices.lock().unwrap().values() {
            if !d.is(id) || !d.bluetooth {
                continue;
            }
            // Sony pads have no off command: their Bluetooth link is dropped instead.
            if let Some(addr) = d.bt_address {
                return match super::bluetooth::disconnect(addr) {
                    Ok(()) => true,
                    Err(e) => {
                        tracing::warn!("Bluetooth disconnect failed: {e}");
                        false
                    }
                };
            }
            d.shared.power_off.store(true, Ordering::Relaxed);
            d.shared.waker.wake();
            return true;
        }
        false
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
        self.scanner.wake();
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
        assert_eq!(
            driver_kind(0x28DE, 0x1102, 0xFF00, 1),
            Some(DriverKind::Steam(SteamModel::Controller))
        );
        assert_eq!(driver_kind(0x28DE, 0x1102, 1, 2), None, "lizard-mode mouse");
        assert_eq!(driver_kind(0x0079, 0x0006, 1, 4), Some(DriverKind::Generic));
        assert_eq!(driver_kind(0x045E, 0x0B13, 1, 5), None, "Xbox: XInput");
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
