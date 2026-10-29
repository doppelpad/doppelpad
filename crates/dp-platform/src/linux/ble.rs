//! Switch 2 controllers over Bluetooth LE: found through BlueZ's D-Bus interface
//! ([`super::bluez`]), connected and read on an ATT socket of Doppelpad's own
//! ([`super::l2cap`]).
//!
//! BlueZ cannot pair them (see [`crate::nintendo::switch2ble`]), and its GATT client never
//! finishes reading one (see [`super::att`]). So Doppelpad runs a discovery of its own, and when
//! a controller advertises pairing mode it opens an L2CAP socket on the controller's ATT channel
//! itself, which connects it without pairing, finds the characteristics and switches input and
//! reply notifications on ([`super::gatt`]). The controller then runs in the HID backend as on
//! Windows and macOS, over a [`crate::switch2gatt`] link. Closing the socket ends the link, also
//! when Doppelpad ends. The discovery runs only while the search is on
//! ([`crate::Switch2Search`]).
//!
//! A `dp-ble-upkeep` thread keeps the discovery running (every D-Bus call may take up to 10 s,
//! so not on the scanner's thread), a `dp-ble` thread follows BlueZ's signals: what the
//! discovery finds, and controllers that disconnect. Each controller is set up on a
//! `dp-ble-connect` thread, as that waits for the controller's answers, and then read on a
//! `dp-ble-input` thread.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use crossbeam_channel::{RecvTimeoutError, Sender};
use zbus::blocking::Connection;

use super::bluez::{self, ADAPTER, DEVICE, Signal, Signals};
use super::l2cap::{AttSocket, LeAddr};
use super::sysfs::{bluez_adapter, bluez_address};
use super::{att, gatt};
use crate::bdaddr::{BdAddr, LeAddrType};
use crate::nintendo::switch2ble::Advert;
use crate::switch2gatt::{
    self, Attempt, CONNECT_TIMEOUT, Feed, Gone, Peers, Poll, Ready, SILENCE, Target, WRITE_TIMEOUT,
    Writer,
};

/// Make sure the discovery runs this often.
const RESCAN: Duration = Duration::from_secs(10);
/// Start the scan over when a controller has not connected after this long (see [`open`]).
const RESCAN_WAIT: Duration = Duration::from_secs(2);
/// How long the input thread waits for the controller before it looks at the silence again.
const INPUT_WAIT: Duration = Duration::from_millis(250);

/// Whether Switch 2 controllers can connect, as a driver check: BlueZ running, with a
/// Bluetooth adapter that is on.
pub fn check() -> crate::Result<()> {
    bluez::check()
}

/// Turn the controller with this address off. It has no command for that, but turns off when
/// its link ends.
pub(super) fn turn_off(address: BdAddr) -> io::Result<()> {
    let path = lock(&central().peers)
        .up_keys()
        .find(|path| bluez_address(path) == Some(address))
        .cloned();
    match path {
        Some(path) => {
            let_go(&path, "turned off");
            Ok(())
        }
        None => Err(io::ErrorKind::NotConnected.into()),
    }
}

/// What changed since the last call, for the HID backend's scanner ([`super::hid`]), with the
/// search on or off as `search` says. Looks for controllers from the first call on; the D-Bus
/// work happens on the upkeep thread.
pub(super) fn poll(search: bool) -> Poll {
    let c = central();
    let changed = c.searching.swap(search, Ordering::Relaxed) != search;
    {
        let mut upkeep = lock(&c.upkeep);
        match &*upkeep {
            Some(wake) if changed => {
                let _ = wake.try_send(());
            }
            Some(_) => {}
            None => *upkeep = start_upkeep(),
        }
    }
    lock(&c.peers).poll()
}

