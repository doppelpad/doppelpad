//! The raw-HID backend of Windows and macOS: DualShock 4, DualSense, Nintendo Switch pads,
//! Steam controllers and other HID gamepads (read through their report descriptor). On Linux,
//! where evdev reads the controllers, it runs only Switch 2 controllers over Bluetooth LE.
//!
//! A scanner thread, which each OS provides, re-enumerates HID devices every couple of seconds
//! and starts one thread per supported controller. That thread owns the device: it runs the
//! driver's start-up, reads reports, and sends rumble and keep-alives. `poll` returns the
//! latest parsed state and the setters only hand a value over, so device I/O never stalls the
//! engine loop.
//!
//! Built in tests everywhere so its pure parts are unit-tested on every OS.

#![cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]

use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use dp_core::pad::{PadState, Rgb, Rumble};

use crate::generic::{GenericLayout, RawInput};
use crate::nintendo::{self, Model};
use crate::padio::{InitError, PadDriver, ReportIo};
use crate::scan::{Pass, RESCAN, Rescanner};
use crate::sony::{self, SonyModel};
use crate::steam::{self, SteamModel};
use crate::{Battery, DeviceId, DeviceInfo, GamepadBackend, Leds, PadSnapshot, TriggerEffects};

/// Interfaces that are not controllers we handle are not reopened for this long.
pub(crate) const SKIP_FOREIGN: Duration = Duration::from_secs(30);
/// A controller that did not start (e.g. an empty charging-grip slot) is retried after this.
const RETRY_START: Duration = Duration::from_secs(4);
/// Longest wait between two driver services when nothing happens.
const IDLE_WAIT: Duration = Duration::from_millis(250);

/// HID usage page and usages (HID Usage Tables 1.4).
pub(crate) mod usage {
    pub const PAGE_GENERIC: u16 = 0x01;
    pub const PAGE_VENDOR_DEFINED_BEGIN: u16 = 0xFF00;
    pub const JOYSTICK: u16 = 0x04;
    pub const GAMEPAD: u16 = 0x05;
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum DriverKind {
    Sony(SonyModel),
    Nintendo(Model),
    Steam(SteamModel),
    /// Any other game pad or joystick, read through the HID parser.
    Generic,
}

impl DriverKind {
    /// How a controller of this kind is connected, as shown after its name.
    pub(crate) fn transport(self, bluetooth: bool, pid: u16) -> &'static str {
        match self {
            _ if bluetooth => "Bluetooth",
            DriverKind::Steam(SteamModel::Receiver) => "wireless",
            DriverKind::Steam(SteamModel::Deck) => "built in",
            DriverKind::Nintendo(Model::Switch1) if pid == nintendo::pid::CHARGING_GRIP => {
                "USB grip"
            }
            _ => "USB",
        }
    }
}

/// Which driver handles a HID top-level collection, if any. `xbox_elsewhere`: Xbox pads are
/// read by another backend (XInput on Windows), so they are left out here.
pub(crate) fn driver_kind(
    vid: u16,
    pid: u16,
    usage_page: u16,
    usage: u16,
    xbox_elsewhere: bool,
) -> Option<DriverKind> {
    // Valve pads: the vendor collection; the others are their keyboard and mouse.
    if let Some(m) = steam::model_for(vid, pid) {
        return (usage_page >= usage::PAGE_VENDOR_DEFINED_BEGIN).then_some(DriverKind::Steam(m));
    }
    if usage_page != usage::PAGE_GENERIC {
        return None;
    }
    if let Some(m) = sony::model_for(vid, pid) {
        return (usage == usage::GAMEPAD).then_some(DriverKind::Sony(m));
    }
    // The GameCube adapter has a backend of its own.
    if nintendo::gcadapter::is_adapter(vid, pid) {
        return None;
    }
    let pad = matches!(usage, usage::JOYSTICK | usage::GAMEPAD);
    // Nintendo pads are game pads over Bluetooth; wired ones may report a joystick.
    if let Some(m) = nintendo::model_for(vid, pid) {
        return pad.then_some(DriverKind::Nintendo(m));
    }
    (pad && !(xbox_elsewhere && vid == crate::MICROSOFT_VID)).then_some(DriverKind::Generic)
}

