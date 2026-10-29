//! Switch 2 controllers over Bluetooth LE, through CoreBluetooth.
//!
//! macOS cannot pair them (see [`crate::nintendo::switch2ble`]), so Doppelpad looks for one
//! in pairing mode itself, connects it and subscribes to its input. The controller then runs
//! in the HID backend like any other, over a [`crate::switch2gatt`] link.
//!
//! Bluetooth needs a permission. Doppelpad turns it on by itself only once the permission is
//! given; [`request`] (the app's *Allow* button) turns it on regardless, which brings up
//! macOS's prompt. It scans only while the search is on ([`crate::Switch2Search`]).
//!
//! CoreBluetooth objects are not thread-safe. They live on one serial dispatch queue, where the
//! manager's and the controllers' delegate calls run; the device threads submit their writes
//! to it, and the scanner asks it for the controllers that are ready.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use dispatch2::{DispatchQueue, DispatchRetained, DispatchTime};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AnyThread, Message, define_class, msg_send};
use objc2_core_bluetooth::{
    CBAdvertisementDataManufacturerDataKey, CBCentralManager, CBCentralManagerDelegate,
    CBCentralManagerOptionShowPowerAlertKey, CBCharacteristic, CBCharacteristicWriteType,
    CBManager, CBManagerAuthorization, CBManagerState, CBPeripheral, CBPeripheralDelegate,
    CBService, CBUUID,
};
use objc2_foundation::{
    NSArray, NSBundle, NSData, NSDictionary, NSError, NSNumber, NSObject, NSObjectProtocol,
    NSString,
};

use crate::nintendo::switch2ble::{self, Advert, Role};
use crate::switch2gatt::{
    self, Attempt, CONNECT_TIMEOUT, Gone, Peers, Poll, Ready, Target, Writer,
};

/// Start the scan over again this often: CoreBluetooth reports a controller once per scan,
/// and one first seen looking for its console may be put into pairing mode later.
const RESCAN: Duration = Duration::from_secs(10);
/// Commands that wait for room on the link; more and the oldest go.
const QUEUED_COMMANDS: usize = 16;

/// The Bluetooth permission, as a driver check.
pub fn check() -> crate::Result<()> {
    match authorization() {
        CBManagerAuthorization::AllowedAlways => Ok(()),
        CBManagerAuthorization::NotDetermined => Err(crate::PlatformError::Other(
            "Switch 2 controllers over Bluetooth need the Bluetooth permission".into(),
        )),
        _ => Err(crate::PlatformError::Other(
            "Switch 2 controllers over Bluetooth need the Bluetooth permission (System \
             Settings › Privacy & Security › Bluetooth)"
                .into(),
        )),
    }
}

/// Turn Bluetooth on. Returns `true` when macOS asks the user for the permission now;
/// `false` when it was refused before and only System Settings can give it.
pub fn request() -> bool {
    match authorization() {
        CBManagerAuthorization::NotDetermined | CBManagerAuthorization::AllowedAlways => {
            if !can_ask() {
                tracing::warn!(
                    "Bluetooth: this program has no usage description, so macOS would end it \
                     when it asks for the permission"
                );
                return false;
            }
            start();
            true
        }
        _ => false,
    }
}

/// Turn Bluetooth on if the permission is given already: never a prompt by surprise.
pub(crate) fn start_if_allowed() {
    if authorization() == CBManagerAuthorization::AllowedAlways {
        start();
    }
}

fn authorization() -> CBManagerAuthorization {
    // SAFETY: a class property without arguments.
    unsafe { CBManager::authorization_class() }
}

/// Whether asking for the permission is safe: macOS ends a program that uses Bluetooth
/// without a usage description in its Info.plist (the app has one; the command-line tool,
/// whose permission belongs to its terminal, does not).
fn can_ask() -> bool {
    NSBundle::mainBundle()
        .objectForInfoDictionaryKey(&NSString::from_str("NSBluetoothAlwaysUsageDescription"))
        .is_some()
}

