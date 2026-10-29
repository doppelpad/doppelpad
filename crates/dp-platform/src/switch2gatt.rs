//! Switch 2 controllers over Bluetooth LE, between an OS's Bluetooth client and the HID
//! backend. The client (`windows::ble`, `macos::ble`, `linux::ble`) connects a controller,
//! feeds its notifications into a [`Feed`] and carries writes out through a [`Writer`]; its
//! device thread sees the input as HID reports ([`BleIo`]) and talks to it over a command
//! channel ([`GattCommands`]), as over USB. What the controller sends and takes is in
//! [`crate::nintendo::switch2ble`].
//!
//! What every client does alike lives here too: which controllers it connects
//! ([`Peers::admit`]), its controllers from discovery until they are let go ([`Peers`]), and
//! its timing. How a client connects one (raw ATT, WinRT, CoreBluetooth) stays its own.
//!
//! It also compiles in tests on other OSes, so it is unit-tested everywhere.

#![cfg_attr(
    not(any(windows, target_os = "linux", target_os = "macos")),
    allow(dead_code)
)]

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TrySendError};

use crate::bdaddr::BdAddr;
use crate::hidhost::{Devices, Found, HidHandle, Wait, Wake, read_via_wait};
use crate::nintendo::switch2::Switch2Driver;
use crate::nintendo::switch2ble::{self, Advert};
use crate::nintendo::{NINTENDO_VID, Switch2Model};
use crate::padio::{BulkIo, ReportIo};
use crate::{DeviceId, Transport};

/// Input reports a device thread may fall behind by before new ones are dropped.
const QUEUED_REPORTS: usize = 64;

/// Give up on a controller that has not connected and answered by then.
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a command may take to go out.
#[cfg_attr(target_os = "macos", allow(dead_code))] // CoreBluetooth queues them itself
pub(crate) const WRITE_TIMEOUT: Duration = Duration::from_secs(1);
/// A controller whose input stopped for this long is let go. It sends all the time (at the
/// least about 17 reports a second, even with nothing touched), while an OS may notice one
/// that was turned off only about 10 s later (Windows).
pub(crate) const SILENCE: Duration = Duration::from_secs(3);

/// The characteristic a write goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Command,
    Vibration,
}

/// The client's side of the writes, all without response. A rumble write the link has no room
/// for may be dropped (rumble comes again soon); commands are worth the wait.
pub(crate) trait Writer: Send + Sync {
    fn write(&self, target: Target, data: Vec<u8>);
}

/// What the client and a controller's device thread share.
struct Shared {
    gone: AtomicBool,
    /// When the last input notification arrived (at first: when the link was made).
    last_input: Mutex<Instant>,
    writer: Box<dyn Writer>,
}

impl Shared {
    fn check(&self) -> io::Result<()> {
        if self.gone.load(Ordering::Relaxed) {
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "the Bluetooth link is gone",
            ))
        } else {
            Ok(())
        }
    }
}

enum Event {
    Report(Vec<u8>),
    Wake,
}

/// A new link to a connected controller: the client's end, and the device thread's two.
pub(crate) fn link(writer: Box<dyn Writer>) -> (Feed, BleIo, GattCommands) {
    let shared = Arc::new(Shared {
        gone: AtomicBool::new(false),
        last_input: Mutex::new(Instant::now()),
        writer,
    });
    let (events, events_rx) = crossbeam_channel::bounded(QUEUED_REPORTS);
    let (replies, replies_rx) = crossbeam_channel::bounded(QUEUED_REPORTS);
    let io = BleIo {
        shared: shared.clone(),
        events: events_rx,
        waker: Arc::new(BleWaker(events.clone())),
        report: Vec::new(),
    };
    let commands = GattCommands {
        shared: shared.clone(),
        replies: replies_rx,
    };
    let feed = Feed {
        shared,
        events,
        replies,
    };
    (feed, io, commands)
}

/// The client's end of a link: what the controller notifies goes in here.
#[derive(Clone)]
pub(crate) struct Feed {
    shared: Arc<Shared>,
    events: Sender<Event>,
    replies: Sender<Vec<u8>>,
}