/// A Bluetooth address from text such as `a4:ae:12:34:56:78` or `a4ae12345678` (Sony pads
/// report theirs as their serial number).
pub(crate) fn parse_bt_address(s: &str) -> Option<u64> {
    let hex: String = s.chars().filter(char::is_ascii_hexdigit).collect();
    (hex.len() == 12).then(|| u64::from_str_radix(&hex, 16).ok())?
}

/// What [`HidHandle::wait`] ended with.
pub(crate) enum Wait {
    /// A report of this many bytes is in [`HidHandle::report`].
    Report(usize),
    Woken,
    Timeout,
}

/// Wakes a device thread blocked in [`HidHandle::wait`].
pub(crate) trait Wake: Send + Sync {
    fn wake(&self);
}

/// An open HID device, as its thread uses it.
pub(crate) trait HidHandle: ReportIo + Send {
    /// Wait for the next input report, a wake-up, or the timeout. An error means the device
    /// is gone.
    fn wait(&mut self, timeout: Duration) -> io::Result<Wait>;
    /// The report completed by the last [`Wait::Report`].
    fn report(&self) -> &[u8];
    fn waker(&self) -> Arc<dyn Wake>;
    /// Keep the device to this program alone, or share it again. Where the OS hides devices
    /// this way (macOS), it is how they are hidden; elsewhere it does nothing.
    fn set_exclusive(&mut self, _on: bool) -> io::Result<()> {
        Ok(())
    }
}

/// What a scanner found out about a controller before its driver starts.
pub(crate) struct Found {
    /// The key of the device in the list: the path the scanner found it under.
    pub path: String,
    /// The id the controller is reported under.
    pub id: DeviceId,
    /// How it is connected, shown after its name ("USB", "Bluetooth").
    pub transport: &'static str,
    pub vid: u16,
    pub pid: u16,
    /// What hides it (see [`DeviceInfo::instance_paths`]).
    pub instance_paths: Vec<String>,
    pub bluetooth: bool,
    /// The controller's Bluetooth address, for a pad that is turned off by dropping its link
    /// (Sony pads report it as their serial number).
    pub bt_address: Option<u64>,
}

/// Builds a controller's driver on its thread, where it may talk to the device first.
pub(crate) type MakeDriver<H> = Box<dyn FnOnce(&mut H) -> Box<dyn PadDriver> + Send>;

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
    /// Keep the device to Doppelpad alone (see [`HidHandle::set_exclusive`]).
    exclusive: AtomicBool,
    waker: Arc<dyn Wake>,
    stop: AtomicBool,
    done: AtomicBool,
}

struct Device {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    /// What hides it (see [`DeviceInfo::instance_paths`]).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))] // only macOS hides this way
    instance_paths: Vec<String>,
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

    /// Open the device exclusively while any of its instance paths is in `hidden`.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))] // only macOS hides this way
    fn set_hidden(&self, hidden: &HashSet<String>) {
        let on = self.instance_paths.iter().any(|p| hidden.contains(p));
        if self.shared.exclusive.swap(on, Ordering::Relaxed) != on {
            self.shared.waker.wake();
        }
    }
}

/// The controllers a scanner started, shared by the scanner and the backend.
#[derive(Default)]
pub(crate) struct Devices {
    /// By the path the scanner found them under.
    devices: Mutex<HashMap<String, Device>>,
    /// Devices that cannot be used, by path, as user-facing explanations.
    problems: Mutex<HashMap<String, String>>,
    /// Layouts of generic pads, handed to each device thread.
    layouts: Mutex<Arc<Vec<GenericLayout>>>,
    /// Instance paths of the devices to open exclusively.
    hidden: Mutex<HashSet<String>>,
}

impl Devices {
    /// Whether a device was started from `path`.
    pub(crate) fn has(&self, path: &str) -> bool {
        self.devices.lock().unwrap().contains_key(path)
    }