/// What changed since the last call, with the search on or off as `search` says (turning it
/// on or off takes effect here rather than with the next rescan). Controllers silent for too
/// long are let go.
pub(crate) fn poll(search: bool) -> Poll {
    let Some(c) = CENTRAL.get() else {
        return Poll::default();
    };
    let changed = c.searching.swap(search, Ordering::SeqCst) != search;
    if changed && c.started.load(Ordering::SeqCst) {
        if search {
            tracing::info!("Bluetooth: looking for Switch 2 controllers in pairing mode");
        } else {
            tracing::info!("Bluetooth: no longer looking for Switch 2 controllers");
        }
    }
    let polled = Mutex::new(Poll::default());
    c.queue.exec_sync(|| {
        let done = with_cb(|cb| {
            if changed && powered_on(cb) {
                scan(cb);
            }
            for key in cb.peers.silent() {
                tracing::info!("Bluetooth: no input from the Switch 2 controller, letting it go");
                cancel(cb, &key);
            }
            cb.peers.poll()
        });
        if let Some(p) = done {
            *polled.lock().unwrap_or_else(PoisonError::into_inner) = p;
        }
    });
    polled.into_inner().unwrap_or_else(PoisonError::into_inner)
}

/// Let every controller go and stop scanning, when the backend stops.
pub(crate) fn let_all_go() {
    let Some(c) = CENTRAL.get() else {
        return;
    };
    c.searching.store(false, Ordering::SeqCst);
    c.queue.exec_sync(|| {
        with_cb(|cb| {
            // SAFETY: stopping the scan of a live manager.
            unsafe { cb.manager.stopScan() };
            for key in cb.peers.keys() {
                cancel(cb, &key);
            }
        });
    });
}

static CENTRAL: OnceLock<Central> = OnceLock::new();

fn central() -> &'static Central {
    CENTRAL.get_or_init(|| Central {
        queue: DispatchQueue::new("dev.doppelpad.bluetooth", None),
        started: AtomicBool::new(false),
        searching: AtomicBool::new(true),
        cb: OnQueue(RefCell::new(None)),
    })
}

struct Central {
    queue: DispatchRetained<DispatchQueue>,
    started: AtomicBool,
    /// Whether the search is on, as the scanner last told.
    searching: AtomicBool,
    /// CoreBluetooth's side, made by [`start`].
    cb: OnQueue,
}

/// CoreBluetooth's side, used only on the central's queue.
struct OnQueue(RefCell<Option<Cb>>);

// SAFETY: the value is only reached through `with_cb`, which runs only on the central's serial
// queue (the delegate calls, and the work `start`, `poll`, `let_all_go`, `rescan_later` and
// `QueueWriter` submit to it): never from two threads at once, and never from a thread other
// than the queue's. Debug builds assert that.
unsafe impl Send for OnQueue {}
// SAFETY: as above.
unsafe impl Sync for OnQueue {}

struct Cb {
    manager: Retained<CBCentralManager>,
    delegate: Retained<Delegate>,
    /// Controllers being connected or connected, by peripheral identifier.
    peers: Peers<String, Peer, Peer>,
}

/// One controller's peripheral and what was found on it.
struct Peer {
    peripheral: Retained<CBPeripheral>,
    advert: Advert,
    input: Option<Retained<CBCharacteristic>>,
    command: Option<Retained<CBCharacteristic>>,
    reply: Option<Retained<CBCharacteristic>>,
    vibration: Option<Retained<CBCharacteristic>>,
    /// Whether the input and the reply characteristic notify yet: both make it ready.
    notifying: [bool; 2],
    /// Commands waiting for room on the link.
    commands: VecDeque<Vec<u8>>,
}

impl Peer {
    fn new(peripheral: Retained<CBPeripheral>, advert: Advert) -> Peer {
        Peer {
            peripheral,
            advert,
            input: None,
            command: None,
            reply: None,
            vibration: None,
            notifying: [false; 2],
            commands: VecDeque::new(),
        }
    }

    /// Send the commands that wait, as far as there is room.
    fn send_commands(&mut self) {
        let Some(ch) = &self.command else {
            self.commands.clear();
            return;
        };
        // SAFETY: asking a connected peripheral for room.
        while unsafe { self.peripheral.canSendWriteWithoutResponse() } {
            let Some(data) = self.commands.pop_front() else {
                return;
            };
            // SAFETY: writing to a characteristic of a connected peripheral.
            unsafe {
                self.peripheral.writeValue_forCharacteristic_type(
                    &NSData::with_bytes(&data),
                    ch,
                    CBCharacteristicWriteType::WithoutResponse,
                );
            }
        }
    }
}