impl Feed {
    /// An input notification. `false` once nobody reads the link (its device thread ended,
    /// or never started): the client lets the controller go then, so that it can come back.
    /// Until then such input counts as none (see [`Feed::silent_for`]).
    pub fn input(&self, value: Vec<u8>) -> bool {
        if let Err(TrySendError::Disconnected(_)) = self.events.try_send(Event::Report(value)) {
            return false;
        }
        *self.shared.last_input.lock().unwrap() = Instant::now();
        true
    }

    /// How long no input has arrived. The controllers send all the time, so a long silence
    /// means the link is gone, even before the OS says so.
    pub fn silent_for(&self) -> Duration {
        self.shared.last_input.lock().unwrap().elapsed()
    }

    /// A command reply notification.
    pub fn reply(&self, value: Vec<u8>) {
        let _ = self.replies.try_send(value);
    }

    /// The link is gone: the device thread stops, and writes fail.
    pub fn close(&self) {
        self.shared.gone.store(true, Ordering::Relaxed);
        let _ = self.events.try_send(Event::Wake);
    }
}

/// A connected controller's reports, for its device thread.
pub(crate) struct BleIo {
    shared: Arc<Shared>,
    events: Receiver<Event>,
    waker: Arc<BleWaker>,
    /// The last input report, its ID in front.
    report: Vec<u8>,
}

struct BleWaker(Sender<Event>);

impl Wake for BleWaker {
    fn wake(&self) {
        let _ = self.0.try_send(Event::Wake);
    }
}

impl HidHandle for BleIo {
    fn wait(&mut self, timeout: Duration) -> io::Result<Wait> {
        self.shared.check()?;
        let event = self.events.recv_timeout(timeout);
        self.shared.check()?;
        match event {
            Ok(Event::Report(value)) => {
                switch2ble::input_report(&value, &mut self.report);
                Ok(Wait::Report)
            }
            Ok(Event::Wake) => Ok(Wait::Woken),
            Err(RecvTimeoutError::Timeout) => Ok(Wait::Timeout),
            Err(RecvTimeoutError::Disconnected) => Err(io::ErrorKind::NotConnected.into()),
        }
    }

    fn report(&self) -> &[u8] {
        &self.report
    }

    fn waker(&self) -> Arc<dyn Wake> {
        self.waker.clone()
    }
}

impl ReportIo for BleIo {
    fn write(&mut self, report: &[u8]) -> io::Result<()> {
        self.shared.check()?;
        self.shared
            .writer
            .write(Target::Vibration, switch2ble::output_report(report));
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        read_via_wait(self, buf, timeout)
    }
}

/// A connected controller's command channel: commands out, replies back as notifications.
pub(crate) struct GattCommands {
    shared: Arc<Shared>,
    replies: Receiver<Vec<u8>>,
}

impl BulkIo for GattCommands {
    /// The reply is told from others by the command and subcommand it repeats (`cmd[0]` and
    /// `cmd[3]`), so a command shorter than that is refused.
    fn command(&mut self, cmd: &[u8], reply_len: usize, timeout: Duration) -> io::Result<Vec<u8>> {
        let (Some(&command), Some(&subcommand)) = (cmd.first(), cmd.get(3)) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a command has at least four bytes",
            ));
        };
        self.shared.check()?;
        // Late replies to commands that timed out.
        while self.replies.try_recv().is_ok() {}
        self.shared.writer.write(Target::Command, cmd.to_vec());
        let until = Instant::now() + timeout;
        loop {
            match self
                .replies
                .recv_timeout(until.saturating_duration_since(Instant::now()))
            {
                Ok(mut reply)
                    if reply.first() == Some(&command) && reply.get(3) == Some(&subcommand) =>
                {
                    reply.truncate(reply_len);
                    return Ok(reply);
                }
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => return Err(io::ErrorKind::TimedOut.into()),
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(io::ErrorKind::NotConnected.into());
                }
            }
        }
    }
}

/// A connected controller, set up and waiting for its device thread.
pub(crate) struct Ready {
    pub path: String,
    pub id: String,
    pub advert: Advert,
    /// The controller's Bluetooth address, when the client can end its link by it, which turns
    /// the controller off (the backend's `disconnect`, as for Sony pads); only on Linux yet.
    pub address: Option<BdAddr>,
    pub io: BleIo,
    pub commands: GattCommands,
}