    /// Note why the device at `path` cannot be used, or (`None`) that nothing is wrong with it.
    pub(crate) fn set_problem(&self, path: &str, problem: Option<String>) {
        let mut problems = self.problems.lock().unwrap();
        match problem {
            Some(p) => problems.insert(path.to_owned(), p),
            None => problems.remove(path),
        };
    }

    /// Forget the devices that are not `present` any more and those whose thread ended; the
    /// ones that did not start are put into `skip` until they may be retried.
    pub(crate) fn sweep(
        &self,
        present: impl Fn(&str) -> bool,
        skip: &mut HashMap<String, Instant>,
        now: Instant,
    ) {
        let mut problems = self.problems.lock().unwrap();
        problems.retain(|path, _| present(path));
        self.devices.lock().unwrap().retain(|path, d| {
            let gone = !present(path);
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
                    skip.insert(path.clone(), now + RETRY_START);
                }
                DevState::Ready { info, .. } => {
                    tracing::info!(name = %info.name, "HID controller disconnected");
                }
                DevState::Starting => {}
            }
            false
        });
    }

    /// Start the controller `found` on a thread of its own, with the driver `make_driver`
    /// builds there.
    pub(crate) fn start<H: HidHandle + 'static>(
        &self,
        io: H,
        found: Found,
        make_driver: MakeDriver<H>,
    ) {
        self.set_problem(&found.path, None);
        let layouts = self.layouts.lock().unwrap().clone();
        let exclusive = {
            let hidden = self.hidden.lock().unwrap();
            found.instance_paths.iter().any(|p| hidden.contains(p))
        };
        let shared = Arc::new(Shared {
            state: Mutex::new(DevState::Starting),
            outputs: Mutex::new(Outputs {
                layouts,
                ..Outputs::default()
            }),
            waker: io.waker(),
            power_off: AtomicBool::new(false),
            exclusive: AtomicBool::new(exclusive),
            stop: AtomicBool::new(false),
            done: AtomicBool::new(false),
        });
        let path = found.path.clone();
        let instance_paths = found.instance_paths.clone();
        let (bluetooth, bt_address) = (found.bluetooth, found.bt_address);
        let thread_shared = shared.clone();
        let thread = std::thread::Builder::new()
            .name("dp-hid-device".into())
            .spawn(move || {
                run_device(io, &found, make_driver, &thread_shared);
                thread_shared.done.store(true, Ordering::Relaxed);
            })
            .expect("spawn HID device thread");
        let device = Device {
            shared,
            thread: Some(thread),
            instance_paths,
            bluetooth,
            bt_address,
        };
        self.devices.lock().unwrap().insert(path, device);
    }

    /// Open exactly the devices with these instance paths exclusively.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))] // only macOS hides this way
    pub(crate) fn set_hidden(&self, instance_paths: &[String]) {
        let mut hidden = self.hidden.lock().unwrap();
        *hidden = instance_paths.iter().cloned().collect();
        for d in self.devices.lock().unwrap().values() {
            d.set_hidden(&hidden);
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

    /// Stop every device thread and wait for each driver to restore its device (report
    /// mode, rumble off).
    fn stop_all(&self) {
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
        for mut d in devices {
            if let Some(t) = d.thread.take() {
                let _ = t.join();
            }
        }
    }
}

/// The device thread: start the driver, then read, parse and service until stopped or
/// the device goes away.
fn run_device<H: HidHandle>(mut io: H, found: &Found, make_driver: MakeDriver<H>, shared: &Shared) {
    let mut exclusive = false;
    sync_exclusive(&mut io, found, shared, &mut exclusive);
    let Some((mut driver, name)) = start(&mut io, found, make_driver, shared) else {
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
                tracing::debug!(path = %found.path, "HID read failed: {e}");
                return; // unplugged: nothing to restore
            }
        }
        sync_exclusive(&mut io, found, shared, &mut exclusive);
        let rumble = apply_outputs(driver.as_mut(), &mut io, found, shared);
        if let Err(e) = driver.service(&mut io, rumble) {
            tracing::debug!(path = %found.path, "HID write failed: {e}");
        }
        if let Some(changed) = driver.take_notice_change()
            && let DevState::Ready { notice, .. } = &mut *shared.state.lock().unwrap()
        {
            *notice = changed.map(|n| format!("{name}: {n}"));
        }
    }
    driver.shutdown(&mut io);
}