/// Let every controller go and stop the upkeep, which ends Doppelpad's discovery.
pub(super) fn let_all_go() {
    let Some(c) = CENTRAL.get() else {
        return;
    };
    lock(&c.upkeep).take();
    let paths = lock(&c.peers).keys();
    for path in paths {
        let_go(&path, "Doppelpad stops");
    }
}

static CENTRAL: OnceLock<Central> = OnceLock::new();

fn central() -> &'static Central {
    CENTRAL.get_or_init(Central::default)
}

#[derive(Default)]
struct Central {
    /// Whether the search is on, as the scanner last told.
    searching: AtomicBool,
    /// Wakes the upkeep thread; dropping it ends the thread.
    upkeep: Mutex<Option<Sender<()>>>,
    /// Controllers being connected or connected, by BlueZ object path.
    peers: Mutex<Peers<String, Conn>>,
}

/// Lock the client's state even after a thread of it panicked.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What keeps a connected controller's link: dropping it ends the link and the input thread.
struct Conn {
    socket: Arc<AttSocket>,
    /// Tells the input thread to end.
    stop: Arc<AtomicBool>,
}

impl Drop for Conn {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.socket.shutdown();
    }
}

/// Why a controller could not be set up.
struct Failure(String);

impl From<zbus::Error> for Failure {
    fn from(e: zbus::Error) -> Self {
        Failure(e.to_string())
    }
}

impl From<io::Error> for Failure {
    fn from(e: io::Error) -> Self {
        Failure(e.to_string())
    }
}