/// Run `f` with CoreBluetooth's side, if Bluetooth was started. Only on the queue.
fn with_cb<R>(f: impl FnOnce(&mut Cb) -> R) -> Option<R> {
    let c = central();
    #[cfg(debug_assertions)]
    c.queue.assert();
    c.cb.0.borrow_mut().as_mut().map(f)
}

fn start() {
    let c = central();
    if c.started.swap(true, Ordering::SeqCst) {
        return;
    }
    if c.searching.load(Ordering::SeqCst) {
        tracing::info!("Bluetooth: looking for Switch 2 controllers in pairing mode");
    }
    c.queue.exec_async(|| {
        let c = central();
        let delegate = Delegate::new();
        let no = NSNumber::new_bool(false);
        // SAFETY: a constant of the framework.
        let key = unsafe { CBCentralManagerOptionShowPowerAlertKey };
        let options = NSDictionary::<NSString, AnyObject>::from_slices(&[key], &[&*no]);
        // SAFETY: the delegate lives as long as the manager (both in `Cb`), and the queue is
        // serial, as `OnQueue` needs.
        let manager = unsafe {
            CBCentralManager::initWithDelegate_queue_options(
                CBCentralManager::alloc(),
                Some(ProtocolObject::from_ref(&*delegate)),
                Some(&c.queue),
                Some(&options),
            )
        };
        *c.cb.0.borrow_mut() = Some(Cb {
            manager,
            delegate,
            peers: Peers::default(),
        });
        with_cb(|cb| {
            if powered_on(cb) {
                scan(cb);
            }
        });
        rescan_later();
    });
}

fn powered_on(cb: &Cb) -> bool {
    // SAFETY: reading the state of a live manager.
    unsafe { cb.manager.state() == CBManagerState::PoweredOn }
}

/// Start the scan over, or only stop it while the search is off.
fn scan(cb: &Cb) {
    // SAFETY: no service filter and no options: everything that advertises.
    unsafe {
        cb.manager.stopScan();
        if central().searching.load(Ordering::SeqCst) {
            cb.manager
                .scanForPeripheralsWithServices_options(None, None);
        }
    }
}

fn rescan_later() {
    let when = DispatchTime::try_from(RESCAN).unwrap_or(DispatchTime::NOW);
    let _ = central().queue.after(when, || {
        with_cb(|cb| {
            if powered_on(cb) {
                scan(cb);
            }
        });
        rescan_later();
    });
}

fn key_of(p: &CBPeripheral) -> String {
    // SAFETY: the identifier of a live peripheral.
    unsafe { p.identifier() }.UUIDString().to_string()
}

/// The UUID of a service or characteristic, as a number.
fn uuid_of(uuid: &CBUUID) -> Option<u128> {
    // SAFETY: the UUID of a live service or characteristic.
    switch2ble::parse_uuid(&unsafe { uuid.UUIDString() }.to_string())
}

fn cbuuid(uuid: u128) -> Retained<CBUUID> {
    // SAFETY: a well-formed UUID string.
    unsafe { CBUUID::UUIDWithString(&NSString::from_str(&switch2ble::uuid_text(uuid))) }
}

/// Whether `slot` holds the characteristic `ch`.
fn is(slot: &Option<Retained<CBCharacteristic>>, ch: &CBCharacteristic) -> bool {
    slot.as_deref().is_some_and(|c| std::ptr::eq(c, ch))
}

fn on_state(manager: &CBManager) {
    // SAFETY: reading the state of a live manager.
    let state = unsafe { manager.state() };
    tracing::debug!(state = state.0, "Bluetooth state");
    with_cb(|cb| {
        if state == CBManagerState::PoweredOn {
            scan(cb);
        } else {
            // Bluetooth off or taken away: every link is gone.
            for key in cb.peers.keys() {
                cb.peers.let_go(&key);
            }
        }
    });
}

