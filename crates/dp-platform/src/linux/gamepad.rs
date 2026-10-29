//! Physical gamepads through evdev, and hiding them with `EVIOCGRAB`.
//!
//! A scanner thread looks for new `/dev/input/event*` nodes every couple of seconds, so
//! discovery never stalls the engine loop. Each node of a controller gets a reader thread
//! that keeps the latest state; `poll` only copies it. hid-playstation (DualShock 4 /
//! DualSense) and hid-nintendo put motion sensors and the touchpad on separate nodes; those
//! are merged into the controller that shares their parent device.
//!
//! Every node also keeps a second `evdev::Device` on a duplicate of the reader's file
//! descriptor. Grabs and force-feedback effects belong to the open file, so grabbing through
//! that handle hides the controller from everyone else while our reader keeps receiving
//! events, and rumble written through it still reaches a grabbed device.
//!
//! What evdev has no interface for goes through the controller's HID device in sysfs: the
//! battery its driver reports and the lights it registers as LEDs. A Bluetooth controller is
//! turned off by ending its connection through BlueZ.

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::os::fd::{AsFd, AsRawFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dp_core::pad::{Motion, PadButton, PadState, Rgb, Rumble, TouchPoint, unit_to_u16_trunc};
use evdev::{
    Device, FFEffect, FFEffectCode, FFEffectData, FFEffectKind, FFReplay, FFTrigger, InputEvent,
    PropType,
};

use super::codes::*;
use super::dev::{
    Fetch, NodeTracker, abs_axes, event_nodes, fetch, has_abs, has_key, is_own, led_names,
    parent_device, read_battery, set_grab, sysfs_has_key, sysfs_name, write_leds,
};
use super::padmap::{
    AbsRange, MotionMapper, NINTENDO_VID, PadMapper, TouchMapper, absinfo_is_state, family_for,
    joycon_side, stable_id,
};
use super::sysfs::{LedWrite, bluez_device_path, lightbar_writes, player_writes};
use crate::scan::{Pass, RESCAN, Rescanner};
use crate::{
    Battery, DeviceHider, DeviceId, DeviceInfo, GamepadBackend, Leds, PadFamily, PadSnapshot,
    PlatformError, Result,
};

/// The latest state of one controller, written by its node readers.
#[derive(Copy, Clone, Debug, Default)]
struct Parts {
    pad: PadState,
    motion: Option<Motion>,
    touch: [Option<TouchPoint>; 2],
    click: bool,
}

impl Parts {
    fn compose(&self) -> PadState {
        let mut s = self.pad;
        s.motion = self.motion;
        s.touch = self.touch;
        if self.click {
            s.set(PadButton::Touchpad, true);
        }
        s
    }
}

enum Mapper {
    Pad(PadMapper),
    Motion(MotionMapper),
    Touch(TouchMapper),
}

impl Mapper {
    fn event(&mut self, ty: u16, code: u16, value: i32) {
        match self {
            Mapper::Pad(m) => m.event(ty, code, value),
            Mapper::Motion(m) => m.event(ty, code, value),
            Mapper::Touch(m) => m.event(ty, code, value),
        }
    }

    fn publish(&self, parts: &mut Parts) {
        match self {
            Mapper::Pad(m) => parts.pad = m.state(),
            Mapper::Motion(m) => parts.motion = Some(m.motion()),
            Mapper::Touch(m) => {
                parts.touch = m.touch();
                parts.click = m.click();
            }
        }
    }
}

/// One open event node of a controller.
struct Node {
    path: PathBuf,
    /// Same open file as the reader's `Device` (see the module docs).
    control: Device,
    grabbed: bool,
    alive: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl Drop for Node {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Node {
    /// Start a reader thread for `dev`, seeded with its current state.
    fn start(
        path: PathBuf,
        dev: Device,
        mut mapper: Mapper,
        parts: Arc<Mutex<Parts>>,
    ) -> io::Result<Node> {
        let control = Device::from_fd(dev.as_fd().try_clone_to_owned()?)?;
        for (code, _, value) in abs_axes(&dev) {
            if absinfo_is_state(code) {
                mapper.event(EV_ABS, code, value);
            }
        }
        if let Ok(keys) = dev.get_key_state() {
            for k in keys.iter() {
                mapper.event(EV_KEY, k.code(), 1);
            }
        }
        mapper.publish(&mut parts.lock().unwrap());

        let alive = Arc::new(AtomicBool::new(true));
        let stop = Arc::new(AtomicBool::new(false));
        let (alive2, stop2) = (alive.clone(), stop.clone());
        std::thread::Builder::new()
            .name("dp-evdev-reader".into())
            .spawn(move || read_loop(dev, mapper, parts, stop2, alive2))?;
        Ok(Node {
            path,
            control,
            grabbed: false,
            alive,
            stop,
        })
    }

    fn set_grab(&mut self, grab: bool) -> Result<()> {
        if self.grabbed == grab || !self.alive.load(Ordering::Relaxed) {
            return Ok(());
        }
        set_grab(self.control.as_raw_fd(), grab).map_err(|source| PlatformError::Os {
            context: "EVIOCGRAB (another program may hold the device)",
            source,
        })?;
        self.grabbed = grab;
        Ok(())
    }
}

fn read_loop(
    mut dev: Device,
    mut mapper: Mapper,
    parts: Arc<Mutex<Parts>>,
    stop: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
) {
    let fd = dev.as_raw_fd();
    'outer: while !stop.load(Ordering::Relaxed) {
        let events: Vec<InputEvent> = match fetch(fd, || dev.fetch_events().map(Iterator::collect))
        {
            Fetch::Ready(events) => events,
            Fetch::Idle => continue,
            Fetch::Closed(_) => break, // ENODEV: unplugged
        };
        for ev in events {
            let (ty, code, value) = (ev.event_type().0, ev.code(), ev.value());
            if ty == EV_SYN && code == SYN_REPORT {
                let Ok(mut p) = parts.lock() else {
                    break 'outer;
                };
                mapper.publish(&mut p);
            } else {
                mapper.event(ty, code, value);
            }
        }
    }
    alive.store(false, Ordering::Relaxed);
}

/// One physical controller: its gamepad node plus any motion / touchpad nodes.
struct Pad {
    info: Arc<DeviceInfo>,
    parent: Option<PathBuf>,
    uniq: Option<String>,
    parts: Arc<Mutex<Parts>>,
    /// `nodes[0]` is the gamepad node.
    nodes: Vec<Node>,
    rumble: Option<FFEffect>,
    rumble_playing: bool,
    last_rumble: Rumble,
    /// As the driver reports it under `parent`, read on every scan.
    battery: Option<Battery>,
    /// The Bluetooth address while it is connected over Bluetooth.
    bluetooth: Option<String>,
    /// The light bar colour wanted, the one written last and when.
    lightbar: Option<Rgb>,
    lightbar_written: Option<Rgb>,
    lightbar_at: Option<Instant>,
    /// The player number set last.
    player: Option<u8>,
    /// Why its lights cannot be set, for [`GamepadBackend::problems`]. They are not tried
    /// again until the controller reconnects.
    lights_problem: Option<String>,
    /// When it connected, for [`LIGHTS_SETTLE`].
    opened: Instant,
}

impl Pad {
    fn owns(&self, parent: Option<&Path>, uniq: Option<&str>, vendor: u16, product: u16) -> bool {
        match (&self.parent, parent) {
            (Some(a), Some(b)) => a == b,
            // Without sysfs, fall back to the MAC address hid-playstation puts in `uniq`.
            _ => {
                uniq.is_some_and(|u| !u.is_empty())
                    && self.uniq.as_deref() == uniq
                    && self.info.vendor_id == vendor
                    && self.info.product_id == product
            }
        }
    }

    fn set_rumble(&mut self, rumble: Rumble) -> io::Result<()> {
        let strong = unit_to_u16_trunc(rumble.low);
        let weak = unit_to_u16_trunc(rumble.high);
        let data = FFEffectData {
            direction: 0,
            trigger: FFTrigger::default(),
            // Length 0 plays until stopped.
            replay: FFReplay {
                length: 0,
                delay: 0,
            },
            kind: FFEffectKind::Rumble {
                strong_magnitude: strong,
                weak_magnitude: weak,
            },
        };
        let off = strong == 0 && weak == 0;
        let effect = match &mut self.rumble {
            Some(e) => {
                e.update(data)?;
                e
            }
            None if off => return Ok(()),
            None => self
                .rumble
                .insert(self.nodes[0].control.upload_ff_effect(data)?),
        };
        // Off is the effect playing at zero. Stopping it instead left the motors running on
        // the ff-memless drivers (hid-playstation, hid-nintendo, xpad): the update takes the
        // effect out of "playing" until their timer runs, and a stop in between never sends
        // the motors their zero.
        if !off && !self.rumble_playing {
            effect.play(1)?;
            self.rumble_playing = true;
        }
        Ok(())
    }

    /// Write the wanted light bar colour, at most every [`LIGHTBAR_EVERY`]: an animated
    /// light bar changes on every tick. `None` leaves the light bar as it is.
    fn flush_lightbar(&mut self, now: Instant) {
        let Some(c) = self.lightbar else {
            return;
        };
        let recent = self.lightbar_at.is_some_and(|t| now < t + LIGHTBAR_EVERY);
        if self.lightbar_written == Some(c) || recent {
            return;
        }
        self.lightbar_written = Some(c);
        self.lightbar_at = Some(now);
        self.write_lights(|leds| lightbar_writes(leds, c));
    }

    /// Write the lights once more. Steam takes a controller over through hidraw just after
    /// it connects and sets its lights then, past the driver, which still reports ours.
    fn rewrite_lights(&mut self) {
        if let Some(n) = self.player {
            self.write_lights(|leds| player_writes(leds, n));
        }
        // The next poll writes the light bar.
        self.lightbar_written = None;
    }

    /// Write the lights `plan` picks from the LEDs the driver registered. Without write
    /// access (the udev rule from docs/LINUX.md) it says so once.
    fn write_lights(&mut self, plan: impl FnOnce(&[String]) -> Vec<LedWrite>) {
        let Some(device) = &self.parent else {
            return;
        };
        if self.lights_problem.is_some() {
            return;
        }
        match write_leds(device, &plan(&led_names(device))) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                tracing::warn!(name = %self.info.name, "cannot set the controller's lights: {e}");
                self.lights_problem = Some(format!(
                    "the lights of the {} cannot be set: they need the udev rule from docs/LINUX.md",
                    self.info.name
                ));
            }
            Err(e) => tracing::debug!(name = %self.info.name, "setting the lights failed: {e}"),
        }
    }
}

