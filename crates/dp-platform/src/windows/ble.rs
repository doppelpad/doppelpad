//! Switch 2 controllers over Bluetooth LE, through WinRT (`Windows.Devices.Bluetooth`).
//!
//! Windows cannot pair them (see [`crate::nintendo::switch2ble`]), so Doppelpad watches the
//! advertisements for one in pairing mode, connects it without pairing and subscribes to its
//! input. The controller then runs in the HID backend like any other, over a
//! [`crate::switch2gatt`] link. A controller that is looking for the console it is paired with
//! is left alone.
//!
//! The pads never ask for a faster connection, so Doppelpad asks Windows for one. Windows 11
//! can be asked; Windows 10 keeps its default interval, about 17 reports a second.
//!
//! The watcher's events arrive on the thread pool. Each controller is set up on a thread of
//! its own, as that waits for the controller's answers.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementWatcher,
    BluetoothLEAdvertisementWatcherStatus, BluetoothLEScanningMode,
};
use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue,
    GattCommunicationStatus, GattDeviceService, GattSession, GattValueChangedEventArgs,
    GattWriteOption,
};
use windows::Devices::Bluetooth::{
    BluetoothAdapter, BluetoothAddressType, BluetoothCacheMode, BluetoothConnectionStatus,
    BluetoothLEDevice, BluetoothLEPreferredConnectionParameters,
    BluetoothLEPreferredConnectionParametersRequest,
};
use windows::Devices::Radios::RadioState;
use windows::Foundation::TypedEventHandler;
use windows::Storage::Streams::{DataReader, DataWriter, IBuffer};
use windows::core::{GUID, IInspectable, RuntimeType};
use windows_future::{AsyncOperationCompletedHandler, IAsyncOperation};

use crate::nintendo::Switch2Model;
use crate::nintendo::switch2ble::{self, Advert};
use crate::switch2gatt::{self, Feed, Links, Poll, Ready, Target, Writer};

/// Start the watcher again this long after it stopped (Bluetooth was off, or taken away).
const RESTART: Duration = Duration::from_secs(10);
/// Give up on a controller that has not connected and answered by then.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a command may take to go out.
const WRITE_TIMEOUT: Duration = Duration::from_secs(1);

/// Whether Switch 2 controllers can connect, as a driver check: a Bluetooth adapter that
/// speaks Bluetooth LE, switched on.
pub fn check() -> crate::Result<()> {
    let unusable = |why: String| Err(crate::PlatformError::Other(why));
    let Ok(adapter) = BluetoothAdapter::GetDefaultAsync().and_then(|op| op.join()) else {
        return unusable("no Bluetooth adapter".into());
    };
    if !adapter.IsLowEnergySupported().unwrap_or(false) {
        return unusable("the Bluetooth adapter does not support Bluetooth LE".into());
    }
    match adapter
        .GetRadioAsync()
        .and_then(|op| op.join())
        .and_then(|radio| radio.State())
    {
        Ok(RadioState::On) => Ok(()),
        Ok(_) => unusable("Bluetooth is off".into()),
        Err(e) => unusable(format!("cannot read the state of Bluetooth: {e}")),
    }
}

/// What changed since the last call. Watches for controllers from the first call on.
pub(crate) fn poll() -> Poll {
    let c = central();
    keep_watching(&mut c.watcher.lock().unwrap());
    c.state.lock().unwrap().links.poll()
}

static CENTRAL: OnceLock<Central> = OnceLock::new();

fn central() -> &'static Central {
    CENTRAL.get_or_init(Central::default)
}

#[derive(Default)]
struct Central {
    watcher: Mutex<Watcher>,
    state: Mutex<State>,
}

#[derive(Default)]
struct Watcher {
    watcher: Option<BluetoothLEAdvertisementWatcher>,
    /// When it was last started.
    started: Option<Instant>,
}

#[derive(Default)]
struct State {
    /// Controllers being connected or connected, by Bluetooth address.
    peers: HashMap<u64, Peer>,
    /// Controllers already logged as not wanted, by Bluetooth address.
    passed: HashSet<u64>,
    links: Links,
}

/// One controller, from discovery until it disconnects.
enum Peer {
    /// Being set up on its own thread.
    Connecting,
    Up {
        feed: Feed,
        conn: Conn,
    },
}

/// What keeps a controller connected. Dropping it lets the link go.
struct Conn {
    device: BluetoothLEDevice,
    session: GattSession,
    service: Option<GattDeviceService>,
    /// Characteristics that notify, with their handlers.
    notifying: Vec<(GattCharacteristic, i64)>,
    /// The handler of the connection status.
    status: Option<i64>,
    /// The faster connection asked for (Windows 11).
    faster: Option<BluetoothLEPreferredConnectionParametersRequest>,
}