fn on_discover(p: &CBPeripheral, ad: &NSDictionary<NSString, AnyObject>) {
    // SAFETY: a constant of the framework.
    let key = unsafe { CBAdvertisementDataManufacturerDataKey };
    let Some(data) = ad
        .objectForKey(key)
        .and_then(|d| d.downcast::<NSData>().ok())
    else {
        return;
    };
    let Some(advert) = switch2ble::parse_advert(&data.to_vec()) else {
        return;
    };
    let key = key_of(p);
    with_cb(|cb| {
        let searching = central().searching.load(Ordering::SeqCst);
        if !cb.peers.admit(&key, &advert, searching) {
            return;
        }
        // SAFETY: the delegate outlives the peripheral's use (both kept in `Cb`).
        unsafe {
            p.setDelegate(Some(ProtocolObject::from_ref(&*cb.delegate)));
            cb.manager.connectPeripheral_options(p, None);
        }
        let attempt = cb
            .peers
            .connecting(key.clone(), Peer::new(p.retain(), advert));
        let when = DispatchTime::try_from(CONNECT_TIMEOUT).unwrap_or(DispatchTime::NOW);
        let _ = central().queue.after(when, move || {
            with_cb(|cb| {
                // Only this attempt: the controller may have been let go and found again.
                if cb.peers.is_connecting(&key, attempt) {
                    tracing::info!("Bluetooth: the controller did not connect in time");
                    cancel(cb, &key);
                }
            });
        });
    });
}

/// Let a controller go and end its connection.
fn cancel(cb: &mut Cb, key: &str) {
    let peer = match cb.peers.let_go(&key.to_owned()) {
        Some(Gone::Connecting(p) | Gone::Up(p)) => p,
        None => return,
    };
    // SAFETY: cancelling a peripheral of this manager.
    unsafe { cb.manager.cancelPeripheralConnection(&peer.peripheral) };
}

fn on_connect(p: &CBPeripheral) {
    tracing::debug!("Bluetooth: connected, looking at the controller's services");
    let services = NSArray::from_retained_slice(&[cbuuid(switch2ble::SERVICE)]);
    // SAFETY: a connected peripheral.
    unsafe { p.discoverServices(Some(&services)) };
}

fn on_failed(p: &CBPeripheral, error: Option<&NSError>) {
    tracing::info!(
        "Bluetooth: connecting the controller failed: {}",
        describe(error)
    );
    let key = key_of(p);
    with_cb(|cb| cb.peers.let_go(&key));
}

fn on_disconnect(p: &CBPeripheral, error: Option<&NSError>) {
    let key = key_of(p);
    with_cb(|cb| {
        if cb.peers.let_go(&key).is_some() {
            tracing::info!(
                "Bluetooth: Switch 2 controller disconnected: {}",
                describe(error)
            );
        }
        scan(cb);
    });
}

fn describe(error: Option<&NSError>) -> String {
    error.map_or_else(
        || "no error".to_owned(),
        |e| e.localizedDescription().to_string(),
    )
}

fn on_services(p: &CBPeripheral, error: Option<&NSError>) {
    let key = key_of(p);
    // SAFETY: the services of a live peripheral.
    let service = unsafe { p.services() }.and_then(|services| {
        services.iter().find(|s| {
            // SAFETY: the UUID of a discovered service.
            uuid_of(&*unsafe { s.UUID() }) == Some(switch2ble::SERVICE)
        })
    });
    match service {
        // SAFETY: a service of this peripheral.
        Some(s) => unsafe { p.discoverCharacteristics_forService(None, &s) },
        None => {
            tracing::warn!(
                "Bluetooth: the controller lacks its input service: {}",
                describe(error)
            );
            with_cb(|cb| cancel(cb, &key));
        }
    }
}

fn on_characteristics(p: &CBPeripheral, service: &CBService, error: Option<&NSError>) {
    let key = key_of(p);
    with_cb(|cb| {
        let Some((_, peer)) = cb.peers.setting_up(&key) else {
            return;
        };
        // SAFETY: the characteristics of a discovered service.
        for ch in unsafe { service.characteristics() }
            .iter()
            .flat_map(|a| a.iter())
        {
            // SAFETY: the UUID of a discovered characteristic.
            let uuid = uuid_of(&*unsafe { ch.UUID() });
            let slot = match uuid.and_then(|u| switch2ble::role(u, peer.advert.model)) {
                Some(Role::Input) => &mut peer.input,
                Some(Role::Command) => &mut peer.command,
                Some(Role::Reply) => &mut peer.reply,
                Some(Role::Vibration) => &mut peer.vibration,
                None => continue,
            };
            *slot = Some(ch);
        }
        let (Some(input), Some(_), Some(reply)) = (&peer.input, &peer.command, &peer.reply) else {
            tracing::warn!(
                "Bluetooth: the controller lacks input or command characteristics: {}",
                describe(error)
            );
            cancel(cb, &key);
            return;
        };
        // SAFETY: notifying characteristics of a connected peripheral.
        unsafe {
            p.setNotifyValue_forCharacteristic(true, input);
            p.setNotifyValue_forCharacteristic(true, reply);
        }
    });
}