/// Open the device exclusively or shared, as the backend wants it now; `now` is how it is open.
fn sync_exclusive<H: HidHandle>(io: &mut H, found: &Found, shared: &Shared, now: &mut bool) {
    let want = shared.exclusive.load(Ordering::Relaxed);
    if want == *now {
        return;
    }
    match io.set_exclusive(want) {
        Ok(()) => *now = want,
        Err(e) => tracing::warn!(path = %found.path, exclusive = want, "cannot reopen: {e}"),
    }
}

/// Start the driver and publish the device as ready: the driver and the device's name, or
/// `None` (with the device marked failed) when it did not start.
fn start<H: HidHandle>(
    io: &mut H,
    found: &Found,
    make_driver: MakeDriver<H>,
    shared: &Shared,
) -> Option<(Box<dyn PadDriver>, String)> {
    let mut driver = make_driver(io);
    let identity = match driver.init(io) {
        Ok(id) => id,
        Err(e) => {
            tracing::info!(path = %found.path, "controller did not start: {e}");
            let problem = match e {
                InitError::Blocked(why) => Some(why),
                _ => None,
            };
            *shared.state.lock().unwrap() = DevState::Failed { problem };
            return None;
        }
    };
    let info = Arc::new(DeviceInfo {
        id: found.id.clone(),
        name: format!("{} ({})", identity.name, found.transport),
        family: identity.family,
        vendor_id: found.vid,
        product_id: found.pid,
        backend: "hid".into(),
        instance_paths: found.instance_paths.clone(),
        joycon: identity.joycon,
        has_motion: identity.has_motion,
        has_touchpad: identity.has_touchpad,
        has_rumble: identity.has_rumble,
        mirror: false,
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
    io: &mut dyn ReportIo,
    found: &Found,
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
            Ok(true) => tracing::info!(path = %found.path, "controller told to turn off"),
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(path = %found.path, "turning the controller off failed: {e}");
            }
        }
    }
    rumble
}

/// Drops the Bluetooth connection to a controller at an address.
pub(crate) type Disconnect = fn(u64) -> io::Result<()>;

/// The backend: the devices a scanner keeps up to date.
pub(crate) struct HidBackend {
    devices: Arc<Devices>,
    /// Woken for an immediate scan that also retries skipped interfaces.
    scanner: Rescanner,
    disconnect: Disconnect,
    /// Runs once the backend stopped (see [`HidBackend::on_drop`]).
    stopped: Option<fn()>,
}

impl HidBackend {
    /// Run `scan` on the scanner thread every couple of seconds and whenever a rescan is
    /// asked for; it finds the controllers and starts them in the [`Devices`] it is given.
    /// `disconnect` turns off Sony pads, which have no off command.
    pub(crate) fn new(
        mut scan: impl FnMut(&Pass, &Devices) + Send + 'static,
        disconnect: Disconnect,
    ) -> Self {
        let devices: Arc<Devices> = Arc::default();
        let scanned = devices.clone();
        let scanner = Rescanner::spawn("dp-hid-scan", RESCAN, move |pass| scan(pass, &scanned));
        HidBackend {
            devices,
            scanner,
            disconnect,
            stopped: None,
        }
    }

    /// Run `stopped` when the backend is dropped, after every driver restored its device.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))] // only Linux has links to let go
    pub(crate) fn on_drop(mut self, stopped: fn()) -> Self {
        self.stopped = Some(stopped);
        self
    }

    /// The devices, for a hider that opens them exclusively.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))] // only macOS hides this way
    pub(crate) fn devices(&self) -> Arc<Devices> {
        self.devices.clone()
    }
}