impl Drop for Conn {
    fn drop(&mut self) {
        for (ch, token) in &self.notifying {
            let _ = ch.RemoveValueChanged(*token);
        }
        if let Some(token) = self.status {
            let _ = self.device.RemoveConnectionStatusChanged(token);
        }
        if let Some(faster) = &self.faster {
            let _ = faster.Close();
        }
        if let Some(service) = &self.service {
            let _ = service.Close();
        }
        let _ = self.session.Close();
        let _ = self.device.Close();
    }
}

/// Why a controller could not be set up, or a write failed.
struct Failure(String);

impl From<windows::core::Error> for Failure {
    fn from(e: windows::core::Error) -> Self {
        Failure(e.to_string())
    }
}

/// Start the watcher, and start it again after Windows stopped it.
fn keep_watching(w: &mut Watcher) {
    let status = w.watcher.as_ref().and_then(|x| x.Status().ok());
    if status == Some(BluetoothLEAdvertisementWatcherStatus::Started)
        || w.started.is_some_and(|at| at.elapsed() < RESTART)
    {
        return;
    }
    w.started = Some(Instant::now());
    if w.watcher.is_none() {
        match new_watcher() {
            Ok(x) => {
                tracing::info!("Bluetooth: looking for Switch 2 controllers in pairing mode");
                w.watcher = Some(x);
            }
            Err(e) => {
                tracing::debug!("Bluetooth: cannot watch for controllers: {e}");
                return;
            }
        }
    }
    if let Some(x) = &w.watcher
        && let Err(e) = x.Start()
    {
        tracing::debug!("Bluetooth: cannot watch for controllers: {e}");
    }
}

fn new_watcher() -> windows::core::Result<BluetoothLEAdvertisementWatcher> {
    let watcher = BluetoothLEAdvertisementWatcher::new()?;
    // Scan responses too, in case a controller sends its manufacturer data in one.
    watcher.SetScanningMode(BluetoothLEScanningMode::Active)?;
    watcher.Received(&TypedEventHandler::<
        BluetoothLEAdvertisementWatcher,
        BluetoothLEAdvertisementReceivedEventArgs,
    >::new(|_, args| {
        if let Some(args) = args.as_ref() {
            let _ = on_advert(args);
        }
        Ok(())
    }))?;
    Ok(watcher)
}

fn on_advert(args: &BluetoothLEAdvertisementReceivedEventArgs) -> windows::core::Result<()> {
    let sections = args
        .Advertisement()?
        .GetManufacturerDataByCompanyId(switch2ble::COMPANY_ID)?;
    for section in sections {
        // Windows takes the company identifier off the front.
        let mut data = switch2ble::COMPANY_ID.to_le_bytes().to_vec();
        data.extend(bytes(&section.Data()?)?);
        if let Some(advert) = switch2ble::parse_advert(&data) {
            discovered(
                args.BluetoothAddress()?,
                args.BluetoothAddressType()?,
                advert,
            );
        }
    }
    Ok(())
}

