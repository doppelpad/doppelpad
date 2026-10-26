//! Switch 2 controllers over Bluetooth LE: found through BlueZ's D-Bus interface, connected
//! and read on an ATT socket of Doppelpad's own.
//!
//! BlueZ cannot pair them (see [`crate::nintendo::switch2ble`]), and its GATT client never
//! finishes reading one (see [`super::att`]). So Doppelpad runs a discovery of its own, and when
//! a controller advertises pairing mode it opens an L2CAP socket on the controller's ATT channel
//! itself, which connects it without pairing, finds the characteristics and switches input and
//! reply notifications on. The controller then runs in the HID backend as on Windows and macOS,
//! over a [`crate::switch2gatt`] link. A controller that is looking for the console it is paired
//! with is left alone. Closing the socket ends the link, also when Doppelpad ends.
//!
//! A `dp-ble` thread follows BlueZ's signals: what the discovery finds, and controllers that
//! disconnect. Each controller is set up on a `dp-ble-connect` thread, as that waits for the
//! controller's answers, and then read on a `dp-ble-input` thread.

use std::collections::{HashMap, HashSet};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type as MessageType;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use super::att;
use crate::nintendo::Switch2Model;
use crate::nintendo::switch2ble::{self, Advert};
use crate::switch2gatt::{self, Feed, Links, Poll, Ready, Target, Writer};

const BLUEZ: &str = "org.bluez";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";

/// Make sure the discovery runs this often.
const RESCAN: Duration = Duration::from_secs(10);
/// Give up on a controller that has not connected and answered by then.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a command may take to go out.
const WRITE_TIMEOUT: Duration = Duration::from_secs(1);
/// Start the scan over when a controller has not connected after this long (see [`open`]).
const RESCAN_WAIT: Duration = Duration::from_secs(2);
/// A controller whose input stopped for this long is let go. It sends all the time (at the
/// least about 17 reports a second, even with nothing touched).
const SILENCE: Duration = Duration::from_secs(3);
/// How long the input thread waits for the controller before it looks at the silence again.
const INPUT_WAIT: Duration = Duration::from_millis(250);
/// An input notification: report 0x05 without its ID. The MTU must leave room for it.
const INPUT_LEN: u16 = 63;

type Properties = HashMap<String, OwnedValue>;
type Objects = HashMap<OwnedObjectPath, HashMap<String, Properties>>;

/// Whether Switch 2 controllers can connect, as a driver check: BlueZ running, with a
/// Bluetooth adapter that is on.
pub fn check() -> crate::Result<()> {
    let unusable = |why: String| Err(crate::PlatformError::Other(why));
    let bus = match bus() {
        Ok(bus) => bus,
        Err(e) => return unusable(format!("cannot reach the system bus: {e}")),
    };
    let objects = match managed_objects(&bus) {
        Ok(objects) => objects,
        Err(e) if error_name(&e) == Some("org.freedesktop.DBus.Error.ServiceUnknown") => {
            return unusable("BlueZ (bluetoothd) is not running".into());
        }
        Err(e) => return unusable(format!("BlueZ does not answer: {e}")),
    };
    let adapters: Vec<&Properties> = objects.values().filter_map(|i| i.get(ADAPTER)).collect();
    if adapters.is_empty() {
        unusable("no Bluetooth adapter".into())
    } else if adapters.iter().any(|a| flag(a, "Powered")) {
        Ok(())
    } else {
        unusable("Bluetooth is off".into())
    }
}

/// Turn the controller with this address off. It has no command for that, but turns off when
/// its link ends.
pub(super) fn turn_off(address: u64) -> io::Result<()> {
    let wanted = format!("{address:012x}");
    let path = central()
        .state
        .lock()
        .unwrap()
        .peers
        .iter()
        .find(|(path, peer)| {
            matches!(peer, Peer::Up { .. }) && key(path).is_some_and(|k| k == wanted)
        })
        .map(|(path, _)| path.clone());
    match path {
        Some(path) => {
            let_go(&path, "turned off");
            Ok(())
        }
        None => Err(io::ErrorKind::NotConnected.into()),
    }
}

/// What changed since the last call, for the HID backend's scanner ([`super::hid`]). Looks for
/// controllers from the first call on.
pub(super) fn poll() -> Poll {
    let c = central();
    keep_watching(&mut c.watch.lock().unwrap());
    c.state.lock().unwrap().links.poll()
}

static CENTRAL: OnceLock<Central> = OnceLock::new();

fn central() -> &'static Central {
    CENTRAL.get_or_init(Central::default)
}

#[derive(Default)]
struct Central {
    watch: Mutex<Watch>,
    state: Mutex<State>,
}

#[derive(Default)]
struct Watch {
    /// When the discovery was last looked at.
    checked: Option<Instant>,
    /// The `dp-ble` thread follows BlueZ's signals.
    listening: Arc<AtomicBool>,
}