impl Drop for HidBackend {
    fn drop(&mut self) {
        // No new device may come in once the list is drained.
        self.scanner.stop();
        self.devices.stop_all();
        if let Some(stopped) = self.stopped {
            stopped();
        }
    }
}

impl GamepadBackend for HidBackend {
    fn name(&self) -> &'static str {
        "hid"
    }

    fn poll(&mut self, out: &mut Vec<PadSnapshot>) {
        for d in self.devices.devices.lock().unwrap().values() {
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
        self.devices.hand_over(id, |o| &mut o.rumble, rumble);
    }

    fn set_lightbar(&mut self, id: &DeviceId, color: Option<Rgb>) {
        self.devices.hand_over(id, |o| &mut o.lightbar, color);
    }

    fn set_leds(&mut self, id: &DeviceId, leds: Leds) {
        self.devices.hand_over(id, |o| &mut o.leds, leds);
    }

    fn set_trigger_effects(&mut self, id: &DeviceId, effects: TriggerEffects) {
        self.devices.hand_over(id, |o| &mut o.triggers, effects);
    }

    fn set_layouts(&mut self, layouts: Arc<Vec<GenericLayout>>) {
        *self.devices.layouts.lock().unwrap() = layouts.clone();
        for d in self.devices.devices.lock().unwrap().values() {
            d.shared.outputs.lock().unwrap().layouts = layouts.clone();
            d.shared.waker.wake();
        }
    }

    fn power_off(&mut self, id: &DeviceId) -> bool {
        for d in self.devices.devices.lock().unwrap().values() {
            if !d.is(id) || !d.bluetooth {
                continue;
            }
            // Sony pads, and Switch 2 pads over Bluetooth LE on Linux, have no off command:
            // their Bluetooth link is dropped instead.
            if let Some(addr) = d.bt_address {
                return match (self.disconnect)(addr) {
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
        let mut out: Vec<String> = self
            .devices
            .problems
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect();
        for d in self.devices.devices.lock().unwrap().values() {
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
            driver_kind(0x054C, 0x0CE6, 1, 5, true),
            Some(DriverKind::Sony(SonyModel::DualSense))
        );
        assert_eq!(
            driver_kind(0x054C, 0x0CE6, 1, 4, true),
            None,
            "Sony needs the game pad collection"
        );
        assert_eq!(
            driver_kind(0x057E, 0x2009, 1, 4, true),
            Some(DriverKind::Nintendo(Model::Switch1))
        );
        assert_eq!(
            driver_kind(0x057E, 0x2069, 1, 5, true),
            Some(DriverKind::Nintendo(Model::Switch2(Switch2Model::Pro)))
        );
        assert_eq!(
            driver_kind(0x057E, 0x2009, 0x0C, 1, true),
            None,
            "consumer collection"
        );
        assert_eq!(driver_kind(0x046D, 0xC52B, 1, 6, true), None);
        assert_eq!(
            driver_kind(0x28DE, 0x1102, 0xFF00, 1, true),
            Some(DriverKind::Steam(SteamModel::Controller))
        );
        assert_eq!(
            driver_kind(0x28DE, 0x1102, 1, 2, true),
            None,
            "lizard-mode mouse"
        );
        assert_eq!(
            driver_kind(0x0079, 0x0006, 1, 4, true),
            Some(DriverKind::Generic)
        );
        assert_eq!(
            driver_kind(0x045E, 0x0B13, 1, 5, true),
            None,
            "Xbox: XInput"
        );
        assert_eq!(
            driver_kind(0x045E, 0x0B13, 1, 5, false),
            Some(DriverKind::Generic),
            "Xbox without XInput"
        );
    }

    #[test]
    fn bluetooth_addresses() {
        assert_eq!(
            parse_bt_address("a4:ae:12:34:56:78"),
            Some(0xA4AE_1234_5678)
        );
        assert_eq!(parse_bt_address("A4AE12345678"), Some(0xA4AE_1234_5678));
        assert_eq!(
            parse_bt_address("10-18-49-75-fc-86"),
            Some(0x1018_4975_FC86)
        );
        assert_eq!(parse_bt_address("12345"), None);
    }
}