fn discovered(address: u64, kind: BluetoothAddressType, advert: Advert) {
    let mut state = central().state.lock().unwrap();
    if state.peers.contains_key(&address) {
        return;
    }
    if !advert.pairing || advert.model != Switch2Model::Pro {
        if state.passed.insert(address) {
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
    let spawned = std::thread::Builder::new()
        .name("dp-ble-connect".into())
        .spawn(move || connect(address, kind, advert));
    match spawned {
        Ok(_) => {
            state.peers.insert(address, Peer::Connecting);
        }
        Err(e) => tracing::warn!("Bluetooth: cannot connect the controller: {e}"),
    }
}

/// The name a controller is known by: its address, which stays.
fn key(address: u64) -> String {
    format!("{address:012x}")
}

/// Set a controller up and hand it to the scanner.
fn connect(address: u64, kind: BluetoothAddressType, advert: Advert) {
    let opened = open(address, kind, advert);
    // Let a connection go only after the lock: closing it may call back.
    let unwanted = {
        let mut state = central().state.lock().unwrap();
        // Not when it disconnected while it was set up.
        let wanted = matches!(state.peers.get(&address), Some(Peer::Connecting));
        match opened {
            Ok((feed, conn, ready)) if wanted => {
                tracing::info!("Bluetooth: Switch 2 Pro Controller connected");
                state.links.add(ready);
                state.peers.insert(address, Peer::Up { feed, conn });
                None
            }
            Ok((_, conn, _)) => Some(conn),
            Err(Failure(e)) => {
                tracing::info!("Bluetooth: connecting the controller failed: {e}");
                if wanted {
                    state.peers.remove(&address);
                }
                None
            }
        }
    };
    drop(unwanted);
}

/// Connect a controller, find its characteristics and switch its notifications on.
fn open(
    address: u64,
    kind: BluetoothAddressType,
    advert: Advert,
) -> Result<(Feed, Conn, Ready), Failure> {
    let until = Instant::now() + CONNECT_TIMEOUT;
    let device = wait(
        BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync(address, kind)?,
        until,
    )?;
    let session = wait(
        GattSession::FromDeviceIdAsync(&device.BluetoothDeviceId()?)?,
        until,
    )?;
    let mut conn = Conn {
        device,
        session,
        service: None,
        notifying: Vec::new(),
        status: None,
        faster: None,
    };
    // Nothing paired the controller, so nothing else keeps the link up.
    conn.session.SetMaintainConnection(true)?;

    let services = wait(
        conn.device.GetGattServicesForUuidWithCacheModeAsync(
            guid(switch2ble::SERVICE),
            BluetoothCacheMode::Uncached,
        )?,
        until,
    )?;
    succeeded(services.Status()?, "reading its services")?;
    let Some(service) = services.Services()?.into_iter().next() else {
        return Err(Failure("it lacks its input service".into()));
    };
    let found = wait(
        service.GetCharacteristicsWithCacheModeAsync(BluetoothCacheMode::Uncached)?,
        until,
    )?;
    conn.service = Some(service);
    succeeded(found.Status()?, "reading its characteristics")?;
    let (mut input, mut command, mut reply, mut vibration) = (None, None, None, None);
    let vibration_uuid = guid(switch2ble::vibration(advert.model));
    for ch in found.Characteristics()? {
        let uuid = ch.Uuid()?;
        let slot = if uuid == guid(switch2ble::INPUT) {
            &mut input
        } else if uuid == guid(switch2ble::COMMAND) {
            &mut command
        } else if uuid == guid(switch2ble::REPLY) {
            &mut reply
        } else if uuid == vibration_uuid {
            &mut vibration
        } else {
            continue;
        };
        *slot = Some(ch);
    }
    let (Some(input), Some(command), Some(reply)) = (input, command, reply) else {
        return Err(Failure("it lacks input or command characteristics".into()));
    };

    let (feed, io, commands) = switch2gatt::link(Box::new(GattWriter {
        command,
        vibration,
        rumbling: Arc::default(),
    }));
    for (ch, is_input) in [(input, true), (reply, false)] {
        let feed = feed.clone();
        let token = ch.ValueChanged(&TypedEventHandler::<
            GattCharacteristic,
            GattValueChangedEventArgs,
        >::new(move |_, args| {
            if let Some(args) = args.as_ref() {
                let value = bytes(&args.CharacteristicValue()?)?;
                if is_input {
                    feed.input(value);
                } else {
                    feed.reply(value);
                }
            }
            Ok(())
        }))?;
        conn.notifying.push((ch.clone(), token));
        let status = wait(
            ch.WriteClientCharacteristicConfigurationDescriptorAsync(
                GattClientCharacteristicConfigurationDescriptorValue::Notify,
            )?,
            until,
        )?;
        succeeded(status, "switching its notifications on")?;
    }

    conn.status = Some(conn.device.ConnectionStatusChanged(&TypedEventHandler::<
        BluetoothLEDevice,
        IInspectable,
    >::new(move |device, _| {
        if let Some(device) = device.as_ref()
            && device.ConnectionStatus()? == BluetoothConnectionStatus::Disconnected
        {
            on_disconnect(address);
        }
        Ok(())
    }))?);
    // Gone before the handler was there to notice.
    if conn.device.ConnectionStatus()? == BluetoothConnectionStatus::Disconnected {
        return Err(Failure("it disconnected".into()));
    }
    conn.faster = ask_faster(&conn.device);
    if let Ok(interval) = conn
        .device
        .GetConnectionParameters()
        .and_then(|p| p.ConnectionInterval())
    {
        // In units of 1.25 ms.
        tracing::info!(
            "Bluetooth: connection interval {} ms",
            f32::from(interval) * 1.25
        );
    }
    let ready = Ready::new(&key(address), advert, io, commands);
    Ok((feed, conn, ready))
}

/// Ask for the shortest connection interval, which the pads never ask for themselves. Only
/// Windows 11 takes the request; it lasts until it is closed.
fn ask_faster(
    device: &BluetoothLEDevice,
) -> Option<BluetoothLEPreferredConnectionParametersRequest> {
    let asked = BluetoothLEPreferredConnectionParameters::ThroughputOptimized()
        .and_then(|p| device.RequestPreferredConnectionParameters(&p));
    match asked {
        Ok(request) => {
            tracing::debug!(
                status = request.Status().map(|s| s.0).unwrap_or(-1),
                "Bluetooth: asked for a faster connection"
            );
            Some(request)
        }
        Err(e) => {
            tracing::debug!("Bluetooth: this Windows cannot be asked for a faster connection: {e}");
            None
        }
    }
}

fn on_disconnect(address: u64) {
    let conn = {
        let mut state = central().state.lock().unwrap();
        // To find it again when it comes back in pairing mode.
        state.passed.remove(&address);
        match state.peers.remove(&address) {
            Some(Peer::Up { feed, conn }) => {
                feed.close();
                state.links.remove(&switch2gatt::path(&key(address)));
                Some(conn)
            }
            // Being set up: `connect` sees it is gone.
            _ => None,
        }
    };
    if let Some(conn) = conn {
        tracing::info!("Bluetooth: Switch 2 controller disconnected");
        // Not inside the handler of the connection it closes.
        let _ = std::thread::Builder::new()
            .name("dp-ble-close".into())
            .spawn(move || drop(conn));
    }
}

/// Writes to a connected controller's characteristics.
struct GattWriter {
    command: GattCharacteristic,
    vibration: Option<GattCharacteristic>,
    /// A rumble report is on its way: the next one is dropped until it is out.
    rumbling: Arc<AtomicBool>,
}

impl Writer for GattWriter {
    fn write(&self, target: Target, data: Vec<u8>) {
        let sent = match target {
            Target::Command => self.command(&data),
            Target::Vibration => self.rumble(&data),
        };
        if let Err(Failure(e)) = sent {
            tracing::debug!("Bluetooth: writing to the controller failed: {e}");
        }
    }
}

impl GattWriter {
    /// A command, once it is out: commands go one after the other.
    fn command(&self, data: &[u8]) -> Result<(), Failure> {
        let op = self
            .command
            .WriteValueWithOptionAsync(&buffer(data)?, GattWriteOption::WriteWithoutResponse)?;
        succeeded(wait(op, Instant::now() + WRITE_TIMEOUT)?, "a command")
    }

    /// A rumble report, unless the last one is still on its way: rumble comes again soon.
    fn rumble(&self, data: &[u8]) -> Result<(), Failure> {
        let Some(ch) = &self.vibration else {
            return Ok(());
        };
        if self.rumbling.swap(true, Ordering::AcqRel) {
            tracing::trace!("Bluetooth: no room for a rumble report");
            return Ok(());
        }
        let rumbling = self.rumbling.clone();
        let sent = buffer(data)
            .and_then(|b| ch.WriteValueWithOptionAsync(&b, GattWriteOption::WriteWithoutResponse))
            .and_then(|op| {
                op.SetCompleted(&AsyncOperationCompletedHandler::new(move |_, _| {
                    rumbling.store(false, Ordering::Release);
                    Ok(())
                }))
            });
        if sent.is_err() {
            self.rumbling.store(false, Ordering::Release);
        }
        Ok(sent?)
    }
}

/// Wait for a WinRT operation; cancel it at `until`.
fn wait<T: RuntimeType + 'static>(op: IAsyncOperation<T>, until: Instant) -> Result<T, Failure> {
    let (done, finished) = crossbeam_channel::bounded(1);
    op.SetCompleted(&AsyncOperationCompletedHandler::new(move |_, _| {
        let _ = done.try_send(());
        Ok(())
    }))?;
    if finished
        .recv_timeout(until.saturating_duration_since(Instant::now()))
        .is_err()
    {
        let _ = op.Cancel();
        return Err(Failure("the controller did not answer in time".into()));
    }
    Ok(op.GetResults()?)
}

fn succeeded(status: GattCommunicationStatus, what: &str) -> Result<(), Failure> {
    let why = match status {
        GattCommunicationStatus::Success => return Ok(()),
        GattCommunicationStatus::Unreachable => "unreachable",
        GattCommunicationStatus::ProtocolError => "protocol error",
        GattCommunicationStatus::AccessDenied => "access denied",
        _ => "failed",
    };
    Err(Failure(format!("{what}: {why}")))
}

fn guid(uuid: &str) -> GUID {
    GUID::try_from(uuid).unwrap_or(GUID::zeroed())
}

fn buffer(data: &[u8]) -> windows::core::Result<IBuffer> {
    let writer = DataWriter::new()?;
    writer.WriteBytes(data)?;
    writer.DetachBuffer()
}

fn bytes(buffer: &IBuffer) -> windows::core::Result<Vec<u8>> {
    let mut out = vec![0; buffer.Length()? as usize];
    DataReader::FromBuffer(buffer)?.ReadBytes(&mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nintendo::Switch2Model;

    #[test]
    fn uuids_parse() {
        let models = [
            Switch2Model::Pro,
            Switch2Model::JoyConLeft,
            Switch2Model::JoyConRight,
            Switch2Model::GameCube,
        ];
        let uuids = [
            switch2ble::SERVICE,
            switch2ble::INPUT,
            switch2ble::COMMAND,
            switch2ble::REPLY,
        ]
        .into_iter()
        .chain(models.map(switch2ble::vibration));
        for uuid in uuids {
            let g = guid(uuid);
            assert_ne!(g, GUID::zeroed(), "{uuid}");
            assert_eq!(format!("{g:?}"), uuid);
        }
    }

    #[test]
    fn buffers_round_trip() {
        let data = [0x01, 0x02, 0xFE, 0xFF];
        assert_eq!(bytes(&buffer(&data).unwrap()).unwrap(), data);
        assert!(bytes(&buffer(&[]).unwrap()).unwrap().is_empty());
    }

    #[test]
    fn statuses_explain_themselves() {
        assert!(succeeded(GattCommunicationStatus::Success, "x").is_ok());
        let Err(Failure(why)) =
            succeeded(GattCommunicationStatus::Unreachable, "reading its services")
        else {
            panic!("not a failure");
        };
        assert_eq!(why, "reading its services: unreachable");
    }

    #[test]
    #[ignore = "listens to Bluetooth on this PC; a Switch 2 controller in pairing mode shows up"]
    fn advertisements_on_this_pc() {
        let watcher = BluetoothLEAdvertisementWatcher::new().unwrap();
        watcher
            .SetScanningMode(BluetoothLEScanningMode::Active)
            .unwrap();
        let seen = Arc::new(Mutex::new((0usize, Vec::new())));
        let log = seen.clone();
        watcher
            .Received(&TypedEventHandler::<
                BluetoothLEAdvertisementWatcher,
                BluetoothLEAdvertisementReceivedEventArgs,
            >::new(move |_, args| {
                let Some(args) = args.as_ref() else {
                    return Ok(());
                };
                let mut log = log.lock().unwrap();
                log.0 += 1;
                let sections = args
                    .Advertisement()?
                    .GetManufacturerDataByCompanyId(switch2ble::COMPANY_ID)?;
                for section in sections {
                    let mut data = switch2ble::COMPANY_ID.to_le_bytes().to_vec();
                    data.extend(bytes(&section.Data()?)?);
                    let advert = switch2ble::parse_advert(&data);
                    log.1.push((key(args.BluetoothAddress()?), advert, data));
                }
                Ok(())
            }))
            .unwrap();
        watcher.Start().unwrap();
        std::thread::sleep(Duration::from_secs(5));
        let status = watcher.Status().unwrap();
        watcher.Stop().unwrap();
        let (count, nintendo) = &*seen.lock().unwrap();
        println!("{count} advertisements");
        for (address, advert, data) in nintendo {
            println!("{address}: {advert:?} {data:02x?}");
        }
        assert_eq!(status, BluetoothLEAdvertisementWatcherStatus::Started);
        assert!(*count > 0, "nothing advertises near this PC");
    }

    #[test]
    #[ignore = "connects a Switch 2 Pro Controller in pairing mode near this PC"]
    fn controller_in_pairing_mode() {
        use crate::hidhost::{HidHandle, Wait};

        let until = Instant::now() + Duration::from_secs(30);
        let mut pad = None;
        while pad.is_none() && Instant::now() < until {
            pad = poll().ready.pop();
            std::thread::sleep(Duration::from_millis(100));
        }
        let mut pad = pad.expect("no controller in pairing mode connected");
        let start = Instant::now();
        let mut reports = 0u32;
        while start.elapsed() < Duration::from_secs(3) {
            if let Wait::Report(_) = pad.io.wait(Duration::from_millis(100)).unwrap() {
                reports += 1;
            }
        }
        println!(
            "{}: {:.1} reports a second",
            pad.id,
            f64::from(reports) / 3.0
        );
        assert!(reports > 0, "connected, but no input");
    }

    #[test]
    fn keys_are_addresses() {
        assert_eq!(key(0x98B6_E912_3456), "98b6e9123456");
        assert_eq!(key(0x12), "000000000012");
    }
}
