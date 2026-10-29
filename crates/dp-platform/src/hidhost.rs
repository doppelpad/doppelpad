//! The raw-HID backend of Windows and macOS: DualShock 4, DualSense, Nintendo Switch pads,
//! Steam controllers and other HID gamepads (read through their report descriptor). On Linux,
//! where evdev reads the controllers, it runs only Switch 2 controllers (on USB and over
//! Bluetooth LE), which no kernel driver starts.
//!
//! A scanner thread, which each OS provides, re-enumerates HID devices every couple of seconds
//! and starts one thread per supported controller. That thread owns the device: it runs the
//! driver's start-up, reads reports, and sends rumble and keep-alives. `poll` returns the
//! latest parsed state and the setters only hand a value over, so device I/O never stalls the
//! engine loop.
//!
//! Built in tests everywhere so its pure parts are unit-tested on every OS.

use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use dp_core::pad::{PadState, Rgb, Rumble};

use crate::bdaddr::BdAddr;
use crate::generic::{GenericDriver, GenericLayout, RawInput, ReportDecoder};
use crate::nintendo::switch1::{Link, Switch1Driver};
use crate::nintendo::wired::WiredDriver;
use crate::nintendo::{self, Model};
use crate::padio::{InitError, PadDriver, ReportIo};
use crate::scan::{Pass, RESCAN, Rescanner};
use crate::sony::{self, SonyDriver, SonyModel};
use crate::steam::{self, SteamDriver, SteamModel};
use crate::switch2gatt::Poll;
use crate::switch2usb::{self, Locate};
use crate::{
    Battery, DeviceId, DeviceInfo, GamepadBackend, JoyConSide, Leds, PadSnapshot, Transport,
    TriggerEffects, VirtualIgnoreList,
};

/// Interfaces that are not controllers we handle are not reopened for this long.
pub(crate) const SKIP_FOREIGN: Duration = Duration::from_secs(30);
/// A controller that did not start (e.g. an empty charging-grip slot) is retried after this.
const RETRY_START: Duration = Duration::from_secs(4);
/// Longest wait between two driver services when nothing happens.
const IDLE_WAIT: Duration = Duration::from_millis(250);
/// A device that could not be opened exclusively (or shared again) is tried again after this,
/// not on every pass of its thread.
const RETRY_EXCLUSIVE: Duration = Duration::from_secs(5);

/// Lock a mutex of the backend even after a device thread panicked while holding it: a driver
/// that fails must not take the engine loop down with it.
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// HID usage page and usages (HID Usage Tables 1.4).
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) mod usage {
    pub const PAGE_GENERIC: u16 = 0x01;
    pub const PAGE_VENDOR_DEFINED_BEGIN: u16 = 0xFF00;
    pub const JOYSTICK: u16 = 0x04;
    pub const GAMEPAD: u16 = 0x05;
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) enum DriverKind {
    Sony(SonyModel),
    Nintendo(Model),
    Steam(SteamModel),
    /// Any other game pad or joystick, read through the HID parser.
    Generic,
}

#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
impl DriverKind {
    /// How a controller of this kind is connected, as shown after its name.
    pub(crate) fn label(self, transport: Transport, pid: u16) -> &'static str {
        match self {
            _ if transport == Transport::Bluetooth => "Bluetooth",
            DriverKind::Steam(SteamModel::Receiver) => "wireless",
            DriverKind::Steam(SteamModel::Deck) => "built in",
            DriverKind::Nintendo(Model::Switch1) if pid == nintendo::pid::CHARGING_GRIP => {
                "USB grip"
            }
            _ => "USB",
        }
    }
}

/// Which backend reads Xbox pads.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) enum XboxPads {
    /// Another one (XInput on Windows): this one leaves them out.
    Elsewhere,
    /// This one, as generic pads (macOS).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Here,
}

/// Which driver handles a HID top-level collection, if any.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) fn driver_kind(
    vid: u16,
    pid: u16,
    usage_page: u16,
    usage: u16,
    xbox: XboxPads,
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
    let elsewhere = xbox == XboxPads::Elsewhere && vid == crate::MICROSOFT_VID;
    (pad && !elsewhere).then_some(DriverKind::Generic)
}

/// The driver a scanner picked for a controller: one of ours by its kind, or a generic pad
/// with the decoder that reads its reports.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) enum Driver {
    Sony(SonyModel),
    Nintendo(Model),
    Steam(SteamModel),
    Generic(Box<dyn ReportDecoder>),
}

#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
impl Driver {
    /// The driver for `kind`. A generic pad needs the decoder `decoder` makes: without one
    /// (its reports cannot be read) there is no driver.
    pub(crate) fn of(
        kind: DriverKind,
        decoder: impl FnOnce() -> Option<Box<dyn ReportDecoder>>,
    ) -> Option<Driver> {
        Some(match kind {
            DriverKind::Sony(m) => Driver::Sony(m),
            DriverKind::Nintendo(m) => Driver::Nintendo(m),
            DriverKind::Steam(m) => Driver::Steam(m),
            DriverKind::Generic => Driver::Generic(decoder()?),
        })
    }
}