/// A characteristic's notifications changed: once the input's and the replies' are on, the
/// controller is ready.
fn on_notifying(p: &CBPeripheral, ch: &CBCharacteristic, error: Option<&NSError>) {
    let key = key_of(p);
    with_cb(|cb| {
        if error.is_some() {
            tracing::warn!(
                "Bluetooth: the controller refused notifications: {}",
                describe(error)
            );
            cancel(cb, &key);
            return;
        }
        let Some((attempt, peer)) = cb.peers.setting_up(&key) else {
            return;
        };
        // SAFETY: the state of a characteristic of a connected peripheral.
        let on = unsafe { ch.isNotifying() };
        if is(&peer.input, ch) {
            peer.notifying[0] = on;
        } else if is(&peer.reply, ch) {
            peer.notifying[1] = on;
        } else {
            return;
        }
        if peer.notifying != [true; 2] {
            return;
        }
        cb.peers.up_from(&key, attempt, |peer| {
            let writer = QueueWriter {
                key: key.clone(),
                attempt,
            };
            let (feed, io, commands) = switch2gatt::link(Box::new(writer));
            let ready = Ready::new(&key, peer.advert, io, commands);
            (feed, peer, ready)
        });
    });
}

fn on_value(p: &CBPeripheral, ch: &CBCharacteristic) {
    let key = key_of(p);
    with_cb(|cb| {
        let Some((feed, peer)) = cb.peers.up_conn(&key) else {
            return;
        };
        // SAFETY: the value of a characteristic that just notified.
        let Some(value) = (unsafe { ch.value() }) else {
            return;
        };
        if is(&peer.input, ch) {
            if !feed.input(value.to_vec()) {
                tracing::info!("Bluetooth: nothing reads the controller any more, letting it go");
                cancel(cb, &key);
            }
        } else if is(&peer.reply, ch) {
            feed.reply(value.to_vec());
        }
    });
}

/// The link has room again: the commands that wait go out.
fn on_ready_to_send(p: &CBPeripheral) {
    let key = key_of(p);
    with_cb(|cb| {
        if let Some(peer) = cb.peers.up_conn_mut(&key) {
            peer.send_commands();
        }
    });
}

/// Writes to a controller through the queue: to the connection `attempt` made, not to one made
/// after it was let go.
///
/// As the [`Writer`] contract says, a rumble write the link has no room for is dropped; a
/// command waits for room (CoreBluetooth says when there is), as on the other OSes.
struct QueueWriter {
    key: String,
    attempt: Attempt,
}

impl Writer for QueueWriter {
    fn write(&self, target: Target, data: Vec<u8>) {
        let (key, attempt) = (self.key.clone(), self.attempt);
        central().queue.exec_async(move || {
            with_cb(|cb| {
                let Some(peer) = cb.peers.conn_mut(&key, attempt) else {
                    return;
                };
                match target {
                    Target::Command => {
                        if peer.commands.len() == QUEUED_COMMANDS {
                            peer.commands.pop_front();
                        }
                        peer.commands.push_back(data);
                        peer.send_commands();
                    }
                    Target::Vibration => {
                        let Some(ch) = &peer.vibration else {
                            return;
                        };
                        // SAFETY: asking a connected peripheral for room, then writing to one
                        // of its characteristics.
                        unsafe {
                            if !peer.peripheral.canSendWriteWithoutResponse() {
                                tracing::trace!("Bluetooth: no room for a rumble report");
                                return;
                            }
                            peer.peripheral.writeValue_forCharacteristic_type(
                                &NSData::with_bytes(&data),
                                ch,
                                CBCharacteristicWriteType::WithoutResponse,
                            );
                        }
                    }
                }
            });
        });
    }
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `Delegate` does not implement
    // `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "DoppelpadBluetoothDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl CBCentralManagerDelegate for Delegate {
        #[unsafe(method(centralManagerDidUpdateState:))]
        fn did_update_state(&self, manager: &CBCentralManager) {
            on_state(manager);
        }