/// Start the upkeep thread; its waker.
fn start_upkeep() -> Option<Sender<()>> {
    let (wake, woken) = crossbeam_channel::bounded(1);
    let spawned = std::thread::Builder::new()
        .name("dp-ble-upkeep".into())
        .spawn(move || {
            let mut watch = Watch::default();
            loop {
                keep_watching(&mut watch);
                match woken.recv_timeout(RESCAN) {
                    Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            if watch.searching {
                stop_searching();
            }
        });
    match spawned {
        Ok(_) => Some(wake),
        Err(e) => {
            tracing::warn!("Bluetooth: cannot look for controllers: {e}");
            None
        }
    }
}

/// What the upkeep thread keeps track of.
#[derive(Default)]
struct Watch {
    /// Whether the search was on at its last look.
    searching: bool,
    /// The `dp-ble` thread follows BlueZ's signals.
    listening: Arc<AtomicBool>,
}

/// Follow BlueZ's signals and keep the discovery running, again after Bluetooth was off or
/// BlueZ started over; not while a controller connects (see [`connect`]). While the search is
/// off, no discovery of Doppelpad's runs.
fn keep_watching(w: &mut Watch) {
    let search = central().searching.load(Ordering::Relaxed);
    let stopped = w.searching && !search;
    w.searching = search;
    if !search {
        if stopped {
            stop_searching();
        }
        return;
    }
    let bus = match bluez::bus() {
        Ok(bus) => bus,
        Err(e) => {
            tracing::debug!("Bluetooth: cannot reach the system bus: {e}");
            return;
        }
    };
    if !w.listening.load(Ordering::Relaxed) {
        match listen(&bus, w.listening.clone()) {
            Ok(()) => tracing::info!("Bluetooth: looking for Switch 2 controllers in pairing mode"),
            Err(e) => {
                tracing::debug!("Bluetooth: cannot follow BlueZ: {e}");
                return;
            }
        }
    }
    if lock(&central().peers).any_connecting() {
        return;
    }
    match bluez::adapters(&bus) {
        Ok(adapters) => {
            for (adapter, _) in adapters.iter().filter(|(_, on)| *on) {
                bluez::discover(&bus, adapter);
            }
        }
        Err(e) => tracing::debug!("Bluetooth: cannot ask BlueZ for its adapters: {e}"),
    }
}

/// End Doppelpad's discovery on every adapter. Controllers already connected stay.
fn stop_searching() {
    tracing::info!("Bluetooth: no longer looking for Switch 2 controllers");
    let adapters = bluez::bus().and_then(|bus| Ok((bluez::adapters(&bus)?, bus)));
    match adapters {
        Ok((adapters, bus)) => {
            for (adapter, _) in adapters {
                bluez::stop_discovery(&bus, &adapter);
            }
        }
        Err(e) => tracing::debug!("Bluetooth: cannot ask BlueZ for its adapters: {e}"),
    }
}

/// Start the thread that follows BlueZ's signals.
fn listen(bus: &Connection, listening: Arc<AtomicBool>) -> zbus::Result<()> {
    let signals = Signals::follow(bus)?;
    listening.store(true, Ordering::Relaxed);
    let thread_listening = listening.clone();
    let spawned = std::thread::Builder::new()
        .name("dp-ble".into())
        .spawn(move || {
            for signal in signals {
                match signal {
                    Ok(signal) => on_signal(signal),
                    Err(e) => tracing::debug!("Bluetooth: a signal from BlueZ: {e}"),
                }
            }
            // The connection broke: a new one with the next look.
            tracing::debug!("Bluetooth: the system bus is gone");
            super::sysbus::reset();
            thread_listening.store(false, Ordering::Relaxed);
        });
    if let Err(e) = spawned {
        listening.store(false, Ordering::Relaxed);
        return Err(zbus::Error::InputOutput(Arc::new(e)));
    }
    Ok(())
}

fn on_signal(signal: Signal) {
    match signal {
        Signal::Device { path, props } => {
            if props.get("Connected").and_then(|c| bool::try_from(c).ok()) == Some(false) {
                let_go(&path, "it disconnected");
            }
            if let Some(advert) = props.get("ManufacturerData").and_then(|m| bluez::advert(m)) {
                discovered(path, advert);
            }
        }
        Signal::Removed { path } => let_go(&path, "BlueZ forgot it"),
        Signal::Disconnected {
            path,
            reason,
            message,
        } => {
            if lock(&central().peers).has(&path) {
                tracing::debug!("Bluetooth: the link ended: {reason} ({message})");
            }
        }
    }
}

fn discovered(path: String, advert: Advert) {
    let c = central();
    let mut peers = lock(&c.peers);
    if !peers.admit(&path, &advert, c.searching.load(Ordering::Relaxed)) {
        return;
    }
    let attempt = peers.connecting(path.clone(), ());
    let thread_path = path.clone();
    let spawned = std::thread::Builder::new()
        .name("dp-ble-connect".into())
        .spawn(move || connect(thread_path, attempt, advert));
    if let Err(e) = spawned {
        tracing::warn!("Bluetooth: cannot connect the controller: {e}");
        peers.failed(&path, attempt);
    }
}

/// A connected controller, set up and waiting for its input thread.
struct Opened {
    feed: Feed,
    ready: Ready,
    socket: Arc<AttSocket>,
    handles: gatt::Handles,
}

/// Set a controller up and hand it to the scanner; Doppelpad's discovery pauses meanwhile.
fn connect(path: String, attempt: Attempt, advert: Advert) {
    let adapter = bluez_adapter(&path).to_owned();
    let opened = bluez::bus().map_err(Failure::from).and_then(|bus| {
        let opened = open(&bus, &path, &adapter, advert);
        // Not when the search was turned off meanwhile.
        if central().searching.load(Ordering::Relaxed) {
            bluez::discover(&bus, &adapter);
        }
        opened
    });
    let mut peers = lock(&central().peers);
    let opened = match opened {
        Ok(opened) => opened,
        Err(Failure(e)) => {
            tracing::info!("Bluetooth: connecting the controller failed: {e}");
            peers.failed(&path, attempt);
            return;
        }
    };
    // Not when it disconnected while it was set up; dropping the socket ends the link.
    if !peers.is_connecting(&path, attempt) {
        return;
    }
    let stop = Arc::new(AtomicBool::new(false));
    let (feed, socket, thread_path, thread_stop) = (
        opened.feed.clone(),
        opened.socket.clone(),
        path.clone(),
        stop.clone(),
    );
    let handles = opened.handles;
    let spawned = std::thread::Builder::new()
        .name("dp-ble-input".into())
        .spawn(move || {
            if let Some(why) = read_input(&feed, &socket, handles, &thread_stop) {
                let gone = lock(&central().peers).let_go_attempt(&thread_path, attempt);
                end(gone, why);
            }
        });
    match spawned {
        Ok(_) => {
            let conn = Conn {
                socket: opened.socket,
                stop,
            };
            // Still connecting: checked under the same lock.
            let _ = peers.up(&path, attempt, opened.feed, conn, opened.ready);
        }
        Err(e) => {
            tracing::warn!("Bluetooth: cannot read the controller: {e}");
            peers.failed(&path, attempt);
        }
    }
}

/// Connect a controller and set it up.
fn open(bus: &Connection, path: &str, adapter: &str, advert: Advert) -> Result<Opened, Failure> {
    let until = Instant::now() + CONNECT_TIMEOUT;
    let Some(key) = bluez_address(path) else {
        return Err(Failure(format!("{path} is not a device")));
    };
    let address = |path: &str, iface: &str| -> Result<LeAddr, Failure> {
        let text = bluez::text_property(bus, path, iface, "Address")?;
        let kind = match bluez::text_property(bus, path, iface, "AddressType")?.as_str() {
            "random" => LeAddrType::Random,
            _ => LeAddrType::Public,
        };
        BdAddr::parse(&text)
            .map(|a| (a, kind))
            .ok_or_else(|| Failure(format!("{text} is not an address")))
    };
    let started = Instant::now();
    let mut socket = AttSocket::connect(address(adapter, ADAPTER)?, address(path, DEVICE)?)?;
    // The kernel connects at the controller's next advertisement. While a discovery runs
    // (Steam keeps one running), the adapter holds back advertisements it has seen, so the
    // connection may never come about. Ending Doppelpad's discovery starts the scan over;
    // stopping it before connecting failed as often ("le-connection-abort-by-local").
    bluez::stop_discovery(bus, adapter);
    while !socket.connected(until.min(Instant::now() + RESCAN_WAIT))? {
        if Instant::now() >= until {
            return Err(Failure("the controller did not connect in time".into()));
        }
        tracing::debug!("Bluetooth: not connected yet, starting the scan over");
        bluez::discover(bus, adapter);
        bluez::stop_discovery(bus, adapter);
    }
    tracing::debug!(
        "Bluetooth: connected in {} ms, reading the controller",
        started.elapsed().as_millis()
    );
    let handles = gatt::set_up(&mut socket, advert.model, until)?;
    tracing::debug!(
        "Bluetooth: set up after {} ms",
        started.elapsed().as_millis()
    );
    let socket = Arc::new(socket);
    let (feed, io, commands) = switch2gatt::link(Box::new(AttWriter {
        socket: socket.clone(),
        command: handles.command,
        vibration: handles.vibration,
    }));
    let mut ready = Ready::new(&key.to_string(), advert, io, commands);
    ready.address = Some(key);
    Ok(Opened {
        feed,
        ready,
        socket,
        handles,
    })
}

/// Read a controller's input and replies, and answer what it asks, until it is let go
/// (`None`) or the link fails (why).
fn read_input(
    feed: &Feed,
    socket: &AttSocket,
    handles: gatt::Handles,
    stop: &AtomicBool,
) -> Option<&'static str> {
    while !stop.load(Ordering::Relaxed) {
        match socket.recv(Instant::now() + INPUT_WAIT) {
            Ok(Some(pdu)) => match att::incoming(&pdu) {
                att::Incoming::Notification { handle, value } if handle == handles.input => {
                    if !feed.input(value.to_vec()) {
                        return Some("nothing reads it any more");
                    }
                }
                att::Incoming::Notification { handle, value } if handle == handles.reply => {
                    feed.reply(value.to_vec());
                }
                att::Incoming::Answer(answer) => {
                    let _ = socket.send(&answer, Instant::now() + WRITE_TIMEOUT);
                }
                _ => {}
            },
            Ok(None) => {}
            Err(_) if stop.load(Ordering::Relaxed) => return None,
            Err(_) => return Some("the link is gone"),
        }
        if feed.silent_for() > SILENCE {
            return Some("no input for a while");
        }
    }
    None
}

/// The controller is gone, or is let go: its device thread stops, and its link ends.
fn let_go(path: &str, why: &str) {
    let gone = lock(&central().peers).let_go(&path.to_owned());
    end(gone, why);
}

/// End what kept a controller that was let go; outside the lock, as it waits for nothing.
fn end(gone: Option<Gone<Conn, ()>>, why: &str) {
    // Being set up: `connect` sees it is gone.
    if let Some(Gone::Up(conn)) = gone {
        tracing::info!("Bluetooth: Switch 2 controller disconnected: {why}");
        drop(conn);
    }
}

/// Writes to a connected controller's characteristics, as write commands (without response).
struct AttWriter {
    socket: Arc<AttSocket>,
    command: u16,
    vibration: Option<u16>,
}

impl Writer for AttWriter {
    fn write(&self, target: Target, data: Vec<u8>) {
        let sent = match (target, self.vibration) {
            (Target::Command, _) => self.socket.send(
                &att::write_command(self.command, &data),
                Instant::now() + WRITE_TIMEOUT,
            ),
            // Rumble comes again soon; commands are worth the wait.
            (Target::Vibration, Some(handle)) => {
                match self.socket.try_send(&att::write_command(handle, &data)) {
                    Ok(false) => {
                        tracing::trace!("Bluetooth: no room for a rumble report");
                        Ok(())
                    }
                    other => other.map(drop),
                }
            }
            (Target::Vibration, None) => Ok(()),
        };
        if let Err(e) = sent {
            tracing::debug!("Bluetooth: writing to the controller failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The client's log, for the hardware tests.
    fn log() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("dp_platform::linux::ble=debug")
            .try_init();
    }

    /// Connects a Pro Controller in pairing mode near this computer in the backend, turns it
    /// off after 5 s, and watches for 10 s that it stays gone. Waits half an hour for it. Run
    /// with `--nocapture`.
    #[test]
    #[ignore = "connects a Switch 2 Pro Controller in pairing mode near this computer"]
    fn switch2_pro_turned_off() {
        use crate::GamepadBackend;

        log();
        let mut backend = super::super::hid::backend(crate::Switch2Search::default());
        println!("hold the pairing button until the lights run");
        let until = Instant::now() + Duration::from_secs(1800);
        let mut pads = Vec::new();
        while pads.is_empty() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(100));
            backend.poll(&mut pads);
        }
        let info = pads.first().expect("no controller connected").info.clone();
        println!("{} connected, turning it off in 5 s", info.name);
        std::thread::sleep(Duration::from_secs(5));
        assert!(
            backend.power_off(&info.id),
            "the backend cannot turn it off"
        );
        println!("turned off; it should go dark and stay off");
        for second in 1..=10 {
            std::thread::sleep(Duration::from_secs(1));
            pads.clear();
            backend.poll(&mut pads);
            println!("second {second}: {} controllers", pads.len());
        }
    }

    /// Connects a Pro Controller in pairing mode near this computer and drives it for a while;
    /// see [`switch2gatt::drive_controller`]. Run with `--nocapture`.
    #[test]
    #[ignore = "connects a Switch 2 Pro Controller in pairing mode near this computer"]
    fn switch2_pro_over_bluetooth() {
        log();
        switch2gatt::drive_controller(|| poll(true));
        let_all_go();
    }
}
