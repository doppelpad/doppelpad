//! Switch 2 controllers over Bluetooth LE, through CoreBluetooth.
//!
//! macOS cannot pair them (see [`crate::nintendo::switch2ble`]), so Doppelpad looks for one
//! in pairing mode itself, connects it and subscribes to its input. The controller then runs
//! in the HID backend like any other, over a [`crate::switch2gatt`] link. A controller that is
//! looking for the console it is paired with is left alone.
//!
//! Bluetooth needs a permission. Doppelpad turns it on by itself only once the permission is
//! given; [`request`] (the app's *Allow* button) turns it on regardless, which brings up
//! macOS's prompt.
//!
//! CoreBluetooth objects are not thread-safe. They live on one serial dispatch queue, where the
//! manager's and the controllers' delegate calls run; the device threads submit their writes
//! to it.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
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

use crate::nintendo::Switch2Model;
use crate::nintendo::switch2ble::{self, Advert};
use crate::switch2gatt::{self, BleIo, Feed, GattCommands, Links, Poll, Ready, Target, Writer};

/// Start the scan over again this often: CoreBluetooth reports a controller once per scan,
/// and one first seen looking for its console may be put into pairing mode later.
const RESCAN: Duration = Duration::from_secs(10);
/// Give up on a controller that has not connected and answered by then.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

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

/// What changed since the last call.
pub(crate) fn poll() -> Poll {
    match CENTRAL.get() {
        Some(c) => c.links.lock().unwrap().poll(),
        None => Poll::default(),
    }
}

static CENTRAL: OnceLock<Central> = OnceLock::new();

fn central() -> &'static Central {
    CENTRAL.get_or_init(|| Central {
        queue: DispatchQueue::new("dev.doppelpad.bluetooth", None),
        started: AtomicBool::new(false),
        cb: OnQueue(RefCell::new(None)),
        links: Mutex::default(),
    })
}

struct Central {
    queue: DispatchRetained<DispatchQueue>,
    started: AtomicBool,
    /// CoreBluetooth's side, made by [`start`].
    cb: OnQueue<RefCell<Option<Cb>>>,
    links: Mutex<Links>,
}

/// A value used only on the central's queue.
struct OnQueue<T>(T);

// SAFETY: the value is only reached from work running on the central's serial queue (the
// delegate calls, and the work `start`, `rescan_later` and `write` submit), so never from two
// threads at once, and never from a thread other than the queue's.
unsafe impl<T> Send for OnQueue<T> {}
// SAFETY: as above.
unsafe impl<T> Sync for OnQueue<T> {}

struct Cb {
    manager: Retained<CBCentralManager>,
    delegate: Retained<Delegate>,
    /// Controllers being connected or connected, by peripheral identifier.
    peers: HashMap<String, Peer>,
    /// Controllers already logged as not wanted, by peripheral identifier.
    passed: HashSet<String>,
}

/// One controller, from discovery until it disconnects.
struct Peer {
    peripheral: Retained<CBPeripheral>,
    /// The peripheral identifier.
    key: String,
    advert: Advert,
    input: Option<Retained<CBCharacteristic>>,
    command: Option<Retained<CBCharacteristic>>,
    reply: Option<Retained<CBCharacteristic>>,
    vibration: Option<Retained<CBCharacteristic>>,
    /// Notifications switched on so far: input and replies make it ready.
    notifying: u8,
    feed: Feed,
    /// The device thread's ends, until the controller is handed to the scanner.
    ends: Option<(BleIo, GattCommands)>,
}

/// Run `f` with CoreBluetooth's side, if Bluetooth was started. Only on the queue.
fn with_cb<R>(f: impl FnOnce(&mut Cb) -> R) -> Option<R> {
    central().cb.0.borrow_mut().as_mut().map(f)
}

fn start() {
    let c = central();
    if c.started.swap(true, Ordering::SeqCst) {
        return;
    }
    tracing::info!("Bluetooth: looking for Switch 2 controllers in pairing mode");
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
            peers: HashMap::new(),
            passed: HashSet::new(),
        });
        with_cb(|cb| {
            // SAFETY: reading the state of a live manager.
            if unsafe { cb.manager.state() } == CBManagerState::PoweredOn {
                scan(cb);
            }
        });
        rescan_later();
    });
}

fn scan(cb: &Cb) {
    // SAFETY: no service filter and no options: everything that advertises.
    unsafe {
        cb.manager.stopScan();
        cb.manager
            .scanForPeripheralsWithServices_options(None, None);
    }
}