/// Ask BlueZ to end a Bluetooth connection, which turns the controller off.
fn disconnect(device: &str) {
    if let Err(e) = super::bluez::disconnect(device) {
        tracing::warn!(device, "BlueZ did not end the connection: {e}");
    }
}

#[derive(Default)]
struct Shared {
    /// By gamepad node path.
    pads: BTreeMap<PathBuf, Pad>,
    /// Gamepad node paths (`instance_paths`) that should be grabbed.
    hidden: HashSet<String>,
    /// Controller nodes Doppelpad may not read, with their names.
    unreadable: BTreeMap<PathBuf, String>,
}

impl Shared {
    /// Grab or release every node according to `hidden`.
    fn sync_grabs(&mut self) -> Result<()> {
        let mut result = Ok(());
        for (path, pad) in &mut self.pads {
            let want = self.hidden.contains(path.to_string_lossy().as_ref());
            for node in &mut pad.nodes {
                if let Err(e) = node.set_grab(want) {
                    tracing::warn!(path = %node.path.display(), "{e}");
                    if result.is_ok() {
                        result = Err(e);
                    }
                }
            }
        }
        for h in &self.hidden {
            if !self.pads.contains_key(Path::new(h)) {
                tracing::debug!(path = %h, "not hiding: not a controller node opened by evdev");
            }
        }
        result
    }
}

