//! Switch 2 controllers over Bluetooth LE, between an OS's Bluetooth client and the HID
//! backend. The client (`windows::ble`, `macos::ble`, `linux::ble`) connects a controller,
//! feeds its notifications into a [`Feed`] and carries writes out through a [`Writer`]; its
//! device thread sees the input as HID reports ([`BleIo`]) and talks to it over a command
//! channel ([`GattCommands`]), as over USB. What the controller sends and takes is in
//! [`crate::nintendo::switch2ble`].
//!
//! It also compiles in tests on other OSes, so it is unit-tested everywhere.

#![cfg_attr(
    not(any(windows, target_os = "linux", target_os = "macos")),
    allow(dead_code)
)]

use std::collections::HashSet;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};

use crate::DeviceId;
use crate::hidhost::{Devices, Found, HidHandle, Wait, Wake};
use crate::nintendo::NINTENDO_VID;
use crate::nintendo::switch2::Switch2Driver;
use crate::nintendo::switch2ble::{self, Advert};
use crate::padio::{BulkIo, ReportIo};

/// Input reports a device thread may fall behind by before new ones are dropped.
const QUEUED_REPORTS: usize = 64;

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
    /// An input notification.
    pub fn input(&self, value: Vec<u8>) {
        *self.shared.last_input.lock().unwrap() = Instant::now();
        let _ = self.events.try_send(Event::Report(value));
    }

    /// How long no input has arrived. The controllers send all the time, so a long silence
    /// means the link is gone, even before the OS says so.
    #[cfg(any(windows, target_os = "linux", test))]
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
                Ok(Wait::Report(self.report.len()))
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
        let until = Instant::now() + timeout;
        loop {
            match self.wait(until.saturating_duration_since(Instant::now()))? {
                Wait::Report(n) => {
                    let n = n.min(buf.len());
                    buf[..n].copy_from_slice(&self.report[..n]);
                    return Ok(Some(n));
                }
                Wait::Woken if Instant::now() < until => {}
                Wait::Woken | Wait::Timeout => return Ok(None),
            }
        }
    }
}

/// A connected controller's command channel: commands out, replies back as notifications.
pub(crate) struct GattCommands {
    shared: Arc<Shared>,
    replies: Receiver<Vec<u8>>,
}

impl BulkIo for GattCommands {
    fn command(&mut self, cmd: &[u8], reply_len: usize, timeout: Duration) -> io::Result<Vec<u8>> {
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
                // The reply repeats the command and subcommand.
                Ok(mut reply) if reply.len() >= 4 && reply[0] == cmd[0] && reply[3] == cmd[3] => {
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
    pub address: Option<u64>,
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

    /// Start the controller's device thread; `instance_paths` hide it (see [`Found`]).
    pub fn start(self, devices: &Devices, instance_paths: Vec<String>) {
        let found = Found {
            path: self.path,
            id: DeviceId(self.id),
            transport: "Bluetooth",
            vid: NINTENDO_VID,
            pid: self.advert.pid,
            instance_paths,
            bluetooth: true,
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

/// A hardware test of a client, the whole way without the rest of Doppelpad: waits for a Pro
/// Controller in pairing mode to come out of the client's `poll`, starts its driver and drives
/// it (see [`crate::hidhost::testing::drive`]), calling `poll` once a second as the scanner
/// does, so that a controller turned off is let go. Run the test with `--nocapture`, hold the
/// pairing button, then press buttons, move the sticks, pull the triggers, tilt it, and turn it
/// off at the end.
#[cfg(all(test, any(windows, target_os = "linux")))]
pub(crate) fn drive_controller(poll: fn() -> Poll) {
    use crate::nintendo::switch2::Switch2Driver;

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
            Wait::Report(64)
        ));
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
}