/// What a controller's driver is made from, besides its kind, as its scanner found it.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) struct Facts {
    pub vid: u16,
    pub pid: u16,
    pub transport: Transport,
    /// The name the device reports, which a generic pad is listed under.
    pub product: Option<String>,
    /// Its longest feature report (Sony pads).
    pub feature_len: usize,
    /// In a charging grip, which Joy-Con the interface holds.
    pub grip_slot: Option<JoyConSide>,
}

/// Build a controller's driver; `switch2` tells how the OS finds a Switch 2 controller's
/// command channel on USB.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) fn make_driver(
    driver: Driver,
    facts: Facts,
    switch2: impl FnOnce() -> Locate,
) -> Box<dyn PadDriver> {
    let Facts {
        vid,
        pid,
        transport,
        ..
    } = facts;
    match driver {
        Driver::Generic(decoder) => {
            let name = facts.product.unwrap_or_else(|| "Game controller".into());
            Box::new(GenericDriver::new(name, vid, pid, decoder))
        }
        Driver::Sony(m) => Box::new(SonyDriver::new(m, transport, facts.feature_len)),
        Driver::Nintendo(Model::Switch1) => Box::new(Switch1Driver::new(Link {
            transport,
            vid,
            pid,
            grip_slot: facts
                .grip_slot
                .filter(|_| pid == nintendo::pid::CHARGING_GRIP),
        })),
        Driver::Nintendo(Model::Wired) => Box::new(WiredDriver::new(vid)),
        Driver::Nintendo(Model::Switch2(m)) => switch2usb::driver(m, switch2()),
        Driver::Steam(m) => Box::new(SteamDriver::new(m)),
    }
}

/// What a scanner's look at a device it has not started made of it.
pub(crate) enum Look {
    /// A controller of ours: started.
    Started,
    /// Not a controller of ours, or one that cannot be opened (with why, when the user can act
    /// on it): not looked at again for [`SKIP_FOREIGN`].
    Skip(Option<String>),
}

/// One pass of a scanner, alike on every OS: let go of what is gone, start the Bluetooth LE
/// controllers that are ready (`wireless`), then look at every device `present` lists (by
/// path) that is neither started nor skipped. `skip` holds the paths not to look at again
/// before a time; a pass asked for retries them.
pub(crate) fn scan_pass<E>(
    pass: &Pass,
    devices: &Devices,
    skip: &mut HashMap<String, Instant>,
    present: Vec<(String, E)>,
    wireless: Poll,
    mut look: impl FnMut(&str, E) -> Look,
) {
    let now = Instant::now();
    if pass.requested {
        skip.clear();
    }
    skip.retain(|_, until| now < *until);
    devices.sweep(
        |path| present.iter().any(|(p, _)| p == path) || wireless.has(path),
        skip,
        now,
    );
    wireless.start(devices);
    for (path, entry) in present {
        if pass.stopping() {
            return;
        }
        if skip.contains_key(&path) || devices.has(&path) {
            continue;
        }
        if let Look::Skip(problem) = look(&path, entry) {
            devices.set_problem(&path, problem);
            skip.insert(path, now + SKIP_FOREIGN);
        }
    }
}

/// [`ReportIo::read`] on top of [`HidHandle::wait`]: the next report into `buf`, waiting up to
/// `timeout`; wake-ups only matter to the device loop, so the wait goes on through them.
pub(crate) fn read_via_wait<H: HidHandle + ?Sized>(
    io: &mut H,
    buf: &mut [u8],
    timeout: Duration,
) -> io::Result<Option<usize>> {
    let until = Instant::now() + timeout;
    loop {
        match io.wait(until.saturating_duration_since(Instant::now()))? {
            Wait::Report => {
                let report = io.report();
                let n = report.len().min(buf.len());
                buf[..n].copy_from_slice(&report[..n]);
                return Ok(Some(n));
            }
            Wait::Woken if Instant::now() < until => {}
            Wait::Woken | Wait::Timeout => return Ok(None),
        }
    }
}

/// What [`HidHandle::wait`] ended with.
pub(crate) enum Wait {
    /// A report is in [`HidHandle::report`].
    Report,
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
    pub transport: Transport,
    /// How it is connected, as shown after its name ("USB", "Bluetooth", "USB grip").
    pub label: &'static str,
    pub vid: u16,
    pub pid: u16,
    /// What hides it (see [`DeviceInfo::instance_paths`]).
    pub instance_paths: Vec<String>,
    /// The controller's Bluetooth address, for a pad that is turned off by dropping its link
    /// (Sony pads report it as their serial number). The others are turned off by their
    /// driver, where it can ([`PadDriver::can_power_off`]).
    pub bt_address: Option<BdAddr>,
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
    /// Whether the driver can turn the controller off now ([`PadDriver::can_power_off`]).
    can_power_off: AtomicBool,
    /// Keep the device to Doppelpad alone (see [`HidHandle::set_exclusive`]).
    exclusive: AtomicBool,
    /// Why the device could not be opened as [`Shared::exclusive`] asks, while it cannot.
    exclusive_failed: Mutex<Option<ExclusiveFailed>>,
    waker: Arc<dyn Wake>,
    stop: AtomicBool,
    /// The device thread ended, however it ended (see [`Ended`]).
    done: AtomicBool,
}