type SharedRef = Arc<Mutex<Shared>>;

/// How long setting up the backend waits for its first scan.
const FIRST_SCAN: Duration = Duration::from_secs(1);

/// The shortest time between two light bar colours written to a controller.
const LIGHTBAR_EVERY: Duration = Duration::from_millis(30);

/// How long after a controller connects every scan writes its lights again, so they
/// outlast Steam setting its own (see [`Pad::rewrite_lights`]). Not for longer: on Switch
/// controllers each light is a subcommand the driver waits for.
const LIGHTS_SETTLE: Duration = Duration::from_secs(6);

pub struct EvdevBackend {
    shared: SharedRef,
    /// Woken for an immediate scan that also retries nodes that could not be opened.
    scanner: Rescanner,
}

impl EvdevBackend {
    pub fn new() -> Self {
        let shared: SharedRef = Arc::default();
        let mut scan = Scanner {
            shared: shared.clone(),
            nodes: NodeTracker::default(),
            warned_access: false,
        };
        let (scanned, first_scan) = std::sync::mpsc::sync_channel(1);
        let scanner = Rescanner::spawn("dp-evdev-scan", RESCAN, move |pass| {
            scan.scan(pass);
            let _ = scanned.try_send(());
        });
        // The controllers connected now are there from the first poll, so `devices` right
        // after the daemon starts lists them.
        let _ = first_scan.recv_timeout(FIRST_SCAN);
        EvdevBackend { shared, scanner }
    }