impl Ready {
    /// `key` is what the OS knows the controller by, stable across reconnects.
    pub fn new(key: &str, advert: Advert, io: BleIo, commands: GattCommands) -> Ready {
        Ready {
            path: path(key),
            id: format!("ble:{NINTENDO_VID:04x}:{:04x}:{key}", advert.pid),
            advert,
            address: None,
            io,
            commands,
        }
    }

    /// Start the controller's device thread. It has nothing that hides it: no other program
    /// sees a controller on a link of Doppelpad's own.
    pub fn start(self, devices: &Devices) {
        let found = Found {
            path: self.path,
            id: DeviceId(self.id),
            transport: Transport::Bluetooth,
            label: "Bluetooth",
            vid: NINTENDO_VID,
            pid: self.advert.pid,
            instance_paths: Vec::new(),
            bt_address: self.address,
        };
        let (model, commands) = (self.advert.model, self.commands);
        devices.start(
            self.io,
            found,
            Box::new(move |_io| Box::new(Switch2Driver::bluetooth(model, Box::new(commands)))),
        );
    }
}

/// The path a controller is listed under, by what the OS knows it by.
pub(crate) fn path(key: &str) -> String {
    format!("ble:{key}")
}

/// The controllers that are connected, by path, and those ready for the scanner to start.
#[derive(Default)]
pub(crate) struct Poll {
    pub connected: HashSet<String>,
    pub ready: Vec<Ready>,
}

impl Poll {
    /// Whether the controller listed under `path` is still connected.
    pub fn has(&self, path: &str) -> bool {
        self.connected.contains(path)
    }

    /// Start the controllers that are ready.
    pub fn start(self, devices: &Devices) {
        for pad in self.ready {
            pad.start(devices);
        }
    }
}

/// A client's connected controllers, as the scanner polls them.
#[derive(Default)]
pub(crate) struct Links {
    /// Paths of the controllers that are connected and set up.
    connected: HashSet<String>,
    ready: Vec<Ready>,
}

impl Links {
    pub fn add(&mut self, ready: Ready) {
        self.connected.insert(ready.path.clone());
        self.ready.push(ready);
    }

    pub fn remove(&mut self, path: &str) {
        self.connected.remove(path);
        self.ready.retain(|r| r.path != path);
    }

    /// What changed since the last call.
    pub fn poll(&mut self) -> Poll {
        Poll {
            connected: self.connected.clone(),
            ready: std::mem::take(&mut self.ready),
        }
    }
}

/// One attempt to connect a controller. A controller let go and found again is connected by a
/// new one, so that whatever is left of an old attempt (a set-up thread, a timeout, a callback)
/// cannot take the new one's place or end it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Attempt(u64);

/// One controller, from discovery until it is let go.
enum Peer<C, S> {
    /// Being set up; `setup` is what the client keeps meanwhile (nothing where a thread of its
    /// own sets it up).
    Connecting { attempt: Attempt, setup: S },
    /// Connected, set up and handed to the scanner.
    Up {
        attempt: Attempt,
        feed: Feed,
        conn: C,
        /// The path it is listed under.
        path: String,
    },
}

impl<C, S> Peer<C, S> {
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    fn attempt(&self) -> Attempt {
        match self {
            Peer::Connecting { attempt, .. } | Peer::Up { attempt, .. } => *attempt,
        }
    }
}

/// A controller that was let go, with what kept it.
pub(crate) enum Gone<C, S> {
    Connecting(S),
    Up(C),
}

/// A client's controllers, by what the OS knows them by (`K`), from discovery until they are
/// let go, and the links of those that are up. `C` keeps a controller connected; `S` is what
/// the client keeps while one is set up.
pub(crate) struct Peers<K, C, S = ()> {
    peers: HashMap<K, Peer<C, S>>,
    /// Controllers already logged as not wanted.
    passed: HashSet<K>,
    links: Links,
    attempts: u64,
}

impl<K, C, S> Default for Peers<K, C, S> {
    fn default() -> Self {
        Peers {
            peers: HashMap::new(),
            passed: HashSet::new(),
            links: Links::default(),
            attempts: 0,
        }
    }
}