#[derive(Default)]
struct State {
    /// Controllers being connected or connected, by BlueZ object path.
    peers: HashMap<String, Peer>,
    /// Controllers already logged as not wanted, by BlueZ object path.
    passed: HashSet<String>,
    links: Links,
}

/// One controller, from discovery until it disconnects.
enum Peer {
    /// Being set up on its own thread.
    Connecting,
    Up {
        feed: Feed,
        socket: Arc<AttSocket>,
        /// Tells its input thread to end.
        stop: Arc<AtomicBool>,
    },
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

/// The system bus (see [`super::sysbus`]).
fn bus() -> zbus::Result<Connection> {
    super::sysbus::connection()
}

/// Follow BlueZ's signals and keep the discovery running, again after Bluetooth was off or
/// BlueZ started over; not while a controller connects (see [`connect`]).
fn keep_watching(w: &mut Watch) {
    if w.checked.is_some_and(|at| at.elapsed() < RESCAN) {
        return;
    }
    w.checked = Some(Instant::now());
    let bus = match bus() {
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
    let connecting = central()
        .state
        .lock()
        .unwrap()
        .peers
        .values()
        .any(|p| matches!(p, Peer::Connecting));
    if connecting {
        return;
    }
    match managed_objects(&bus) {
        Ok(objects) => {
            for (path, ifaces) in &objects {
                if ifaces.get(ADAPTER).is_some_and(|a| flag(a, "Powered")) {
                    discover(&bus, path.as_str());
                }
            }
        }
        Err(e) => tracing::debug!("Bluetooth: cannot ask BlueZ for its adapters: {e}"),
    }
}

/// Start the thread that follows BlueZ's signals.
fn listen(bus: &Connection, listening: Arc<AtomicBool>) -> zbus::Result<()> {
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(BLUEZ)?
        .build();
    let signals = MessageIterator::for_match_rule(rule, bus, Some(256))?;
    listening.store(true, Ordering::Relaxed);
    let thread_listening = listening.clone();
    let spawned = std::thread::Builder::new()
        .name("dp-ble".into())
        .spawn(move || {
            for signal in signals {
                match signal {
                    Ok(signal) => on_signal(&signal),
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

/// Keep a discovery running on an adapter: Doppelpad's own, which BlueZ ends when Doppelpad
/// does. Asking again while it runs changes nothing.
///
/// Bluetooth LE only, and every advertisement told: what BlueZ knows of a device may be
/// minutes old, so only a fresh advertisement says a controller is in pairing mode now.
fn discover(bus: &Connection, adapter: &str) {
    let filter = HashMap::from([
        ("Transport", Value::from("le")),
        ("DuplicateData", Value::from(true)),
    ]);
    let started = bus
        .call_method(
            Some(BLUEZ),
            adapter,
            Some(ADAPTER),
            "SetDiscoveryFilter",
            &filter,
        )
        .and_then(|_| bus.call_method(Some(BLUEZ), adapter, Some(ADAPTER), "StartDiscovery", &()));
    match started {
        Ok(_) => tracing::debug!(adapter, "Bluetooth: discovery started"),
        Err(e) if error_name(&e) == Some("org.bluez.Error.InProgress") => {}
        Err(e) => tracing::debug!(adapter, "Bluetooth: cannot start a discovery: {e}"),
    }
}

fn on_signal(signal: &zbus::Message) {
    let header = signal.header();
    let (Some(path), Some(member)) = (header.path(), header.member()) else {
        return;
    };
    let body = signal.body();
    match member.as_str() {
        "PropertiesChanged" => {
            if let Ok((iface, changed, _)) = body.deserialize::<(String, Properties, Vec<String>)>()
                && iface == DEVICE
            {
                on_device(path.as_str(), &changed);
            }
        }
        "InterfacesAdded" => {
            if let Ok((path, ifaces)) =
                body.deserialize::<(OwnedObjectPath, HashMap<String, Properties>)>()
                && let Some(device) = ifaces.get(DEVICE)
            {
                on_device(path.as_str(), device);
            }
        }
        // Why a link ended (BlueZ 5.78 and later).
        "Disconnected" => {
            if let Ok((reason, message)) = body.deserialize::<(String, String)>()
                && central()
                    .state
                    .lock()
                    .unwrap()
                    .peers
                    .contains_key(path.as_str())
            {
                tracing::debug!("Bluetooth: the link ended: {reason} ({message})");
            }
        }
        "InterfacesRemoved" => {
            if let Ok((path, ifaces)) = body.deserialize::<(OwnedObjectPath, Vec<String>)>()
                && ifaces.iter().any(|i| i == DEVICE)
            {
                let_go(path.as_str(), "BlueZ forgot it");
            }
        }
        _ => {}
    }
}

/// A device BlueZ found, or properties of one that changed.
fn on_device(path: &str, props: &Properties) {
    if props.get("Connected").and_then(|c| bool::try_from(c).ok()) == Some(false) {
        let_go(path, "it disconnected");
    }
    if let Some(advert) = props.get("ManufacturerData").and_then(|m| advert(m)) {
        discovered(path, advert);
    }
}

fn discovered(path: &str, advert: Advert) {
    let mut state = central().state.lock().unwrap();
    if state.peers.contains_key(path) {
        return;
    }
    if !advert.pairing || advert.model != Switch2Model::Pro {
        if state.passed.insert(path.to_owned()) {
            if advert.pairing {
                tracing::info!(
                    model = ?advert.model,
                    "Bluetooth: only the Switch 2 Pro Controller is supported over Bluetooth yet"
                );
            } else {
                tracing::debug!(model = ?advert.model, "Bluetooth: a Switch 2 controller looks for its console, left alone");
            }
        }
        return;
    }
    tracing::info!("Bluetooth: Switch 2 Pro Controller in pairing mode, connecting");
    let owned = path.to_owned();
    let spawned = std::thread::Builder::new()
        .name("dp-ble-connect".into())
        .spawn(move || connect(owned, advert));
    match spawned {
        Ok(_) => {
            state.peers.insert(path.to_owned(), Peer::Connecting);
        }
        Err(e) => tracing::warn!("Bluetooth: cannot connect the controller: {e}"),
    }
}

/// A connected controller, set up and waiting for its input thread.
struct Opened {
    feed: Feed,
    ready: Ready,
    socket: Arc<AttSocket>,
    handles: Handles,
}

/// Set a controller up and hand it to the scanner; Doppelpad's discovery pauses meanwhile.
fn connect(path: String, advert: Advert) {
    let adapter = adapter_of(&path).to_owned();
    let opened = bus().map_err(Failure::from).and_then(|bus| {
        let opened = open(&bus, &path, &adapter, advert);
        discover(&bus, &adapter);
        opened
    });
    let mut state = central().state.lock().unwrap();
    // Not when it disconnected while it was set up; dropping the socket ends the link.
    let wanted = matches!(state.peers.get(&path), Some(Peer::Connecting));
    match opened {
        Ok(opened) if wanted => {
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
                        let_go(&thread_path, why);
                    }
                });
            match spawned {
                Ok(_) => {
                    tracing::info!("Bluetooth: Switch 2 Pro Controller connected");
                    state.links.add(opened.ready);
                    state.peers.insert(
                        path,
                        Peer::Up {
                            feed: opened.feed,
                            socket: opened.socket,
                            stop,
                        },
                    );
                }
                Err(e) => {
                    tracing::warn!("Bluetooth: cannot read the controller: {e}");
                    state.peers.remove(&path);
                }
            }
        }
        Ok(_) => {}
        Err(Failure(e)) => {
            tracing::info!("Bluetooth: connecting the controller failed: {e}");
            if wanted {
                state.peers.remove(&path);
            }
        }
    }
}

/// Connect a controller and set it up.
fn open(bus: &Connection, path: &str, adapter: &str, advert: Advert) -> Result<Opened, Failure> {
    let until = Instant::now() + CONNECT_TIMEOUT;
    let Some(key) = key(path) else {
        return Err(Failure(format!("{path} is not a device")));
    };
    let address = |path: &str, iface: &str| -> Result<([u8; 6], bool), Failure> {
        let text = text_property(bus, path, iface, "Address")?;
        let random = text_property(bus, path, iface, "AddressType")? == "random";
        bdaddr(&text)
            .map(|a| (a, random))
            .ok_or_else(|| Failure(format!("{text} is not an address")))
    };
    let started = Instant::now();
    let mut socket = AttSocket::connect(address(adapter, ADAPTER)?, address(path, DEVICE)?)?;
    // The kernel connects at the controller's next advertisement. While a discovery runs
    // (Steam keeps one running), the adapter holds back advertisements it has seen, so the
    // connection may never come about. Ending Doppelpad's discovery starts the scan over;
    // stopping it before connecting failed as often ("le-connection-abort-by-local").
    stop_discovery(bus, adapter);
    while !socket.connected(until.min(Instant::now() + RESCAN_WAIT))? {
        if Instant::now() >= until {
            return Err(Failure("the controller did not connect in time".into()));
        }
        tracing::debug!("Bluetooth: not connected yet, starting the scan over");
        discover(bus, adapter);
        stop_discovery(bus, adapter);
    }
    tracing::debug!(
        "Bluetooth: connected in {} ms, reading the controller",
        started.elapsed().as_millis()
    );
    let handles = set_up(&mut socket, advert.model, until)?;
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
    let mut ready = Ready::new(&key, advert, io, commands);
    ready.address = u64::from_str_radix(&key, 16).ok();
    Ok(Opened {
        feed,
        ready,
        socket,
        handles,
    })
}

/// Where a controller's characteristics have their values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Handles {
    input: u16,
    reply: u16,
    command: u16,
    vibration: Option<u16>,
}

/// Exchange the MTU, find the controller's characteristics and switch input and reply
/// notifications on.
fn set_up(
    bearer: &mut dyn att::Bearer,
    model: Switch2Model,
    until: Instant,
) -> Result<Handles, Failure> {
    let mtu = att::exchange_mtu(bearer, until)?;
    if mtu < INPUT_LEN + 3 {
        return Err(Failure(format!("its MTU stays at {mtu}")));
    }
    let uuid = |text| att::uuid(text).expect("a well-formed UUID");
    let service = att::services(bearer, until)?
        .into_iter()
        .find(|s| s.uuid == uuid(switch2ble::SERVICE))
        .ok_or_else(|| Failure("it lacks its input service".into()))?;
    let wanted = [
        switch2ble::INPUT,
        switch2ble::COMMAND,
        switch2ble::REPLY,
        switch2ble::vibration(model),
    ]
    .map(uuid);
    let found = att::characteristics(
        bearer,
        service.start,
        service.end,
        |found| wanted.iter().all(|w| found.iter().any(|c| c.uuid == *w)),
        until,
    )?;
    let value = |text| {
        let u = uuid(text);
        found.iter().find(|c| c.uuid == u).map(|c| c.value)
    };
    let (Some(input), Some(command), Some(reply)) = (
        value(switch2ble::INPUT),
        value(switch2ble::COMMAND),
        value(switch2ble::REPLY),
    ) else {
        return Err(Failure("it lacks input or command characteristics".into()));
    };
    for handle in [input, reply] {
        let Some(configuration) = att::configuration(bearer, handle, service.end, until)? else {
            return Err(Failure("its input cannot be switched on".into()));
        };
        att::notify(bearer, configuration, until)?;
    }
    Ok(Handles {
        input,
        reply,
        command,
        vibration: value(switch2ble::vibration(model)),
    })
}

/// Read a controller's input and replies, and answer what it asks, until it is let go
/// (`None`) or the link fails (why).
fn read_input(
    feed: &Feed,
    socket: &AttSocket,
    handles: Handles,
    stop: &AtomicBool,
) -> Option<&'static str> {
    while !stop.load(Ordering::Relaxed) {
        match socket.recv(Instant::now() + INPUT_WAIT) {
            Ok(Some(pdu)) => match att::incoming(&pdu) {
                att::Incoming::Notification { handle, value } if handle == handles.input => {
                    feed.input(value.to_vec());
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
            return Some("no input for 3 s");
        }
    }
    None
}

/// The controller is gone, or is let go: its device thread stops, and its link ends.
fn let_go(path: &str, why: &str) {
    let peer = {
        let mut state = central().state.lock().unwrap();
        // To find it again when it comes back in pairing mode.
        state.passed.remove(path);
        let peer = state.peers.remove(path);
        if let Some(Peer::Up { .. }) = &peer
            && let Some(key) = key(path)
        {
            state.links.remove(&switch2gatt::path(&key));
        }
        peer
    };
    // Being set up: `connect` sees it is gone.
    if let Some(Peer::Up { feed, socket, stop }) = peer {
        tracing::info!("Bluetooth: Switch 2 controller disconnected: {why}");
        stop.store(true, Ordering::Relaxed);
        feed.close();
        socket.shutdown();
    }
}

/// Let every controller go.
pub(super) fn let_all_go() {
    let Some(c) = CENTRAL.get() else {
        return;
    };
    let paths: Vec<String> = c.state.lock().unwrap().peers.keys().cloned().collect();
    for path in paths {
        let_go(&path, "Doppelpad stops");
    }
}

fn stop_discovery(bus: &Connection, adapter: &str) {
    if let Err(e) = bus.call_method(Some(BLUEZ), adapter, Some(ADAPTER), "StopDiscovery", &()) {
        tracing::debug!(adapter, "Bluetooth: cannot stop the discovery: {e}");
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

/// `struct sockaddr_l2` of `<bluetooth/l2cap.h>`.
#[repr(C)]
struct SockaddrL2 {
    family: libc::sa_family_t,
    psm: u16,
    /// Least significant byte first.
    bdaddr: [u8; 6],
    cid: u16,
    bdaddr_type: u8,
}

const BTPROTO_L2CAP: libc::c_int = 0;
const BDADDR_LE_PUBLIC: u8 = 1;
const BDADDR_LE_RANDOM: u8 = 2;

impl SockaddrL2 {
    /// The ATT channel of a Bluetooth LE device (`random`: its address is a random one).
    fn att((bdaddr, random): ([u8; 6], bool)) -> SockaddrL2 {
        SockaddrL2 {
            family: libc::AF_BLUETOOTH as libc::sa_family_t,
            psm: 0,
            bdaddr,
            cid: att::CID.to_le(),
            bdaddr_type: if random {
                BDADDR_LE_RANDOM
            } else {
                BDADDR_LE_PUBLIC
            },
        }
    }
}

/// An L2CAP socket on a controller's ATT channel, which needs no privileges. It keeps PDUs
/// apart; it is non-blocking, and every wait has a deadline.
struct AttSocket(OwnedFd);

impl AttSocket {
    /// Start connecting from the adapter `local` to the device `remote` (address, random);
    /// [`AttSocket::connected`] tells when it is done.
    fn connect(local: ([u8; 6], bool), remote: ([u8; 6], bool)) -> io::Result<Self> {
        // SAFETY: plain socket creation; the descriptor is owned at once.
        let fd = unsafe {
            libc::socket(
                libc::AF_BLUETOOTH,
                libc::SOCK_SEQPACKET | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                BTPROTO_L2CAP,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a descriptor that was just created and is owned by nothing else.
        let socket = AttSocket(unsafe { OwnedFd::from_raw_fd(fd) });
        let len = std::mem::size_of::<SockaddrL2>() as libc::socklen_t;
        let (local, remote) = (SockaddrL2::att(local), SockaddrL2::att(remote));
        // SAFETY: a valid address of the given length, for a socket of its family.
        if unsafe { libc::bind(fd, (&raw const local).cast(), len) } < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: as above.
        if unsafe { libc::connect(fd, (&raw const remote).cast(), len) } < 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() != Some(libc::EINPROGRESS) {
                return Err(e);
            }
        }
        Ok(socket)
    }

    /// Whether the connection is made, waiting until `until`; an error if it failed.
    fn connected(&self, until: Instant) -> io::Result<bool> {
        if !self.wait(libc::POLLOUT, until)? {
            return Ok(false);
        }
        let mut error: libc::c_int = 0;
        let mut size = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
        // SAFETY: an int-sized buffer for SO_ERROR.
        let read = unsafe {
            libc::getsockopt(
                self.0.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                (&raw mut error).cast(),
                &raw mut size,
            )
        };
        if read < 0 {
            return Err(io::Error::last_os_error());
        }
        if error != 0 {
            return Err(io::Error::from_raw_os_error(error));
        }
        Ok(true)
    }

    /// Wait until the socket is ready for `events`; `false` once `until` passed.
    fn wait(&self, events: libc::c_short, until: Instant) -> io::Result<bool> {
        loop {
            let left = until.saturating_duration_since(Instant::now());
            let mut fd = libc::pollfd {
                fd: self.0.as_raw_fd(),
                events,
                revents: 0,
            };
            // Round up, so a wait of less than a millisecond still waits.
            let ms = left.as_micros().div_ceil(1000).min(i32::MAX as u128) as libc::c_int;
            // SAFETY: one valid pollfd that lives through the call.
            let n = unsafe { libc::poll(&raw mut fd, 1, ms) };
            if n < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            }
            // An error or hang-up shows in the call that follows.
            return Ok(n > 0);
        }
    }

    /// Send a PDU, waiting until `until` for room.
    fn send(&self, pdu: &[u8], until: Instant) -> io::Result<()> {
        loop {
            if self.try_send(pdu)? {
                return Ok(());
            }
            if !self.wait(libc::POLLOUT, until)? {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "no room to send to the controller",
                ));
            }
        }
    }

    /// Send a PDU now, or not at all (`false`: no room).
    fn try_send(&self, pdu: &[u8]) -> io::Result<bool> {
        // SAFETY: sending from a buffer of this length.
        let n = unsafe {
            libc::send(
                self.0.as_raw_fd(),
                pdu.as_ptr().cast(),
                pdu.len(),
                libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL,
            )
        };
        if n >= 0 {
            return Ok(true);
        }
        let e = io::Error::last_os_error();
        match e.kind() {
            io::ErrorKind::WouldBlock => Ok(false),
            io::ErrorKind::Interrupted => self.try_send(pdu),
            _ => Err(e),
        }
    }

    /// The next PDU, or `None` once `until` passed. An error means the link is gone.
    fn recv(&self, until: Instant) -> io::Result<Option<Vec<u8>>> {
        let mut buf = vec![0u8; usize::from(att::MTU)];
        loop {
            // SAFETY: receiving into a buffer of this length.
            let n = unsafe {
                libc::recv(
                    self.0.as_raw_fd(),
                    buf.as_mut_ptr().cast(),
                    buf.len(),
                    libc::MSG_DONTWAIT,
                )
            };
            if n > 0 {
                buf.truncate(n as usize);
                return Ok(Some(buf));
            }
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            let e = io::Error::last_os_error();
            match e.kind() {
                io::ErrorKind::WouldBlock => {
                    if !self.wait(libc::POLLIN, until)? {
                        return Ok(None);
                    }
                }
                io::ErrorKind::Interrupted => {}
                _ => return Err(e),
            }
        }
    }

    /// End the link, also for whoever else holds the socket.
    fn shutdown(&self) {
        // SAFETY: shutting down a socket this value owns.
        unsafe { libc::shutdown(self.0.as_raw_fd(), libc::SHUT_RDWR) };
    }
}

impl att::Bearer for AttSocket {
    fn send(&mut self, pdu: &[u8]) -> io::Result<()> {
        AttSocket::send(self, pdu, Instant::now() + WRITE_TIMEOUT)
    }

    fn recv(&mut self, until: Instant) -> io::Result<Option<Vec<u8>>> {
        AttSocket::recv(self, until)
    }
}

fn managed_objects(bus: &Connection) -> zbus::Result<Objects> {
    bus.call_method(
        Some(BLUEZ),
        "/",
        Some(OBJECT_MANAGER),
        "GetManagedObjects",
        &(),
    )?
    .body()
    .deserialize()
}

fn text_property(bus: &Connection, path: &str, iface: &str, name: &str) -> zbus::Result<String> {
    let value: OwnedValue = bus
        .call_method(Some(BLUEZ), path, Some(PROPERTIES), "Get", &(iface, name))?
        .body()
        .deserialize()?;
    Ok(String::try_from(value)?)
}

/// The name of the D-Bus error BlueZ (or the bus) answered with.
fn error_name(e: &zbus::Error) -> Option<&str> {
    match e {
        zbus::Error::MethodError(name, _, _) => Some(name.as_str()),
        _ => None,
    }
}

fn flag(props: &Properties, name: &str) -> bool {
    props
        .get(name)
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}

/// The Switch 2 controller a device's manufacturer data (`a{qv}`, by company) describes.
fn advert(manufacturer: &Value<'_>) -> Option<Advert> {
    let Value::Dict(by_company) = inner(manufacturer) else {
        return None;
    };
    let data = by_company.iter().find_map(|(company, data)| {
        matches!(inner(company), Value::U16(id) if *id == switch2ble::COMPANY_ID)
            .then(|| bytes(data))?
    })?;
    // BlueZ takes the company identifier off the front.
    let mut full = switch2ble::COMPANY_ID.to_le_bytes().to_vec();
    full.extend(data);
    switch2ble::parse_advert(&full)
}

/// A value inside its variants.
fn inner<'v>(v: &'v Value<'v>) -> &'v Value<'v> {
    match v {
        Value::Value(boxed) => inner(boxed),
        _ => v,
    }
}

fn bytes(v: &Value<'_>) -> Option<Vec<u8>> {
    let Value::Array(array) = inner(v) else {
        return None;
    };
    array
        .iter()
        .map(|b| match b {
            Value::U8(b) => Some(*b),
            _ => None,
        })
        .collect()
}

/// The name a controller is known by: its address (as on Windows), from BlueZ's object path
/// (`…/dev_98_B6_E9_12_34_56`).
fn key(path: &str) -> Option<String> {
    let address = path.rsplit('/').next()?.strip_prefix("dev_")?;
    let hex: String = address.split('_').collect::<String>().to_ascii_lowercase();
    (hex.len() == 12 && hex.chars().all(|c| c.is_ascii_hexdigit())).then_some(hex)
}

/// The adapter a device's object path is under.
fn adapter_of(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(adapter, _)| adapter)
}

/// An address as BlueZ writes it (`98:B6:E9:12:34:56`), least significant byte first as the
/// kernel takes it.
fn bdaddr(text: &str) -> Option<[u8; 6]> {
    let mut out = [0u8; 6];
    let mut parts = text.split(':');
    for byte in out.iter_mut().rev() {
        let part = parts.next()?;
        if part.len() != 2 {
            return None;
        }
        *byte = u8::from_str_radix(part, 16).ok()?;
    }
    parts.next().is_none().then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::att::testing::ScriptBearer;
    use crate::nintendo::pid;

    /// A Pro Controller 2's manufacturer data as BlueZ shows it: without the company
    /// identifier, in a variant.
    fn manufacturer_data(pairing: bool) -> Value<'static> {
        let mut data: Vec<u8> = vec![
            0x01, 0x00, 0x03, 0x7E, 0x05, 0x69, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x0F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        if !pairing {
            data[10..16].copy_from_slice(&[0x5F, 0x11, 0x85, 0xEB, 0xF1, 0x48]);
        }
        Value::from(HashMap::from([
            (switch2ble::COMPANY_ID, Value::new(data)),
            (0x004C_u16, Value::new(vec![0x10_u8, 0x05])),
        ]))
    }

    #[test]
    fn adverts_from_bluez() {
        let pro = Advert {
            pid: pid::SWITCH2_PRO,
            model: Switch2Model::Pro,
            pairing: true,
        };
        assert_eq!(advert(&manufacturer_data(true)), Some(pro));
        assert!(!advert(&manufacturer_data(false)).unwrap().pairing);
        // As a property, the dictionary sits in a variant of its own.
        assert_eq!(advert(&Value::new(manufacturer_data(true))), Some(pro));

        let apple = Value::from(HashMap::from([(0x004C_u16, Value::new(vec![0x10_u8]))]));
        assert_eq!(advert(&apple), None);
        assert_eq!(advert(&Value::from("no dictionary")), None);
    }

    /// The manufacturer data survives D-Bus: serialised as BlueZ sends it, and read back as
    /// `on_device` gets it.
    #[test]
    fn adverts_through_dbus() {
        let props = HashMap::from([("ManufacturerData", manufacturer_data(true))]);
        let msg = zbus::Message::signal(
            "/org/bluez/hci0/dev_98_B6_E9_12_34_56",
            PROPERTIES,
            "PropertiesChanged",
        )
        .unwrap()
        .build(&(DEVICE, props, Vec::<String>::new()))
        .unwrap();
        let (_, changed, _): (String, Properties, Vec<String>) = msg.body().deserialize().unwrap();
        assert!(advert(&changed["ManufacturerData"]).unwrap().pairing);
    }

    #[test]
    fn names_and_addresses() {
        let path = "/org/bluez/hci0/dev_98_B6_E9_12_34_56";
        assert_eq!(key(path).as_deref(), Some("98b6e9123456"));
        assert_eq!(key("/org/bluez/hci0"), None);
        assert_eq!(key(&format!("{path}/service0010")), None);
        assert_eq!(key("/org/bluez/hci0/dev_98_B6"), None);
        assert_eq!(adapter_of(path), "/org/bluez/hci0");

        assert_eq!(
            bdaddr("98:B6:E9:12:34:56"),
            Some([0x56, 0x34, 0x12, 0xE9, 0xB6, 0x98])
        );
        assert_eq!(bdaddr("98:B6:E9:12:34"), None);
        assert_eq!(bdaddr("98:B6:E9:12:34:56:78"), None);
        assert_eq!(bdaddr("98:B6:E9:12:34:5"), None);
    }

    #[test]
    fn socket_addresses_are_the_kernels() {
        assert_eq!(std::mem::size_of::<SockaddrL2>(), 14);
        let a = SockaddrL2::att(([1, 2, 3, 4, 5, 6], true));
        assert_eq!(a.cid, 4u16.to_le());
        assert_eq!(a.bdaddr_type, BDADDR_LE_RANDOM);
    }

    fn uuid_le(text: &str) -> Vec<u8> {
        att::uuid(text).unwrap().into_iter().rev().collect()
    }

    /// One characteristic declaration per answer, as the controller sends them.
    fn declaration(decl: u16, props: u8, value: u16, uuid: &str) -> Vec<Vec<u8>> {
        let mut a = vec![0x09, 21];
        a.extend(decl.to_le_bytes());
        a.push(props);
        a.extend(value.to_le_bytes());
        a.extend(uuid_le(uuid));
        vec![a]
    }

    /// Set up as a Pro Controller 2 answered, with its handles.
    #[test]
    fn set_up_a_pro_controller() {
        let mut services = vec![0x11, 20];
        for (start, end, uuid) in [
            (0x01_u16, 0x07_u16, "00C5AF5D-1964-4E30-8F51-1956F96BD280"),
            (0x08, 0x32, switch2ble::SERVICE),
        ] {
            services.extend(start.to_le_bytes());
            services.extend(end.to_le_bytes());
            services.extend(uuid_le(uuid));
        }
        let other = "7492866C-EC3E-4619-8258-32755FFCC0F8";
        let mut b = ScriptBearer::new(vec![
            vec![vec![0x03, 0x00, 0x02]],
            vec![services],
            vec![vec![0x11, 6, 0x33, 0, 0x38, 0, 0x00, 0x18]],
            vec![vec![0x01, 0x10, 0x39, 0x00, 0x0A]],
            declaration(0x09, 0x12, 0x0A, switch2ble::INPUT),
            declaration(0x0D, 0x12, 0x0E, other),
            declaration(0x11, 0x04, 0x12, switch2ble::vibration(Switch2Model::Pro)),
            declaration(0x13, 0x04, 0x14, switch2ble::COMMAND),
            declaration(0x15, 0x04, 0x16, other),
            declaration(0x17, 0x04, 0x18, other),
            declaration(0x19, 0x10, 0x1A, switch2ble::REPLY),
            vec![vec![0x05, 1, 0x0B, 0x00, 0x02, 0x29]],
            vec![vec![0x13]],
            vec![vec![0x05, 1, 0x1B, 0x00, 0x02, 0x29]],
            // Input already flows.
            vec![vec![0x1B, 0x0A, 0x00, 0xAA], vec![0x13]],
        ]);
        let until = Instant::now() + Duration::from_secs(1);
        let Ok(handles) = set_up(&mut b, Switch2Model::Pro, until) else {
            panic!("not set up: {:02x?}", b.sent);
        };
        assert_eq!(
            handles,
            Handles {
                input: 0x0A,
                reply: 0x1A,
                command: 0x14,
                vibration: Some(0x12),
            }
        );
        // No characteristic is looked for after the reply's.
        assert!(b.answers.is_empty());
        let writes: Vec<&Vec<u8>> = b.sent.iter().filter(|p| p[0] == 0x12).collect();
        assert_eq!(
            writes,
            [&vec![0x12, 0x0B, 0, 1, 0], &vec![0x12, 0x1B, 0, 1, 0]]
        );
    }

    #[test]
    fn set_up_needs_room_for_input() {
        let mut b = ScriptBearer::new(vec![vec![vec![0x03, 0x17, 0x00]]]);
        let until = Instant::now() + Duration::from_secs(1);
        let Err(Failure(why)) = set_up(&mut b, Switch2Model::Pro, until) else {
            panic!("set up with a 23-byte MTU");
        };
        assert_eq!(why, "its MTU stays at 23");
    }

    #[test]
    fn errors_have_names() {
        let call = zbus::Message::method_call("/", "Ping")
            .unwrap()
            .build(&())
            .unwrap();
        let error = zbus::Message::error(&call.header(), "org.bluez.Error.InProgress")
            .unwrap()
            .build(&("Operation already in progress",))
            .unwrap();
        let e = zbus::Error::from(error);
        assert_eq!(error_name(&e), Some("org.bluez.Error.InProgress"));
        assert_eq!(error_name(&zbus::Error::Unsupported), None);
    }

    /// Runs a discovery for 10 s and prints the Switch 2 controllers BlueZ reports, with each
    /// signal that told of one.
    #[test]
    #[ignore = "listens to Bluetooth on this computer; a Switch 2 controller in pairing mode shows up"]
    fn advertisements_on_this_computer() {
        let bus = bus().expect("the system bus");
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender(BLUEZ)
            .unwrap()
            .build();
        let signals = MessageIterator::for_match_rule(rule, &bus, Some(1024)).unwrap();
        let objects = managed_objects(&bus).expect("BlueZ answers");
        for (path, ifaces) in &objects {
            if let Some(m) = ifaces.get(DEVICE).and_then(|d| d.get("ManufacturerData")) {
                println!("known: {path}: {:?}", advert(m));
            }
            if ifaces.contains_key(ADAPTER) {
                discover(&bus, path.as_str());
            }
        }
        let counted = Arc::new(Mutex::new((0usize, Vec::new())));
        let log = counted.clone();
        let started = Instant::now();
        std::thread::spawn(move || {
            for signal in signals.flatten() {
                let header = signal.header();
                let (Some(path), Some(member)) = (header.path(), header.member()) else {
                    continue;
                };
                let props = match member.as_str() {
                    "PropertiesChanged" => signal
                        .body()
                        .deserialize::<(String, Properties, Vec<String>)>()
                        .ok()
                        .filter(|(iface, ..)| iface == DEVICE)
                        .map(|(_, p, _)| p),
                    "InterfacesAdded" => signal
                        .body()
                        .deserialize::<(OwnedObjectPath, HashMap<String, Properties>)>()
                        .ok()
                        .and_then(|(_, mut i)| i.remove(DEVICE)),
                    _ => None,
                };
                let mut log = log.lock().unwrap();
                log.0 += 1;
                if let Some(a) = props
                    .as_ref()
                    .and_then(|p| p.get("ManufacturerData"))
                    .and_then(|m| advert(m))
                {
                    log.1.push(format!(
                        "{:5.2} s {member} {path}: {a:?}",
                        started.elapsed().as_secs_f32()
                    ));
                }
            }
        });
        std::thread::sleep(Duration::from_secs(10));
        let (count, switch2) = &*counted.lock().unwrap();
        println!("{count} signals from BlueZ");
        for line in switch2 {
            println!("  {line}");
        }
    }

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
        let mut backend = super::super::hid::backend();
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
        switch2gatt::drive_controller(poll);
        let_all_go();
    }
}