    /// The device hider working on this backend's open nodes.
    pub fn hider(&self) -> GrabHider {
        GrabHider {
            shared: self.shared.clone(),
        }
    }
}

impl Drop for EvdevBackend {
    fn drop(&mut self) {
        self.scanner.stop();
        self.shared.lock().unwrap().pads.clear();
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Kind {
    Pad,
    Motion,
    Touch,
    Other,
}

fn classify(dev: &Device) -> Kind {
    if is_own(dev) {
        return Kind::Other;
    }
    if dev.properties().contains(PropType::ACCELEROMETER) && has_abs(dev, ABS_RX) {
        Kind::Motion
    } else if has_key(dev, BTN_SOUTH) && has_abs(dev, ABS_X) {
        Kind::Pad
    } else if has_abs(dev, ABS_MT_POSITION_X) && has_key(dev, BTN_LEFT) {
        Kind::Touch // only kept if it belongs to a controller
    } else {
        Kind::Other
    }
}

struct Scanner {
    shared: SharedRef,
    /// Nodes that are not controllers; motion / touch nodes without a controller yet wait
    /// for the next new controller.
    nodes: NodeTracker,
    warned_access: bool,
}

impl Scanner {
    fn scan(&mut self, pass: &Pass) {
        if pass.requested {
            self.nodes.retry_failed();
        }
        let present = event_nodes();
        let now = Instant::now();
        self.nodes.prune(&present, now);

        let mut shared = self.shared.lock().unwrap();
        shared.pads.retain(|_, pad| {
            let alive = pad.nodes[0].alive.load(Ordering::Relaxed);
            if !alive {
                tracing::info!(name = %pad.info.name, "evdev controller disconnected");
            }
            alive
        });
        for pad in shared.pads.values_mut() {
            pad.nodes.retain(|n| n.alive.load(Ordering::Relaxed));
            pad.battery = pad.parent.as_deref().and_then(read_battery);
            if now < pad.opened + LIGHTS_SETTLE {
                pad.rewrite_lights();
            }
        }
        shared.unreadable.retain(|path, _| present.contains(path));
        let known: HashSet<PathBuf> = shared
            .pads
            .values()
            .flat_map(|p| p.nodes.iter().map(|n| n.path.clone()))
            .collect();
        drop(shared);

        let mut pads = Vec::new();
        let mut extras = Vec::new();
        let mut unreadable = Vec::new();
        let mut readable = Vec::new();
        for path in present {
            if pass.stopping() {
                return;
            }
            if known.contains(&path) || self.nodes.skips(&path) {
                continue;
            }
            let dev = match Device::open(&path) {
                Ok(dev) => dev,
                Err(e) => {
                    // Only for controllers: other nodes that are not readable (audio jacks,
                    // the touchscreen on a Steam Deck) don't matter here.
                    if e.kind() == io::ErrorKind::PermissionDenied
                        && sysfs_has_key(&path, BTN_SOUTH)
                    {
                        let name = sysfs_name(&path).unwrap_or_else(|| "controller".into());
                        if !self.warned_access {
                            tracing::warn!(
                                "cannot read {} ({name}): {e}; controllers need the udev rule from docs/LINUX.md",
                                path.display(),
                            );
                            self.warned_access = true;
                        }
                        unreadable.push((path.clone(), name));
                    }
                    self.nodes.failed(path, now);
                    continue;
                }
            };
            readable.push(path.clone());
            let id = dev.input_id();
            if crate::nintendo::is_switch2(id.vendor(), id.product()) {
                // The generic node usbhid makes for one on USB. The HID backend reads it
                // through hidraw, as only its driver starts it (see `hid`), and grabs this
                // node to hide it.
                self.nodes.ignore(path);
                continue;
            }
            match classify(&dev) {
                Kind::Pad => pads.push((path, dev)),
                Kind::Motion | Kind::Touch => extras.push((path, dev)),
                Kind::Other => self.nodes.ignore(path),
            }
        }

        let mut shared = self.shared.lock().unwrap();
        shared.unreadable.extend(unreadable);
        for path in readable {
            shared.unreadable.remove(&path);
        }
        if !pads.is_empty() {
            // Motion / touch nodes seen before their controller get another chance.
            self.nodes.retry_waiting();
        }
        for (path, dev) in pads {
            match open_pad(&path, dev) {
                Ok(pad) => {
                    tracing::info!(name = %pad.info.name, id = %pad.info.id, "evdev controller connected");
                    shared.pads.insert(path, pad);
                }
                Err(e) => {
                    tracing::debug!(path = %path.display(), "cannot open controller: {e}");
                    self.nodes.failed(path, now);
                }
            }
        }
        for (path, dev) in extras {
            let parent = parent_device(&path);
            let id = dev.input_id();
            let Some(pad) = shared.pads.values_mut().find(|p| {
                p.owns(
                    parent.as_deref(),
                    dev.unique_name(),
                    id.vendor(),
                    id.product(),
                )
            }) else {
                self.nodes.wait(path);
                continue;
            };
            if let Err(e) = attach_extra(pad, &path, dev) {
                tracing::debug!(path = %path.display(), "cannot open controller node: {e}");
                self.nodes.failed(path, now);
            }
        }
        if let Err(e) = shared.sync_grabs() {
            tracing::debug!("hiding newly found nodes failed: {e}");
        }
    }
}

fn open_pad(path: &Path, dev: Device) -> io::Result<Pad> {
    let id = dev.input_id();
    let (vendor, product) = (id.vendor(), id.product());
    let name = dev.name().unwrap_or("Gamepad").to_owned();
    let joycon = joycon_side(vendor, product, &name);
    let family = if joycon.is_some() {
        PadFamily::JoyCon
    } else {
        family_for(vendor, product)
    };
    let axes: Vec<(u16, AbsRange)> = abs_axes(&dev).into_iter().map(|(c, r, _)| (c, r)).collect();
    let info = DeviceInfo {
        id: DeviceId(stable_id(
            vendor,
            product,
            dev.unique_name(),
            dev.physical_path(),
            &path.display().to_string(),
        )),
        name,
        family,
        vendor_id: vendor,
        product_id: product,
        transport: transport(id.bus_type().0),
        backend: "evdev".into(),
        instance_paths: vec![path.display().to_string()],
        joycon,
        has_motion: false,
        has_touchpad: false,
        has_rumble: dev
            .supported_ff()
            .is_some_and(|ff| ff.contains(FFEffectCode::FF_RUMBLE)),
        mirror: crate::steam::is_virtual_pad(vendor, product),
    };
    let uniq = dev.unique_name().map(str::to_owned);
    let bluetooth = uniq
        .clone()
        .filter(|u| id.bus_type().0 == BUS_BLUETOOTH && !u.is_empty());
    let parent = parent_device(path);
    let parts = Arc::new(Mutex::new(Parts::default()));
    let mapper = Mapper::Pad(PadMapper::new(family, &axes).joycon(joycon));
    let node = Node::start(path.to_owned(), dev, mapper, parts.clone())?;
    Ok(Pad {
        info: Arc::new(info),
        battery: parent.as_deref().and_then(read_battery),
        parent,
        uniq,
        parts,
        nodes: vec![node],
        rumble: None,
        rumble_playing: false,
        last_rumble: Rumble::default(),
        bluetooth,
        lightbar: None,
        lightbar_written: None,
        lightbar_at: None,
        player: None,
        lights_problem: None,
        opened: Instant::now(),
    })
}

fn attach_extra(pad: &mut Pad, path: &Path, dev: Device) -> io::Result<()> {
    let axes = abs_axes(&dev);
    let range = |code| {
        axes.iter()
            .find(|a| a.0 == code)
            .map(|a| a.1)
            .unwrap_or_default()
    };
    let mut info = (*pad.info).clone();
    let mapper = match classify(&dev) {
        Kind::Motion => {
            info.has_motion = true;
            let ranges: Vec<(u16, AbsRange)> = axes.iter().map(|a| (a.0, a.1)).collect();
            Mapper::Motion(if dev.input_id().vendor() == NINTENDO_VID {
                MotionMapper::nintendo(&ranges)
            } else {
                MotionMapper::new(&ranges)
            })
        }
        _ => {
            info.has_touchpad = true;
            Mapper::Touch(TouchMapper::new(
                range(ABS_MT_POSITION_X),
                range(ABS_MT_POSITION_Y),
            ))
        }
    };
    let node = Node::start(path.to_owned(), dev, mapper, pad.parts.clone())?;
    tracing::debug!(path = %path.display(), name = %info.name, "merged controller node");
    pad.nodes.push(node);
    pad.info = Arc::new(info);
    Ok(())
}

impl GamepadBackend for EvdevBackend {
    fn name(&self) -> &'static str {
        "evdev"
    }

    fn poll(&mut self, out: &mut Vec<PadSnapshot>) {
        let mut shared = self.shared.lock().unwrap();
        let now = Instant::now();
        for pad in shared.pads.values_mut() {
            // A colour held back by the pacing.
            pad.flush_lightbar(now);
            let state = pad.parts.lock().unwrap().compose();
            out.push(PadSnapshot {
                info: pad.info.clone(),
                state,
                battery: pad.battery,
                raw: None,
            });
        }
    }

    fn set_rumble(&mut self, id: &DeviceId, rumble: Rumble) {
        let mut shared = self.shared.lock().unwrap();
        let Some(pad) = shared.pads.values_mut().find(|p| &p.info.id == id) else {
            return;
        };
        if !pad.info.has_rumble || pad.last_rumble == rumble {
            return;
        }
        pad.last_rumble = rumble;
        if let Err(e) = pad.set_rumble(rumble) {
            tracing::debug!(device = %id, "rumble failed: {e}");
        }
    }

    fn set_lightbar(&mut self, id: &DeviceId, color: Option<Rgb>) {
        let mut shared = self.shared.lock().unwrap();
        let Some(pad) = shared.pads.values_mut().find(|p| &p.info.id == id) else {
            return;
        };
        pad.lightbar = color;
        pad.flush_lightbar(Instant::now());
    }

    /// The player lights. The DualSense's microphone LED has no LED device: hid-playstation
    /// drives it from the mute button itself.
    fn set_leds(&mut self, id: &DeviceId, leds: Leds) {
        let mut shared = self.shared.lock().unwrap();
        let Some(pad) = shared.pads.values_mut().find(|p| &p.info.id == id) else {
            return;
        };
        if pad.player == leds.player {
            return;
        }
        pad.player = leds.player;
        if let Some(n) = leds.player {
            pad.write_lights(|leds| player_writes(leds, n));
        }
    }

    /// Ends the Bluetooth connection, after which DualShock 4, DualSense and Switch
    /// controllers switch off. A controller on its cable stays on.
    fn power_off(&mut self, id: &DeviceId) -> bool {
        let shared = self.shared.lock().unwrap();
        let Some(pad) = shared.pads.values().find(|p| &p.info.id == id) else {
            return false;
        };
        let (Some(device), Some(mac)) = (&pad.parent, &pad.bluetooth) else {
            return false;
        };
        let Some(path) = bluez_device_path(&device.to_string_lossy(), mac) else {
            return false;
        };
        let spawned = std::thread::Builder::new()
            .name("dp-bluez".into())
            .spawn(move || disconnect(&path));
        spawned.is_ok()
    }

    fn problems(&self) -> Vec<String> {
        let shared = self.shared.lock().unwrap();
        let mut out: Vec<String> = shared
            .unreadable
            .values()
            .map(|name| {
                format!(
                    "the {name} cannot be read: controllers need the udev rule from docs/LINUX.md"
                )
            })
            .collect();
        out.extend(
            shared
                .pads
                .values()
                .filter_map(|p| p.lights_problem.clone()),
        );
        out.sort();
        out
    }

    fn rescan(&mut self) {
        self.scanner.wake();
    }
}

/// Hides controllers by grabbing their nodes (`instance_paths` = gamepad node path); motion
/// and touchpad nodes of the same controller are grabbed with it.
pub struct GrabHider {
    shared: SharedRef,
}

impl DeviceHider for GrabHider {
    fn available(&self) -> Result<()> {
        Ok(())
    }

    fn hide(&mut self, instance_paths: &[String]) -> Result<()> {
        let mut shared = self.shared.lock().unwrap();
        shared.hidden = instance_paths.iter().cloned().collect();
        let result = shared.sync_grabs();
        if result.is_ok() && !instance_paths.is_empty() {
            tracing::info!(devices = ?instance_paths, "devices hidden via EVIOCGRAB");
        }
        result
    }

    fn restore(&mut self) -> Result<()> {
        let mut shared = self.shared.lock().unwrap();
        if shared.hidden.is_empty() {
            return Ok(());
        }
        shared.hidden.clear();
        shared.sync_grabs()
    }
}

impl Drop for GrabHider {
    fn drop(&mut self) {
        if let Err(e) = self.restore() {
            tracing::warn!("could not release grabbed controllers: {e}");
        }
    }
}