impl<K: Clone + Eq + Hash, C, S> Peers<K, C, S> {
    /// Whether to connect the controller `key` that advertised `advert`, while the search is
    /// `searching`: only one that is not connected or being connected yet, in pairing mode,
    /// and a Pro Controller (the only model read over Bluetooth yet). A controller looking for
    /// the console it is paired with is left alone: taking it would steal it. Others' discoveries
    /// (Steam's, a desktop's settings) still bring advertisements in while the search is off.
    pub fn admit(&mut self, key: &K, advert: &Advert, searching: bool) -> bool {
        if self.peers.contains_key(key) || !searching {
            return false;
        }
        if !advert.pairing || advert.model != Switch2Model::Pro {
            if self.passed.insert(key.clone()) {
                if advert.pairing {
                    tracing::info!(
                        model = ?advert.model,
                        "Bluetooth: only the Switch 2 Pro Controller is supported over Bluetooth yet"
                    );
                } else {
                    tracing::debug!(
                        model = ?advert.model,
                        "Bluetooth: a Switch 2 controller looks for its console, left alone"
                    );
                }
            }
            return false;
        }
        tracing::info!("Bluetooth: Switch 2 Pro Controller in pairing mode, connecting");
        true
    }

    /// Start a new attempt to connect `key`, which the client sets up with `setup`.
    pub fn connecting(&mut self, key: K, setup: S) -> Attempt {
        self.attempts += 1;
        let attempt = Attempt(self.attempts);
        self.peers.insert(key, Peer::Connecting { attempt, setup });
        attempt
    }

    /// Whether `attempt` is still the one setting `key` up: it was not let go meanwhile.
    pub fn is_connecting(&self, key: &K, attempt: Attempt) -> bool {
        matches!(self.peers.get(key), Some(Peer::Connecting { attempt: a, .. }) if *a == attempt)
    }

    /// Whether any controller is being set up.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn any_connecting(&self) -> bool {
        self.peers
            .values()
            .any(|p| matches!(p, Peer::Connecting { .. }))
    }

    /// The attempt setting `key` up now, and what the client keeps meanwhile.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn setting_up(&mut self, key: &K) -> Option<(Attempt, &mut S)> {
        match self.peers.get_mut(key) {
            Some(Peer::Connecting { attempt, setup }) => Some((*attempt, setup)),
            _ => None,
        }
    }

    /// `attempt` set `key` up: it is up and its link goes to the scanner, unless the controller
    /// was let go meanwhile; then the connection comes back, to be dropped outside any lock
    /// (closing one may call back).
    #[cfg_attr(target_os = "macos", allow(dead_code))] // CoreBluetooth sets up on its queue
    pub fn up(
        &mut self,
        key: &K,
        attempt: Attempt,
        feed: Feed,
        conn: C,
        ready: Ready,
    ) -> Result<(), C> {
        if !self.is_connecting(key, attempt) {
            return Err(conn);
        }
        self.add_up(key.clone(), attempt, feed, conn, ready);
        Ok(())
    }

    /// As [`Peers::up`], for a client whose connection is what it kept while setting up:
    /// `link` turns that into the connection and its link. `false` when the controller was let
    /// go meanwhile.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn up_from(
        &mut self,
        key: &K,
        attempt: Attempt,
        link: impl FnOnce(S) -> (Feed, C, Ready),
    ) -> bool {
        if !self.is_connecting(key, attempt) {
            return false;
        }
        let Some(Peer::Connecting { setup, .. }) = self.peers.remove(key) else {
            return false;
        };
        let (feed, conn, ready) = link(setup);
        self.add_up(key.clone(), attempt, feed, conn, ready);
        true
    }

    fn add_up(&mut self, key: K, attempt: Attempt, feed: Feed, conn: C, ready: Ready) {
        tracing::info!("Bluetooth: Switch 2 Pro Controller connected");
        let path = ready.path.clone();
        self.links.add(ready);
        self.peers.insert(
            key,
            Peer::Up {
                attempt,
                feed,
                conn,
                path,
            },
        );
    }

    /// `attempt` could not set `key` up: it is forgotten, unless another attempt took over.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn failed(&mut self, key: &K, attempt: Attempt) {
        if self.is_connecting(key, attempt) {
            self.peers.remove(key);
        }
    }

    /// The link and connection of `key`, while it is up.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn up_conn(&self, key: &K) -> Option<(&Feed, &C)> {
        match self.peers.get(key) {
            Some(Peer::Up { feed, conn, .. }) => Some((feed, conn)),
            _ => None,
        }
    }

    /// The connection of `key`, while it is up.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn up_conn_mut(&mut self, key: &K) -> Option<&mut C> {
        match self.peers.get_mut(key) {
            Some(Peer::Up { conn, .. }) => Some(conn),
            _ => None,
        }
    }

    /// The connection of `key`, while `attempt` keeps it up.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn conn_mut(&mut self, key: &K, attempt: Attempt) -> Option<&mut C> {
        match self.peers.get_mut(key) {
            Some(Peer::Up {
                attempt: a, conn, ..
            }) if *a == attempt => Some(conn),
            _ => None,
        }
    }

    /// Whether the controller `key` is known (being set up, or up).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn has(&self, key: &K) -> bool {
        self.peers.contains_key(key)
    }

    /// The controllers that are up.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn up_keys(&self) -> impl Iterator<Item = &K> {
        self.peers.iter().filter_map(|(k, p)| match p {
            Peer::Up { .. } => Some(k),
            Peer::Connecting { .. } => None,
        })
    }

    /// The controllers that are up but sent no input for longer than [`SILENCE`] (or whose
    /// device thread stopped reading them).
    #[cfg_attr(target_os = "linux", allow(dead_code))] // its input thread watches
    pub fn silent(&self) -> Vec<K> {
        self.peers
            .iter()
            .filter_map(|(k, p)| match p {
                Peer::Up { feed, .. } if feed.silent_for() > SILENCE => Some(k.clone()),
                _ => None,
            })
            .collect()
    }

    /// Every controller known.
    pub fn keys(&self) -> Vec<K> {
        self.peers.keys().cloned().collect()
    }

    /// The controller is gone, or is let go: its device thread stops and its link is no longer
    /// listed. It will be connected again when it comes back in pairing mode. What kept it is
    /// handed back, to be ended outside any lock.
    pub fn let_go(&mut self, key: &K) -> Option<Gone<C, S>> {
        self.passed.remove(key);
        match self.peers.remove(key)? {
            Peer::Connecting { setup, .. } => Some(Gone::Connecting(setup)),
            Peer::Up {
                feed, conn, path, ..
            } => {
                feed.close();
                self.links.remove(&path);
                Some(Gone::Up(conn))
            }
        }
    }

    /// Like [`Peers::let_go`], only if `attempt` is the one that has `key`.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn let_go_attempt(&mut self, key: &K, attempt: Attempt) -> Option<Gone<C, S>> {
        if self.peers.get(key).map(Peer::attempt) != Some(attempt) {
            return None;
        }
        self.let_go(key)
    }

    /// What changed since the last call, for the scanner.
    pub fn poll(&mut self) -> Poll {
        self.links.poll()
    }
}