impl Shared {
    fn new(waker: Arc<dyn Wake>, layouts: Arc<Vec<GenericLayout>>, exclusive: bool) -> Shared {
        Shared {
            state: Mutex::new(DevState::Starting),
            outputs: Mutex::new(Outputs {
                layouts,
                ..Outputs::default()
            }),
            power_off: AtomicBool::new(false),
            can_power_off: AtomicBool::new(false),
            exclusive: AtomicBool::new(exclusive),
            exclusive_failed: Mutex::new(None),
            waker,
            stop: AtomicBool::new(false),
            done: AtomicBool::new(false),
        }
    }

    /// What to tell the user about the device, if anything: its driver's notice, and why it
    /// cannot be hidden.
    fn problems(&self, out: &mut Vec<String>) {
        if self.done.load(Ordering::Acquire) {
            return;
        }
        let failed = lock(&self.exclusive_failed).clone();
        let state = lock(&self.state);
        let name = match &*state {
            DevState::Ready { info, notice, .. } => {
                out.extend(notice.iter().cloned());
                info.name.as_str()
            }
            _ => "a controller",
        };
        if let Some(f) = failed {
            out.push(f.explain(name));
        }
    }
}

/// A device that could not be opened exclusively, or shared again.
#[derive(Clone, Debug)]
struct ExclusiveFailed {
    /// What was asked for: exclusive (hidden) or shared.
    exclusive: bool,
    error: String,
}

impl ExclusiveFailed {
    fn explain(&self, name: &str) -> String {
        if self.exclusive {
            format!("{name} cannot be hidden from games: {}", self.error)
        } else {
            format!("{name} cannot be shown to games again: {}", self.error)
        }
    }
}

/// Marks the end of a device thread however it ends (unplugged, a read error, a driver that
/// panicked): its last state goes, so the engine does not keep its buttons held and its sticks
/// deflected until the next sweep.
struct Ended<'a>(&'a Shared);

impl Drop for Ended<'_> {
    fn drop(&mut self) {
        if let DevState::Ready {
            pad, battery, raw, ..
        } = &mut *lock(&self.0.state)
        {
            *pad = None;
            *battery = None;
            *raw = None;
        }
        self.0.done.store(true, Ordering::Release);
    }
}

struct Device {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    /// What hides it (see [`DeviceInfo::instance_paths`]).
    #[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
    // Windows hides with HidHide
    instance_paths: Vec<String>,
    /// Its link is dropped to turn it off (see [`Found::bt_address`]).
    bt_address: Option<BdAddr>,
}

impl Device {
    /// Whether this is the started device `id`.
    fn is(&self, id: &DeviceId) -> bool {
        matches!(
            &*lock(&self.shared.state),
            DevState::Ready { info, .. } if &info.id == id
        )
    }

    fn stop(&self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.waker.wake();
    }

    /// Stop the thread and wait until its driver has restored the device.
    fn stop_and_join(mut self) {
        self.stop();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    /// Open the device exclusively while any of its instance paths is in `hidden`.
    #[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))] // Windows hides with HidHide
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
        lock(&self.devices).contains_key(path)
    }

    /// Note why the device at `path` cannot be used, or (`None`) that nothing is wrong with it.
    pub(crate) fn set_problem(&self, path: &str, problem: Option<String>) {
        let mut problems = lock(&self.problems);
        match problem {
            Some(p) => problems.insert(path.to_owned(), p),
            None => problems.remove(path),
        };
    }

    /// The started device `id`'s shared state.
    fn find(&self, id: &DeviceId) -> Option<Arc<Shared>> {
        lock(&self.devices)
            .values()
            .find(|d| d.is(id))
            .map(|d| d.shared.clone())
    }