        #[unsafe(method(centralManager:didDiscoverPeripheral:advertisementData:RSSI:))]
        fn did_discover(
            &self,
            _manager: &CBCentralManager,
            peripheral: &CBPeripheral,
            advertisement: &NSDictionary<NSString, AnyObject>,
            _rssi: &NSNumber,
        ) {
            on_discover(peripheral, advertisement);
        }

        #[unsafe(method(centralManager:didConnectPeripheral:))]
        fn did_connect(&self, _manager: &CBCentralManager, peripheral: &CBPeripheral) {
            on_connect(peripheral);
        }

        #[unsafe(method(centralManager:didFailToConnectPeripheral:error:))]
        fn did_fail(
            &self,
            _manager: &CBCentralManager,
            peripheral: &CBPeripheral,
            error: Option<&NSError>,
        ) {
            on_failed(peripheral, error);
        }

        #[unsafe(method(centralManager:didDisconnectPeripheral:error:))]
        fn did_disconnect(
            &self,
            _manager: &CBCentralManager,
            peripheral: &CBPeripheral,
            error: Option<&NSError>,
        ) {
            on_disconnect(peripheral, error);
        }
    }

    unsafe impl CBPeripheralDelegate for Delegate {
        #[unsafe(method(peripheral:didDiscoverServices:))]
        fn did_discover_services(&self, peripheral: &CBPeripheral, error: Option<&NSError>) {
            on_services(peripheral, error);
        }

        #[unsafe(method(peripheral:didDiscoverCharacteristicsForService:error:))]
        fn did_discover_characteristics(
            &self,
            peripheral: &CBPeripheral,
            service: &CBService,
            error: Option<&NSError>,
        ) {
            on_characteristics(peripheral, service, error);
        }

        #[unsafe(method(peripheral:didUpdateNotificationStateForCharacteristic:error:))]
        fn did_update_notification_state(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            on_notifying(peripheral, characteristic, error);
        }

        #[unsafe(method(peripheral:didUpdateValueForCharacteristic:error:))]
        fn did_update_value(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            _error: Option<&NSError>,
        ) {
            on_value(peripheral, characteristic);
        }

        #[unsafe(method(peripheralIsReadyToSendWriteWithoutResponse:))]
        fn is_ready_to_send(&self, peripheral: &CBPeripheral) {
            on_ready_to_send(peripheral);
        }
    }
);

impl Delegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        // SAFETY: NSObject's designated initialiser.
        unsafe { msg_send![super(this), init] }
    }
}

#[cfg(test)]
mod tests {
    use objc2::sel;

    use super::*;

    #[test]
    fn delegate_answers_corebluetooth() {
        let delegate = Delegate::new();
        let selectors = [
            sel!(centralManagerDidUpdateState:),
            sel!(centralManager:didDiscoverPeripheral:advertisementData:RSSI:),
            sel!(centralManager:didConnectPeripheral:),
            sel!(centralManager:didFailToConnectPeripheral:error:),
            sel!(centralManager:didDisconnectPeripheral:error:),
            sel!(peripheral:didDiscoverServices:),
            sel!(peripheral:didDiscoverCharacteristicsForService:error:),
            sel!(peripheral:didUpdateNotificationStateForCharacteristic:error:),
            sel!(peripheral:didUpdateValueForCharacteristic:error:),
            sel!(peripheralIsReadyToSendWriteWithoutResponse:),
        ];
        for s in selectors {
            assert!(delegate.respondsToSelector(s), "{s:?}");
        }
        // Nothing was started: no prompt, and no controllers.
        let nothing = poll(true);
        assert!(nothing.connected.is_empty() && nothing.ready.is_empty());
    }

    /// Connects a Pro Controller in pairing mode near this Mac and drives it for a while; see
    /// [`switch2gatt::drive_controller`]. Needs the Bluetooth permission. Run with
    /// `--nocapture`.
    #[test]
    #[ignore = "connects a Switch 2 Pro Controller in pairing mode near this Mac"]
    fn switch2_pro_over_bluetooth() {
        start();
        switch2gatt::drive_controller(|| poll(true));
        let_all_go();
    }
}