/// A hardware test of a client, the whole way without the rest of Doppelpad: waits for a Pro
/// Controller in pairing mode to come out of the client's `poll`, starts its driver and drives
/// it (see [`crate::hidhost::testing::drive`]), calling `poll` once a second as the scanner
/// does, so that a controller turned off is let go. Run the test with `--nocapture`, hold the
/// pairing button, then press buttons, move the sticks, pull the triggers, tilt it, and turn it
/// off at the end.
#[cfg(all(test, any(windows, target_os = "linux", target_os = "macos")))]
pub(crate) fn drive_controller(mut poll: impl FnMut() -> Poll) {
    println!("hold the pairing button until the lights run");
    let until = Instant::now() + Duration::from_secs(300);
    let mut pad = None;
    while pad.is_none() && Instant::now() < until {
        pad = poll().ready.pop();
        std::thread::sleep(Duration::from_millis(100));
    }
    let Ready {
        id,
        advert,
        mut io,
        commands,
        ..
    } = pad.expect("no controller in pairing mode connected");
    let driver = Switch2Driver::bluetooth(advert.model, Box::new(commands));
    crate::hidhost::testing::drive(&id, Box::new(driver), &mut io, || {
        let _ = poll();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nintendo::{Switch2Model, pid};

    /// Hands every write to the test.
    struct Wire(Sender<(Target, Vec<u8>)>);

    impl Writer for Wire {
        fn write(&self, target: Target, data: Vec<u8>) {
            let _ = self.0.send((target, data));
        }
    }

    fn wired() -> (Feed, BleIo, GattCommands, Receiver<(Target, Vec<u8>)>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let (feed, io, commands) = link(Box::new(Wire(tx)));
        (feed, io, commands, rx)
    }

    #[test]
    fn input_becomes_hid_reports() {
        let (feed, mut io, _commands, _wire) = wired();
        std::thread::sleep(Duration::from_millis(30));
        let before = feed.silent_for();
        feed.input(vec![0xAA; 63]);
        assert!(feed.silent_for() < before, "input ends the silence");
        assert!(matches!(
            io.wait(Duration::from_secs(1)).unwrap(),
            Wait::Report
        ));
        assert_eq!(io.report().len(), 64);
        assert_eq!(io.report()[..2], [0x05, 0xAA]);
        io.waker().wake();
        assert!(matches!(io.wait(Duration::ZERO).unwrap(), Wait::Woken));
        assert!(matches!(io.wait(Duration::ZERO).unwrap(), Wait::Timeout));

        let mut buf = [0u8; 8];
        feed.input(vec![0xBB; 63]);
        assert_eq!(io.read(&mut buf, Duration::from_secs(1)).unwrap(), Some(8));
        assert_eq!(buf[..2], [0x05, 0xBB]);
    }

    #[test]
    fn rumble_goes_to_the_vibration_characteristic() {
        let (_feed, mut io, _commands, wire) = wired();
        io.write(&[0x02, 0x51]).unwrap();
        let (target, data) = wire.try_recv().unwrap();
        assert_eq!(target, Target::Vibration);
        assert_eq!(data, switch2ble::output_report(&[0x02, 0x51]));
    }

    #[test]
    fn commands_wait_for_their_reply() {
        let (feed, _io, mut commands, wire) = wired();
        // A reply from before, which the command must not take for its own.
        feed.reply(vec![0x07, 0x01, 0x00, 0x02, 0xEE]);
        let replier = std::thread::spawn(move || {
            let (target, cmd) = wire.recv().unwrap();
            assert_eq!(target, Target::Command);
            // Someone else's reply first, then the one that repeats the command.
            feed.reply(vec![0x99, 0, 0, 0x99]);
            feed.reply(vec![cmd[0], 0x01, 0x00, cmd[3], 0xAB, 0xCD]);
        });
        let reply = commands
            .command(&[0x07, 0x91, 0x01, 0x02], 5, Duration::from_secs(1))
            .unwrap();
        assert_eq!(reply, [0x07, 0x01, 0x00, 0x02, 0xAB]);
        replier.join().unwrap();
    }

    #[test]
    fn unanswered_commands_time_out() {
        let (_feed, _io, mut commands, _wire) = wired();
        let e = commands
            .command(&[0x07, 0x91, 0x01, 0x02], 4, Duration::from_millis(20))
            .unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn a_closed_link_stops_the_device_thread() {
        let (feed, mut io, mut commands, wire) = wired();
        feed.close();
        assert_eq!(
            io.wait(Duration::from_secs(1)).err().map(|e| e.kind()),
            Some(io::ErrorKind::NotConnected)
        );
        assert!(io.write(&[0x02]).is_err());
        assert!(commands.command(&[0; 4], 4, Duration::ZERO).is_err());
        assert!(wire.try_recv().is_err(), "nothing written");
    }

    #[test]
    fn links_hand_out_ready_controllers_once() {
        let advert = Advert {
            pid: pid::SWITCH2_PRO,
            model: Switch2Model::Pro,
            pairing: true,
        };
        let mut links = Links::default();
        let (_feed, io, commands, _wire) = wired();
        let ready = Ready::new("98b6e9123456", advert, io, commands);
        assert_eq!(ready.path, "ble:98b6e9123456");
        assert_eq!(ready.id, "ble:057e:2069:98b6e9123456");
        links.add(ready);

        let first = links.poll();
        assert_eq!(first.ready.len(), 1);
        assert!(first.connected.contains("ble:98b6e9123456"));
        let second = links.poll();
        assert!(second.ready.is_empty());
        assert!(second.connected.contains("ble:98b6e9123456"));

        links.remove("ble:98b6e9123456");
        assert!(links.poll().connected.is_empty());
    }

    #[test]
    fn short_commands_are_refused() {
        let (_feed, _io, mut commands, wire) = wired();
        let e = commands
            .command(&[0x07, 0x91], 4, Duration::ZERO)
            .unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
        assert!(wire.try_recv().is_err(), "nothing written");
    }

    #[test]
    fn input_nobody_reads_is_no_input() {
        let (feed, io, _commands, _wire) = wired();
        assert!(feed.input(vec![0xAA; 63]));
        drop(io);
        let since = feed.silent_for();
        assert!(!feed.input(vec![0xAA; 63]), "the device thread is gone");
        assert!(feed.silent_for() >= since, "the silence goes on");
    }

    fn pro(pairing: bool) -> Advert {
        Advert {
            pid: pid::SWITCH2_PRO,
            model: Switch2Model::Pro,
            pairing,
        }
    }

    /// A controller set up by a client: its link's client end, connection (an id here) and
    /// ready ends, and the device thread's end the test keeps.
    fn set_up(key: &str, conn: u32) -> (Feed, u32, Ready) {
        let (feed, io, commands, _wire) = wired();
        (feed, conn, Ready::new(key, pro(true), io, commands))
    }

    #[test]
    fn only_pro_controllers_in_pairing_mode_are_connected() {
        let mut peers: Peers<&str, u32> = Peers::default();
        assert!(!peers.admit(&"a", &pro(true), false), "search off");
        assert!(
            !peers.admit(&"a", &pro(false), true),
            "it wants its console"
        );
        let joycon = Advert {
            model: Switch2Model::JoyConLeft,
            ..pro(true)
        };
        assert!(!peers.admit(&"b", &joycon, true));
        assert!(peers.admit(&"a", &pro(true), true));
        peers.connecting("a", ());
        assert!(!peers.admit(&"a", &pro(true), true), "already connecting");
    }

    #[test]
    fn a_stale_attempt_cannot_take_a_newer_ones_place() {
        let mut peers: Peers<&str, u32> = Peers::default();
        let old = peers.connecting("a", ());
        // It disconnected while set up, and came back.
        assert!(matches!(peers.let_go(&"a"), Some(Gone::Connecting(()))));
        let new = peers.connecting("a", ());
        assert_ne!(old, new);

        let (feed, conn, ready) = set_up("a", 1);
        assert_eq!(
            peers.up(&"a", old, feed, conn, ready),
            Err(1),
            "handed back"
        );
        peers.failed(&"a", old);
        assert!(peers.is_connecting(&"a", new), "the new attempt goes on");
        assert!(peers.poll().ready.is_empty());

        let (feed, conn, ready) = set_up("a", 2);
        assert_eq!(peers.up(&"a", new, feed, conn, ready), Ok(()));
        assert_eq!(peers.poll().ready.len(), 1);
        assert!(
            peers.let_go_attempt(&"a", old).is_none(),
            "not the old one's to end"
        );
        assert!(peers.conn_mut(&"a", new).is_some());
    }

    #[test]
    fn letting_go_ends_the_link() {
        let mut peers: Peers<&str, u32> = Peers::default();
        assert!(!peers.admit(&"a", &pro(false), true), "passed");
        let attempt = peers.connecting("a", ());
        let (feed, conn, ready) = set_up("a", 7);
        assert!(peers.up(&"a", attempt, feed, conn, ready).is_ok());
        let mut poll = peers.poll();
        assert!(poll.has("ble:a"));
        let mut io = poll.ready.pop().unwrap().io;
        assert!(peers.silent().is_empty());

        assert!(matches!(peers.let_go(&"a"), Some(Gone::Up(7))));
        assert!(!peers.poll().has("ble:a"));
        assert_eq!(
            io.wait(Duration::ZERO).err().map(|e| e.kind()),
            Some(io::ErrorKind::NotConnected),
            "its device thread stops"
        );
        assert!(peers.admit(&"a", &pro(true), true), "found again");
    }

    #[test]
    fn silent_controllers_are_found() {
        let mut peers: Peers<&str, u32> = Peers::default();
        let attempt = peers.connecting("a", ());
        let (feed, conn, ready) = set_up("a", 1);
        let quiet = feed.clone();
        assert!(peers.up(&"a", attempt, feed, conn, ready).is_ok());
        let long_ago = Instant::now().checked_sub(SILENCE * 2).unwrap();
        *quiet.shared.last_input.lock().unwrap() = long_ago;
        assert_eq!(peers.silent(), ["a"]);
    }
}