    /// Forget the devices that are not `present` any more and those whose thread ended; the
    /// ones that did not start are put into `skip` until they may be retried.
    pub(crate) fn sweep(
        &self,
        present: impl Fn(&str) -> bool,
        skip: &mut HashMap<String, Instant>,
        now: Instant,
    ) {
        lock(&self.problems).retain(|path, _| present(path));
        let mut failed = Vec::new();
        lock(&self.devices).retain(|path, d| {
            let gone = !present(path);
            if gone {
                d.stop();
            }
            if !d.shared.done.load(Ordering::Acquire) && !gone {
                return true;
            }
            match &*lock(&d.shared.state) {
                DevState::Failed { problem } => {
                    if let Some(p) = problem {
                        failed.push((path.clone(), p.clone()));
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
        lock(&self.problems).extend(failed);
    }

    /// Start the controller `found` on a thread of its own, with the driver `make_driver`
    /// builds there. A device still listed under its path (the link of a Bluetooth controller
    /// that came back before its old thread ended) is stopped first.
    pub(crate) fn start<H: HidHandle + 'static>(
        &self,
        io: H,
        found: Found,
        make_driver: MakeDriver<H>,
    ) {
        let path = found.path.clone();
        self.set_problem(&path, None);
        let old = lock(&self.devices).remove(&path);
        if let Some(old) = old {
            tracing::debug!(%path, "a device is still listed there; stopping it first");
            old.stop_and_join();
        }
        let layouts = lock(&self.layouts).clone();
        // Held until the device is listed, so that `set_hidden` cannot pass it by.
        let hidden = lock(&self.hidden);
        let exclusive = found.instance_paths.iter().any(|p| hidden.contains(p));
        let shared = Arc::new(Shared::new(io.waker(), layouts, exclusive));
        let (instance_paths, bt_address) = (found.instance_paths.clone(), found.bt_address);
        let thread_shared = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("dp-hid-device".into())
            .spawn(move || {
                let _ended = Ended(&thread_shared);
                run_device(io, &found, make_driver, &thread_shared);
            });
        let thread = match spawned {
            Ok(t) => t,
            Err(e) => {
                // Tried again with the next scan, as the path is not listed.
                tracing::warn!(%path, "cannot start a thread for the controller: {e}");
                drop(hidden);
                self.set_problem(&path, Some(format!("a controller cannot be started: {e}")));
                return;
            }
        };
        let device = Device {
            shared,
            thread: Some(thread),
            instance_paths,
            bt_address,
        };
        lock(&self.devices).insert(path, device);
    }

    /// Open exactly the devices with these instance paths exclusively.
    #[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))] // Windows hides with HidHide
    pub(crate) fn set_hidden(&self, instance_paths: &[String]) {
        // Held while the devices are told, as `start` holds it until a new one is listed.
        let mut hidden = lock(&self.hidden);
        *hidden = instance_paths.iter().cloned().collect();
        for d in lock(&self.devices).values() {
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
        let Some(shared) = self.find(id) else {
            return;
        };
        let changed = {
            let mut outputs = lock(&shared.outputs);
            let current = slot(&mut outputs);
            let changed = *current != v;
            if changed {
                *current = v;
            }
            changed
        };
        if changed {
            shared.waker.wake();
        }
    }

    /// Stop every device thread and wait for each driver to restore its device (report
    /// mode, rumble off).
    fn stop_all(&self) {
        let devices: Vec<Device> = lock(&self.devices).drain().map(|(_, d)| d).collect();
        for d in &devices {
            d.stop();
        }
        for d in devices {
            d.stop_and_join();
        }
    }
}

/// The device thread: start the driver, then read, parse and service until stopped or
/// the device goes away.
fn run_device<H: HidHandle>(mut io: H, found: &Found, make_driver: MakeDriver<H>, shared: &Shared) {
    let mut exclusive = Exclusive::default();
    exclusive.sync(&mut io, found, shared, Instant::now());
    let Some((mut driver, name)) = start(&mut io, found, make_driver, shared) else {
        return;
    };
    shared
        .can_power_off
        .store(driver.can_power_off(), Ordering::Relaxed);
    while !shared.stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        let timeout = driver
            .next_deadline()
            .map_or(IDLE_WAIT, |d| d.saturating_duration_since(now))
            .min(IDLE_WAIT);
        match io.wait(timeout) {
            Ok(Wait::Report) => {
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
        exclusive.sync(&mut io, found, shared, Instant::now());
        let rumble = apply_outputs(driver.as_mut(), &mut io, found, shared);
        if let Err(e) = driver.service(&mut io, rumble) {
            tracing::debug!(path = %found.path, "HID write failed: {e}");
        }
        shared
            .can_power_off
            .store(driver.can_power_off(), Ordering::Relaxed);
        if let Some(changed) = driver.take_notice_change()
            && let DevState::Ready { notice, .. } = &mut *lock(&shared.state)
        {
            *notice = changed.map(|n| format!("{name}: {n}"));
        }
    }
    driver.shutdown(&mut io);
}

/// How a device thread keeps the device open as [`Shared::exclusive`] asks.
#[derive(Default)]
struct Exclusive {
    /// Whether it is open exclusively now.
    on: bool,
    /// The last attempt that failed: what it asked for, and when.
    failed: Option<(bool, Instant)>,
}

impl Exclusive {
    /// Open the device exclusively or shared, as the backend wants it now. One that cannot be
    /// is tried again after [`RETRY_EXCLUSIVE`], and the user is told why meanwhile.
    fn sync<H: HidHandle>(&mut self, io: &mut H, found: &Found, shared: &Shared, now: Instant) {
        let want = shared.exclusive.load(Ordering::Relaxed);
        if want == self.on {
            if self.failed.take().is_some() {
                *lock(&shared.exclusive_failed) = None;
            }
            return;
        }
        let again = self.failed.is_some_and(|(asked, _)| asked == want);
        if self
            .failed
            .is_some_and(|(asked, at)| asked == want && now < at + RETRY_EXCLUSIVE)
        {
            return;
        }
        match io.set_exclusive(want) {
            Ok(()) => {
                self.on = want;
                if self.failed.take().is_some() {
                    tracing::info!(path = %found.path, exclusive = want, "reopened after all");
                    *lock(&shared.exclusive_failed) = None;
                }
            }
            Err(e) => {
                if again {
                    tracing::debug!(path = %found.path, exclusive = want, "cannot reopen: {e}");
                } else {
                    tracing::warn!(path = %found.path, exclusive = want, "cannot reopen: {e}");
                }
                self.failed = Some((want, now));
                *lock(&shared.exclusive_failed) = Some(ExclusiveFailed {
                    exclusive: want,
                    error: e.to_string(),
                });
            }
        }
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
            *lock(&shared.state) = DevState::Failed { problem };
            return None;
        }
    };
    let info = Arc::new(DeviceInfo {
        id: found.id.clone(),
        name: format!("{} ({})", identity.name, found.label),
        transport: found.transport,
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
    *lock(&shared.state) = DevState::Ready {
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
    // Asked before the lock: a driver that panics must not poison it.
    let connected = driver.connected();
    let update = match parsed {
        Some(p) if connected => Some((p, driver.battery(), driver.raw())),
        _ => None,
    };
    let mut state = lock(&shared.state);
    let DevState::Ready {
        pad, battery, raw, ..
    } = &mut *state
    else {
        return;
    };
    if !connected {
        *pad = None;
        *battery = None;
    } else if let Some((p, b, r)) = update {
        *pad = Some(p);
        *battery = b;
        *raw = r;
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
    // Copied out first: the driver is not called under the lock.
    let (rumble, lightbar, leds, triggers, layouts) = {
        let o = lock(&shared.outputs);
        (o.rumble, o.lightbar, o.leds, o.triggers, o.layouts.clone())
    };
    driver.set_lightbar(lightbar);
    driver.set_leds(leds);
    driver.set_trigger_effects(triggers);
    driver.set_layouts(&layouts);
    if shared.power_off.swap(false, Ordering::Relaxed) {
        match driver.power_off(io) {
            Ok(()) => tracing::info!(path = %found.path, "controller told to turn off"),
            Err(e) => {
                tracing::warn!(path = %found.path, "turning the controller off failed: {e}");
            }
        }
    }
    rumble
}

/// Drops the Bluetooth connection to a controller at an address.
pub(crate) type Disconnect = fn(BdAddr) -> io::Result<()>;

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

    /// Run `stopped` when the backend is dropped, after every driver restored its device: the
    /// Bluetooth LE client lets its controllers go there, as nothing else would end their links
    /// before Doppelpad ends.
    pub(crate) fn on_drop(mut self, stopped: fn()) -> Self {
        self.stopped = Some(stopped);
        self
    }

    /// The devices, for a hider that opens them exclusively.
    #[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))] // Windows hides with HidHide
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
        for d in lock(&self.devices.devices).values() {
            if let DevState::Ready {
                info,
                pad: Some(state),
                battery,
                raw,
                ..
            } = &*lock(&d.shared.state)
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
        *lock(&self.devices.layouts) = layouts.clone();
        for d in lock(&self.devices.devices).values() {
            lock(&d.shared.outputs).layouts = layouts.clone();
            d.shared.waker.wake();
        }
    }

    /// Sony pads, and Switch 2 pads over Bluetooth LE on Linux, have no off command: their
    /// Bluetooth link is dropped. The others are turned off by their driver, whatever their
    /// transport (a Steam Controller through its USB receiver), where it can.
    fn power_off(&mut self, id: &DeviceId) -> bool {
        let (shared, address) = {
            let devices = lock(&self.devices.devices);
            let Some(d) = devices.values().find(|d| d.is(id)) else {
                return false;
            };
            (d.shared.clone(), d.bt_address)
        };
        if let Some(address) = address {
            return match (self.disconnect)(address) {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!("Bluetooth disconnect failed: {e}");
                    false
                }
            };
        }
        if !shared.can_power_off.load(Ordering::Relaxed) {
            return false;
        }
        shared.power_off.store(true, Ordering::Relaxed);
        shared.waker.wake();
        true
    }

    fn problems(&self) -> Vec<String> {
        let mut out: Vec<String> = lock(&self.devices.problems).values().cloned().collect();
        for d in lock(&self.devices.devices).values() {
            d.shared.problems(&mut out);
        }
        out.sort();
        out
    }

    fn rescan(&mut self) {
        self.scanner.wake();
    }

    // Each scanner leaves virtual controllers out itself (on Windows by their device node).
    fn ignore_virtual(&mut self, _ignored: &VirtualIgnoreList) {}
}

#[cfg(all(test, any(windows, target_os = "linux", target_os = "macos")))]
pub(crate) mod testing {
    use std::time::{Duration, Instant};

    use dp_core::pad::{Axis, PadButton, Rumble};

    use super::{HidHandle, Wait};
    use crate::padio::PadDriver;

    /// A hardware test of a controller and its driver, without the rest of Doppelpad: starts
    /// the driver (player light 1 comes on), rumbles in the first second and then reports
    /// second by second what arrives (sticks and triggers as their largest deflection), until
    /// the device goes away or 30 s after its input stopped. `tick` runs once a second, as the
    /// scanner would.
    pub(crate) fn drive(
        id: &str,
        mut driver: Box<dyn PadDriver>,
        io: &mut impl HidHandle,
        mut tick: impl FnMut(),
    ) {
        let started = Instant::now();
        let identity = driver.init(io).expect("the driver starts");
        println!(
            "{id}: {} started in {} ms; player light 1 should be on",
            identity.name,
            started.elapsed().as_millis()
        );

        const AXES: [Axis; 6] = [
            Axis::LeftX,
            Axis::LeftY,
            Axis::RightX,
            Axis::RightY,
            Axis::LeftTrigger,
            Axis::RightTrigger,
        ];
        let mut last_report = Instant::now();
        for second in 1.. {
            tick();
            let rumble = if second == 1 {
                Rumble {
                    low: 0.6,
                    high: 0.6,
                }
            } else {
                Rumble::default()
            };
            let end = Instant::now() + Duration::from_secs(1);
            let (mut reports, mut motion, mut pressed) = (0u32, false, Vec::new());
            let mut peak = [0.0f32; 6];
            while Instant::now() < end {
                match io.wait(Duration::from_millis(20)) {
                    Ok(Wait::Report) => {
                        reports += 1;
                        last_report = Instant::now();
                        if let Some(s) = driver.parse(io.report(), last_report) {
                            motion |= s.motion.is_some();
                            for b in PadButton::ALL.iter().filter(|b| s.pressed(**b)) {
                                if !pressed.contains(&b.name()) {
                                    pressed.push(b.name());
                                }
                            }
                            for (p, a) in peak.iter_mut().zip(AXES) {
                                if s.axis(a).abs() > p.abs() {
                                    *p = s.axis(a);
                                }
                            }
                        }
                    }
                    Ok(Wait::Woken | Wait::Timeout) => {}
                    Err(e) => {
                        println!(
                            "second {second}: the controller was let go, {:.1} s after its \
                             last report ({e})",
                            last_report.elapsed().as_secs_f32()
                        );
                        return;
                    }
                }
                if let Err(e) = driver.service(io, rumble) {
                    println!("second {second}: rumble failed: {e}");
                }
            }
            println!(
                "second {second}: {reports} reports, motion {motion}, battery {:?}, \
                 sticks {:+.2?}, triggers {:.2?}, pressed {pressed:?}",
                driver.battery().map(|b| b.level),
                &peak[..4],
                &peak[4..],
            );
            if last_report.elapsed() > Duration::from_secs(30) {
                println!("the controller was not let go 30 s after its input stopped");
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nintendo::Switch2Model;

    #[test]
    fn driver_kinds() {
        assert_eq!(
            driver_kind(0x054C, 0x0CE6, 1, 5, XboxPads::Elsewhere),
            Some(DriverKind::Sony(SonyModel::DualSense))
        );
        assert_eq!(
            driver_kind(0x054C, 0x0CE6, 1, 4, XboxPads::Elsewhere),
            None,
            "Sony needs the game pad collection"
        );
        assert_eq!(
            driver_kind(0x057E, 0x2009, 1, 4, XboxPads::Elsewhere),
            Some(DriverKind::Nintendo(Model::Switch1))
        );
        assert_eq!(
            driver_kind(0x057E, 0x2069, 1, 5, XboxPads::Elsewhere),
            Some(DriverKind::Nintendo(Model::Switch2(Switch2Model::Pro)))
        );
        assert_eq!(
            driver_kind(0x057E, 0x2009, 0x0C, 1, XboxPads::Elsewhere),
            None,
            "consumer collection"
        );
        assert_eq!(driver_kind(0x046D, 0xC52B, 1, 6, XboxPads::Elsewhere), None);
        assert_eq!(
            driver_kind(0x28DE, 0x1102, 0xFF00, 1, XboxPads::Elsewhere),
            Some(DriverKind::Steam(SteamModel::Controller))
        );
        assert_eq!(
            driver_kind(0x28DE, 0x1102, 1, 2, XboxPads::Elsewhere),
            None,
            "lizard-mode mouse"
        );
        assert_eq!(
            driver_kind(0x0079, 0x0006, 1, 4, XboxPads::Elsewhere),
            Some(DriverKind::Generic)
        );
        assert_eq!(
            driver_kind(0x045E, 0x0B13, 1, 5, XboxPads::Elsewhere),
            None,
            "Xbox: XInput"
        );
        assert_eq!(
            driver_kind(0x045E, 0x0B13, 1, 5, XboxPads::Here),
            Some(DriverKind::Generic),
            "Xbox without XInput"
        );
    }
}

/// The device lifecycle, with a fake device and driver on real device threads.
#[cfg(test)]
mod lifecycle {
    use std::sync::atomic::AtomicU64;

    use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
    use dp_core::pad::PadButton;

    use super::*;
    use crate::PadFamily;
    use crate::padio::Identity;

    /// Long enough for a device thread to react, short enough for a test that fails.
    const SOON: Duration = Duration::from_secs(5);

    enum Event {
        Report(Vec<u8>),
        Wake,
        /// The device is unplugged.
        Gone,
    }

    /// What a fake device and its driver saw.
    #[derive(Default)]
    struct Log {
        /// Driver calls, in order.
        calls: Vec<&'static str>,
        /// The rumble the driver was last serviced with.
        rumble: Option<Rumble>,
        /// What the device was asked to be: exclusive or shared.
        exclusive: Vec<bool>,
    }

    type Seen = Arc<Mutex<Log>>;

    struct FakeIo {
        events: Receiver<Event>,
        waker: Arc<FakeWaker>,
        report: Vec<u8>,
        seen: Seen,
        exclusive_fails: bool,
    }

    struct FakeWaker(Sender<Event>);

    impl Wake for FakeWaker {
        fn wake(&self) {
            let _ = self.0.send(Event::Wake);
        }
    }

    impl ReportIo for FakeIo {
        fn write(&mut self, _report: &[u8]) -> io::Result<()> {
            Ok(())
        }

        fn read(&mut self, _buf: &mut [u8], _timeout: Duration) -> io::Result<Option<usize>> {
            Ok(None)
        }
    }

    impl HidHandle for FakeIo {
        fn wait(&mut self, timeout: Duration) -> io::Result<Wait> {
            match self.events.recv_timeout(timeout) {
                Ok(Event::Report(r)) => {
                    self.report = r;
                    Ok(Wait::Report)
                }
                Ok(Event::Wake) => Ok(Wait::Woken),
                Err(RecvTimeoutError::Timeout) => Ok(Wait::Timeout),
                Ok(Event::Gone) | Err(RecvTimeoutError::Disconnected) => {
                    Err(io::ErrorKind::NotConnected.into())
                }
            }
        }

        fn report(&self) -> &[u8] {
            &self.report
        }

        fn waker(&self) -> Arc<dyn Wake> {
            self.waker.clone()
        }

        fn set_exclusive(&mut self, on: bool) -> io::Result<()> {
            lock(&self.seen).exclusive.push(on);
            if self.exclusive_fails {
                Err(io::Error::other("another program seized it"))
            } else {
                Ok(())
            }
        }
    }

    /// A pad whose reports are `[id, buttons]`: bit 0 presses A, and `0xFF` is a driver bug.
    struct FakeDriver {
        seen: Seen,
        can_power_off: bool,
    }

    impl FakeDriver {
        fn log(&self, call: &'static str) {
            lock(&self.seen).calls.push(call);
        }
    }

    impl PadDriver for FakeDriver {
        fn init(&mut self, _io: &mut dyn ReportIo) -> Result<Identity, InitError> {
            self.log("init");
            Ok(Identity::new("Fake pad", PadFamily::Generic))
        }

        fn parse(&mut self, report: &[u8], _now: Instant) -> Option<PadState> {
            let buttons = *report.get(1)?;
            assert_ne!(buttons, 0xFF, "a driver bug");
            let mut s = PadState::default();
            s.set(PadButton::A, buttons & 1 != 0);
            Some(s)
        }

        fn service(&mut self, _io: &mut dyn ReportIo, rumble: Rumble) -> io::Result<()> {
            lock(&self.seen).rumble = Some(rumble);
            Ok(())
        }

        fn can_power_off(&self) -> bool {
            self.can_power_off
        }

        fn power_off(&mut self, _io: &mut dyn ReportIo) -> io::Result<()> {
            self.log("power_off");
            Ok(())
        }

        fn next_deadline(&self) -> Option<Instant> {
            None
        }

        fn shutdown(&mut self, _io: &mut dyn ReportIo) {
            self.log("shutdown");
        }
    }

    /// A fake controller: what the test sends it, and what it saw.
    struct Pad {
        events: Sender<Event>,
        seen: Seen,
    }

    impl Pad {
        fn calls(&self) -> Vec<&'static str> {
            lock(&self.seen).calls.clone()
        }
    }

    fn found(path: &str, bt_address: Option<BdAddr>) -> Found {
        Found {
            path: path.into(),
            id: DeviceId(format!("id-{path}")),
            transport: Transport::Usb,
            label: "USB",
            vid: 1,
            pid: 2,
            instance_paths: vec![format!("hide-{path}")],
            bt_address,
        }
    }

    /// Start a fake controller in `devices`.
    fn plug(devices: &Devices, found: Found, can_power_off: bool, exclusive_fails: bool) -> Pad {
        let (events, rx) = crossbeam_channel::unbounded();
        let seen = Seen::default();
        let io = FakeIo {
            events: rx,
            waker: Arc::new(FakeWaker(events.clone())),
            report: Vec::new(),
            seen: seen.clone(),
            exclusive_fails,
        };
        let driver = FakeDriver {
            seen: seen.clone(),
            can_power_off,
        };
        devices.start(io, found, Box::new(move |_| Box::new(driver)));
        Pad { events, seen }
    }

    /// Wait until `ok` holds.
    fn eventually(what: &str, mut ok: impl FnMut() -> bool) {
        let until = Instant::now() + SOON;
        while !ok() {
            assert!(Instant::now() < until, "never: {what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    static DISCONNECTED: AtomicU64 = AtomicU64::new(0);

    fn disconnect(address: BdAddr) -> io::Result<()> {
        DISCONNECTED.store(address.get(), Ordering::SeqCst);
        Ok(())
    }

    /// A backend whose scanner finds nothing: the tests start the devices.
    fn backend() -> HidBackend {
        HidBackend::new(|_, _| {}, disconnect)
    }

    /// Whether A is held, for each device `poll` reports.
    fn held(b: &mut HidBackend) -> Vec<bool> {
        let mut out = Vec::new();
        b.poll(&mut out);
        out.iter().map(|s| s.state.pressed(PadButton::A)).collect()
    }

    /// Send `pad` a report, and wait until `poll` reports `n` devices.
    fn report(b: &mut HidBackend, pad: &Pad, buttons: u8, n: usize) {
        pad.events.send(Event::Report(vec![0, buttons])).unwrap();
        eventually("the report arrives", || held(b).len() == n);
    }

    #[test]
    fn a_device_that_fails_is_no_longer_reported() {
        let mut b = backend();
        let pad = plug(&b.devices, found("a", None), false, false);
        report(&mut b, &pad, 1, 1);
        eventually("A is held", || held(&mut b) == [true]);
        pad.events.send(Event::Gone).unwrap();
        eventually("its state goes with its thread", || held(&mut b).is_empty());
        assert!(b.devices.has("a"), "until the next sweep");
    }

    #[test]
    fn a_driver_that_panics_is_no_longer_reported() {
        let mut b = backend();
        let pad = plug(&b.devices, found("a", None), false, false);
        report(&mut b, &pad, 1, 1);
        eventually("A is held", || held(&mut b) == [true]);
        pad.events.send(Event::Report(vec![0, 0xFF])).unwrap();
        eventually("its state goes with its thread", || held(&mut b).is_empty());
        assert!(b.problems().is_empty());
    }

    #[test]
    fn power_off_asks_the_driver_whatever_the_transport() {
        let mut b = backend();
        // A Steam Controller's receiver is on USB, and turns its controller off.
        let receiver = plug(&b.devices, found("receiver", None), true, false);
        let plain = plug(&b.devices, found("plain", None), false, false);
        // A Sony pad over Bluetooth: its link is dropped.
        let sony = plug(
            &b.devices,
            found("sony", BdAddr::new(0xA4AE_1234_5678)),
            false,
            false,
        );
        report(&mut b, &receiver, 0, 1);
        report(&mut b, &plain, 0, 2);
        report(&mut b, &sony, 0, 3);

        assert!(b.power_off(&DeviceId("id-receiver".into())));
        eventually("its driver turns it off", || {
            receiver.calls().contains(&"power_off")
        });
        assert!(!b.power_off(&DeviceId("id-plain".into())), "no off command");
        assert!(b.power_off(&DeviceId("id-sony".into())));
        assert_eq!(DISCONNECTED.load(Ordering::SeqCst), 0xA4AE_1234_5678);
        assert!(!sony.calls().contains(&"power_off"));
        assert!(!b.power_off(&DeviceId("nobody".into())));
    }

    #[test]
    fn a_device_that_cannot_be_hidden_is_not_tried_on_every_pass() {
        let mut b = backend();
        let pad = plug(&b.devices, found("a", None), false, true);
        report(&mut b, &pad, 0, 1);
        b.devices.set_hidden(&["hide-a".into()]);
        eventually("tried", || !lock(&pad.seen).exclusive.is_empty());
        for _ in 0..20 {
            pad.events.send(Event::Wake).unwrap();
        }
        eventually("the passes ran", || pad.events.is_empty());
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(lock(&pad.seen).exclusive, [true], "tried once");
        let problems = b.problems();
        assert_eq!(
            problems,
            ["Fake pad (USB) cannot be hidden from games: another program seized it"]
        );

        b.devices.set_hidden(&[]);
        eventually("no problem once it is not to be hidden", || {
            b.problems().is_empty()
        });
        assert_eq!(lock(&pad.seen).exclusive, [true], "nothing to undo");
    }

    #[test]
    fn outputs_reach_the_driver_and_gone_devices_are_stopped() {
        let mut b = backend();
        let pad = plug(&b.devices, found("a", None), false, false);
        report(&mut b, &pad, 0, 1);
        let rumble = Rumble {
            low: 0.5,
            high: 0.25,
        };
        b.set_rumble(&DeviceId("id-a".into()), rumble);
        eventually("the rumble is handed over", || {
            lock(&pad.seen).rumble == Some(rumble)
        });

        // The scanner does not find it any more.
        let mut skip = HashMap::new();
        b.devices.sweep(|_| false, &mut skip, Instant::now());
        assert!(!b.devices.has("a"));
        eventually("its driver restores it", || {
            pad.calls() == ["init", "shutdown"]
        });
    }

    #[test]
    fn a_device_started_at_the_path_of_another_replaces_it() {
        let mut b = backend();
        let old = plug(&b.devices, found("ble:x", None), false, false);
        report(&mut b, &old, 1, 1);
        let new = plug(&b.devices, found("ble:x", None), false, false);
        assert_eq!(old.calls(), ["init", "shutdown"], "stopped first");
        new.events.send(Event::Report(vec![0, 0])).unwrap();
        eventually("only the new one", || held(&mut b) == [false]);
    }

    #[test]
    fn dropping_the_backend_restores_every_device() {
        let mut b = backend();
        let pads: Vec<Pad> = ["a", "b"]
            .iter()
            .map(|p| plug(&b.devices, found(p, None), false, false))
            .collect();
        for (i, pad) in pads.iter().enumerate() {
            report(&mut b, pad, 0, i + 1);
        }
        drop(b);
        for pad in &pads {
            assert_eq!(pad.calls(), ["init", "shutdown"]);
        }
    }
}