fn rescan_later() {
    let when = DispatchTime::try_from(RESCAN).unwrap_or(DispatchTime::NOW);
    let _ = central().queue.after(when, || {
        with_cb(|cb| {
            // SAFETY: reading the state of a live manager.
            if unsafe { cb.manager.state() } == CBManagerState::PoweredOn {
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

fn uuid_of(ch: &CBCharacteristic) -> String {
    // SAFETY: the UUID of a live characteristic.
    unsafe { ch.UUID().UUIDString() }.to_string()
}

fn cbuuid(s: &str) -> Retained<CBUUID> {
    // SAFETY: a well-formed UUID string.
    unsafe { CBUUID::UUIDWithString(&NSString::from_str(s)) }
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
            for (_, peer) in cb.peers.drain() {
                drop_peer(&peer);
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
        if cb.peers.contains_key(&key) {
            return;
        }
        if !advert.pairing || advert.model != Switch2Model::Pro {
            if cb.passed.insert(key) {
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
        // SAFETY: the delegate outlives the peripheral's use (both kept in `Cb`).
        unsafe {
            p.setDelegate(Some(ProtocolObject::from_ref(&*cb.delegate)));
            cb.manager.connectPeripheral_options(p, None);
        }
        let (feed, io, commands) = switch2gatt::link(Box::new(QueueWriter { key: key.clone() }));
        cb.peers.insert(
            key.clone(),
            Peer {
                peripheral: p.retain(),
                key: key.clone(),
                advert,
                input: None,
                command: None,
                reply: None,
                vibration: None,
                notifying: 0,
                feed,
                ends: Some((io, commands)),
            },
        );
        let when = DispatchTime::try_from(CONNECT_TIMEOUT).unwrap_or(DispatchTime::NOW);
        let _ = central().queue.after(when, move || {
            with_cb(|cb| {
                if cb.peers.get(&key).is_some_and(|p| p.ends.is_some()) {
                    tracing::info!("Bluetooth: the controller did not connect in time");
                    cancel(cb, &key);
                }
            });
        });
    });
}

/// Drop a controller: the connection, and its device thread's link.
fn cancel(cb: &mut Cb, key: &str) {
    if let Some(peer) = cb.peers.remove(key) {
        // SAFETY: cancelling a peripheral of this manager.
        unsafe { cb.manager.cancelPeripheralConnection(&peer.peripheral) };
        drop_peer(&peer);
    }
}

/// Tell a controller's device thread it is gone, and forget it.
fn drop_peer(peer: &Peer) {
    peer.feed.close();
    central()
        .links
        .lock()
        .unwrap()
        .remove(&switch2gatt::path(&peer.key));
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
    with_cb(|cb| {
        if let Some(peer) = cb.peers.remove(&key) {
            drop_peer(&peer);
        }
    });
}

fn on_disconnect(p: &CBPeripheral, error: Option<&NSError>) {
    let key = key_of(p);
    with_cb(|cb| {
        if let Some(peer) = cb.peers.remove(&key) {
            tracing::info!(
                "Bluetooth: Switch 2 controller disconnected: {}",
                describe(error)
            );
            drop_peer(&peer);
        }
        // To find it again when it comes back in pairing mode.
        cb.passed.remove(&key);
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
            unsafe { s.UUID().UUIDString() }.to_string() == switch2ble::SERVICE
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
        let Some(peer) = cb.peers.get_mut(&key) else {
            return;
        };
        let vibration = switch2ble::vibration(peer.advert.model);
        // SAFETY: the characteristics of a discovered service.
        for ch in unsafe { service.characteristics() }
            .iter()
            .flat_map(|a| a.iter())
        {
            let uuid = uuid_of(&ch);
            let slot = match uuid.as_str() {
                switch2ble::INPUT => &mut peer.input,
                switch2ble::COMMAND => &mut peer.command,
                switch2ble::REPLY => &mut peer.reply,
                u if u == vibration => &mut peer.vibration,
                _ => continue,
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

fn on_notifying(p: &CBPeripheral, error: Option<&NSError>) {
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
        let Some(peer) = cb.peers.get_mut(&key) else {
            return;
        };
        peer.notifying += 1;
        if peer.notifying < 2 {
            return;
        }
        let Some((io, commands)) = peer.ends.take() else {
            return;
        };
        tracing::info!("Bluetooth: Switch 2 Pro Controller connected");
        let ready = Ready::new(&peer.key, peer.advert, io, commands);
        central().links.lock().unwrap().add(ready);
    });
}

fn on_value(p: &CBPeripheral, ch: &CBCharacteristic) {
    let key = key_of(p);
    with_cb(|cb| {
        let Some(peer) = cb.peers.get(&key) else {
            return;
        };
        // SAFETY: the value of a characteristic that just notified.
        let Some(value) = (unsafe { ch.value() }) else {
            return;
        };
        let is = |slot: &Option<Retained<CBCharacteristic>>| {
            slot.as_deref().is_some_and(|c| std::ptr::eq(c, ch))
        };
        if is(&peer.input) {
            peer.feed.input(value.to_vec());
        } else if is(&peer.reply) {
            peer.feed.reply(value.to_vec());
        }
    });
}

/// Writes to the controller with this peripheral identifier, through the queue.
struct QueueWriter {
    key: String,
}

impl Writer for QueueWriter {
    fn write(&self, target: Target, data: Vec<u8>) {
        let key = self.key.clone();
        central().queue.exec_async(move || {
            with_cb(|cb| {
                let Some(peer) = cb.peers.get(&key) else {
                    return;
                };
                let slot = match target {
                    Target::Command => &peer.command,
                    Target::Vibration => &peer.vibration,
                };
                let Some(ch) = slot else {
                    return;
                };
                // SAFETY: writing to a characteristic of a connected peripheral.
                unsafe {
                    // Rumble comes again soon; commands are worth the wait.
                    if matches!(target, Target::Vibration)
                        && !peer.peripheral.canSendWriteWithoutResponse()
                    {
                        tracing::trace!("Bluetooth: no room for a rumble report");
                        return;
                    }
                    peer.peripheral.writeValue_forCharacteristic_type(
                        &NSData::with_bytes(&data),
                        ch,
                        CBCharacteristicWriteType::WithoutResponse,
                    );
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
            _characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            on_notifying(peripheral, error);
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
        ];
        for s in selectors {
            assert!(delegate.respondsToSelector(s), "{s:?}");
        }
        // Nothing was started: no prompt, and no controllers.
        let nothing = poll();
        assert!(nothing.connected.is_empty() && nothing.ready.is_empty());
    }
}
