//! Switch 2 controllers over Bluetooth LE, through WinRT (`Windows.Devices.Bluetooth`).
//!
//! Windows cannot pair them (see [`crate::nintendo::switch2ble`]), so Doppelpad watches the
//! advertisements for one in pairing mode, connects it without pairing and subscribes to its
//! input. The controller then runs in the HID backend like any other, over a
//! [`crate::switch2gatt`] link. The watcher runs only while the search is on
//! ([`crate::Switch2Search`]).
//!
//! The pads never ask for a faster connection, so Doppelpad asks Windows for one. Windows 11
//! can be asked; Windows 10 keeps its default, which in tests brought the reports unevenly:
//! at times a steady 17 a second, at times 15 to 105 (about 45 on average).
//!
//! The watcher's events arrive on the thread pool. Each controller is set up on a thread of
//! its own, as that waits for the controller's answers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementWatcher,
    BluetoothLEAdvertisementWatcherStatus, BluetoothLEManufacturerData, BluetoothLEScanningMode,
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

use crate::bdaddr::{BdAddr, LeAddrType};
use crate::nintendo::Switch2Model;
use crate::nintendo::switch2ble::{self, Advert, Role};
use crate::switch2gatt::{
    self, Attempt, CONNECT_TIMEOUT, Feed, Gone, Peers, Poll, Ready, SILENCE, Target, WRITE_TIMEOUT,
    Writer,
};

/// Start the watcher again this long after it stopped (Bluetooth was off, or taken away).
const RESTART: Duration = Duration::from_secs(10);
/// How long the driver check waits for Windows to tell about the Bluetooth adapter.
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Whether Switch 2 controllers can connect, as a driver check: a Bluetooth adapter that
/// speaks Bluetooth LE, switched on. Every answer is waited for at most [`CHECK_TIMEOUT`].
pub fn check() -> crate::Result<()> {
    let unusable = |why: String| Err(crate::PlatformError::Other(why));
    let until = Instant::now() + CHECK_TIMEOUT;
    let Ok(adapter) = BluetoothAdapter::GetDefaultAsync()
        .map_err(Failure::from)
        .and_then(|op| wait(op, until))
    else {
        return unusable("no Bluetooth adapter".into());
    };
    if !adapter.IsLowEnergySupported().unwrap_or(false) {
        return unusable("the Bluetooth adapter does not support Bluetooth LE".into());
    }
    let state = adapter
        .GetRadioAsync()
        .map_err(Failure::from)
        .and_then(|op| wait(op, until))
        .and_then(|radio| Ok(radio.State()?));
    match state {
        Ok(RadioState::On) => Ok(()),
        Ok(_) => unusable("Bluetooth is off".into()),
        Err(Failure(e)) => unusable(format!("cannot read the state of Bluetooth: {e}")),
    }
}

/// What changed since the last call, with the search on or off as `search` says. Watches for
/// controllers from the first call on; controllers silent for too long are let go.
pub(crate) fn poll(search: bool) -> Poll {
    let c = central();
    c.searching.store(search, Ordering::Relaxed);
    keep_watching(&mut lock(&c.watcher), search);
    let silent = lock(&c.peers).silent();
    for address in silent {
        tracing::info!(
            "Bluetooth: no input from the Switch 2 controller for {} s, letting it go",
            SILENCE.as_secs()
        );
        // Not in a handler: the connection may be closed right here, after the lock (closing
        // it may call back).
        let gone = lock(&c.peers).let_go(&address);
        drop(gone);
    }
    lock(&c.peers).poll()
}

/// Stop watching and let every controller go, when the backend stops: nothing else would end
/// their links before Doppelpad ends.
pub(crate) fn let_all_go() {
    let Some(c) = CENTRAL.get() else {
        return;
    };
    keep_watching(&mut lock(&c.watcher), false);
    let addresses = lock(&c.peers).keys();
    for address in addresses {
        let gone = lock(&c.peers).let_go(&address);
        drop(gone);
    }
}

static CENTRAL: OnceLock<Central> = OnceLock::new();

fn central() -> &'static Central {
    CENTRAL.get_or_init(Central::default)
}

#[derive(Default)]
struct Central {
    watcher: Mutex<Watcher>,
    /// Whether the search is on, as the scanner last told.
    searching: AtomicBool,
    /// Controllers being connected or connected, by Bluetooth address.
    peers: Mutex<Peers<BdAddr, Conn>>,
}

/// Lock the client's state even after a handler panicked.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
struct Watcher {
    watcher: Option<BluetoothLEAdvertisementWatcher>,
    /// When it was last started.
    started: Option<Instant>,
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

/// Start the watcher, and start it again after Windows stopped it; stop it while the search
/// is off.
fn keep_watching(w: &mut Watcher, search: bool) {
    if !search {
        if let Some(x) = w.watcher.take() {
            let _ = x.Stop();
            tracing::info!("Bluetooth: no longer looking for Switch 2 controllers");
        }
        w.started = None;
        return;
    }
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
    // Listening is enough: a controller sends its manufacturer data in the advertisement
    // itself (a connectable one), not in a scan response, so no scan requests go out.
    watcher.SetScanningMode(BluetoothLEScanningMode::Passive)?;
    // Only advertisements with Nintendo's manufacturer data come through: the Bluetooth stack
    // drops every other device's (phones, headphones) instead of handing each to Doppelpad.
    let nintendo = BluetoothLEManufacturerData::new()?;
    nintendo.SetCompanyId(switch2ble::COMPANY_ID)?;
    watcher
        .AdvertisementFilter()?
        .Advertisement()?
        .ManufacturerData()?
        .Append(&nintendo)?;
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
    let Some(address) = BdAddr::new(args.BluetoothAddress()?) else {
        return Ok(());
    };
    let kind = match args.BluetoothAddressType()? {
        BluetoothAddressType::Random => LeAddrType::Random,
        _ => LeAddrType::Public,
    };
    for section in sections {
        // Windows takes the company identifier off the front.
        let mut data = switch2ble::COMPANY_ID.to_le_bytes().to_vec();
        data.extend(bytes(&section.Data()?)?);
        if let Some(advert) = switch2ble::parse_advert(&data) {
            discovered(address, kind, advert);
        }
    }
    Ok(())
}

fn discovered(address: BdAddr, kind: LeAddrType, advert: Advert) {
    let c = central();
    let mut peers = lock(&c.peers);
    if !peers.admit(&address, &advert, c.searching.load(Ordering::Relaxed)) {
        return;
    }
    let attempt = peers.connecting(address, ());
    let spawned = std::thread::Builder::new()
        .name("dp-ble-connect".into())
        .spawn(move || connect(address, kind, advert, attempt));
    if let Err(e) = spawned {
        tracing::warn!("Bluetooth: cannot connect the controller: {e}");
        peers.failed(&address, attempt);
    }
}

/// Set a controller up and hand it to the scanner.
fn connect(address: BdAddr, kind: LeAddrType, advert: Advert, attempt: Attempt) {
    let opened = open(address, kind, advert, attempt);
    // Let a connection go only after the lock: closing it may call back.
    let unwanted = {
        let mut peers = lock(&central().peers);
        match opened {
            // Not when it disconnected while it was set up.
            Ok((feed, conn, ready)) => peers.up(&address, attempt, feed, conn, ready).err(),
            Err(Failure(e)) => {
                tracing::info!("Bluetooth: connecting the controller failed: {e}");
                peers.failed(&address, attempt);
                None
            }
        }
    };
    drop(unwanted);
}

/// The controller of `attempt` is gone: Windows says so, or nothing reads it any more. Called
/// in a handler of the connection, so the connection is closed on a thread of its own.
fn on_disconnect(address: BdAddr, attempt: Attempt) {
    let Some(Gone::Up(conn)) = lock(&central().peers).let_go_attempt(&address, attempt) else {
        // Being set up: `connect` sees it is gone.
        return;
    };
    tracing::info!("Bluetooth: Switch 2 controller disconnected");
    close_elsewhere(conn);
}

/// Drop a connection on a thread of its own: not inside a handler of the connection it closes.
/// Should no thread start, the connection is leaked rather than closed in the handler.
fn close_elsewhere(conn: Conn) {
    let conn = Arc::new(Mutex::new(Some(conn)));
    let closing = conn.clone();
    let spawned = std::thread::Builder::new()
        .name("dp-ble-close".into())
        .spawn(move || drop(lock(&closing).take()));
    if let Err(e) = spawned {
        tracing::warn!("Bluetooth: cannot close the link to the controller: {e}");
        std::mem::forget(lock(&conn).take());
    }
}

/// Connect a controller, find its characteristics and switch its notifications on.
fn open(
    address: BdAddr,
    kind: LeAddrType,
    advert: Advert,
    attempt: Attempt,
) -> Result<(Feed, Conn, Ready), Failure> {
    let until = Instant::now() + CONNECT_TIMEOUT;
    let mut conn = connect_device(address, kind, until)?;
    let chars = characteristics(&mut conn, advert.model, until)?;
    let (feed, io, commands) = switch2gatt::link(Box::new(GattWriter {
        command: chars.command,
        vibration: chars.vibration,
        rumbling: Arc::default(),
    }));
    subscribe(
        &mut conn,
        &feed,
        [chars.input, chars.reply],
        until,
        move || {
            on_disconnect(address, attempt);
        },
    )?;
    conn.status = Some(conn.device.ConnectionStatusChanged(&TypedEventHandler::<
        BluetoothLEDevice,
        IInspectable,
    >::new(move |device, _| {
        if let Some(device) = device.as_ref()
            && device.ConnectionStatus()? == BluetoothConnectionStatus::Disconnected
        {
            on_disconnect(address, attempt);
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
    let ready = Ready::new(&address.to_string(), advert, io, commands);
    Ok((feed, conn, ready))
}

/// Connect the device and open a GATT session that keeps the link up.
fn connect_device(address: BdAddr, kind: LeAddrType, until: Instant) -> Result<Conn, Failure> {
    let kind = match kind {
        LeAddrType::Public => BluetoothAddressType::Public,
        LeAddrType::Random => BluetoothAddressType::Random,
    };
    let device = wait(
        BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync(address.get(), kind)?,
        until,
    )?;
    let session = wait(
        GattSession::FromDeviceIdAsync(&device.BluetoothDeviceId()?)?,
        until,
    )?;
    let conn = Conn {
        device,
        session,
        service: None,
        notifying: Vec::new(),
        status: None,
        faster: None,
    };
    // Nothing paired the controller, so nothing else keeps the link up.
    conn.session.SetMaintainConnection(true)?;
    Ok(conn)
}

/// A controller's characteristics.
struct Characteristics {
    input: GattCharacteristic,
    command: GattCharacteristic,
    reply: GattCharacteristic,
    vibration: Option<GattCharacteristic>,
}

/// Find the controller's service and its characteristics; the service stays with `conn`.
fn characteristics(
    conn: &mut Conn,
    model: Switch2Model,
    until: Instant,
) -> Result<Characteristics, Failure> {
    let services = wait(
        conn.device.GetGattServicesForUuidWithCacheModeAsync(
            GUID::from_u128(switch2ble::SERVICE),
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
    for ch in found.Characteristics()? {
        let slot = match switch2ble::role(ch.Uuid()?.to_u128(), model) {
            Some(Role::Input) => &mut input,
            Some(Role::Command) => &mut command,
            Some(Role::Reply) => &mut reply,
            Some(Role::Vibration) => &mut vibration,
            None => continue,
        };
        *slot = Some(ch);
    }
    let (Some(input), Some(command), Some(reply)) = (input, command, reply) else {
        return Err(Failure("it lacks input or command characteristics".into()));
    };
    Ok(Characteristics {
        input,
        command,
        reply,
        vibration,
    })
}

/// Feed the input and the replies into `feed` and switch their notifications on. Input that
/// nobody reads any more calls `gone`.
fn subscribe(
    conn: &mut Conn,
    feed: &Feed,
    [input, reply]: [GattCharacteristic; 2],
    until: Instant,
    gone: impl Fn() + Send + Sync + 'static,
) -> Result<(), Failure> {
    let gone = Arc::new(gone);
    for (ch, is_input) in [(input, true), (reply, false)] {
        let (feed, gone) = (feed.clone(), gone.clone());
        let token = ch.ValueChanged(&TypedEventHandler::<
            GattCharacteristic,
            GattValueChangedEventArgs,
        >::new(move |_, args| {
            if let Some(args) = args.as_ref() {
                let value = bytes(&args.CharacteristicValue()?)?;
                if !is_input {
                    feed.reply(value);
                } else if !feed.input(value) {
                    gone();
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
    Ok(())
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
        return Err(Failure("no answer in time".into()));
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

    /// The numbers are the GUIDs Windows names the characteristics by.
    #[test]
    fn uuids_are_windows_guids() {
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
            let g = GUID::from_u128(uuid);
            assert_eq!(format!("{g:?}"), switch2ble::uuid_text(uuid));
            assert_eq!(g.to_u128(), uuid);
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

    /// Listens 5 s with a passive scan, then 5 s with an active one, and counts what each
    /// hears: all advertisements, and those of Switch 2 controllers by kind.
    #[test]
    #[ignore = "listens to Bluetooth on this PC; a Switch 2 controller in pairing mode shows up"]
    fn advertisements_on_this_pc() {
        use std::collections::BTreeMap;

        use windows::Devices::Bluetooth::Advertisement::BluetoothLEAdvertisementType;

        let modes = [
            ("passive", BluetoothLEScanningMode::Passive),
            ("active", BluetoothLEScanningMode::Active),
        ];
        let mut heard = 0;
        for (name, mode) in modes {
            let watcher = BluetoothLEAdvertisementWatcher::new().unwrap();
            watcher.SetScanningMode(mode).unwrap();
            let seen = Arc::new(Mutex::new((0usize, BTreeMap::new())));
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
                        // A passive scan hears no scan responses.
                        let kind = match args.AdvertisementType()? {
                            BluetoothLEAdvertisementType::ConnectableUndirected => "connectable",
                            BluetoothLEAdvertisementType::ScanResponse => "scan response",
                            _ => "other",
                        };
                        let what = format!(
                            "{:012x} ({kind}): {:?}",
                            args.BluetoothAddress()?,
                            switch2ble::parse_advert(&data)
                        );
                        *log.1.entry(what).or_insert(0) += 1;
                    }
                    Ok(())
                }))
                .unwrap();
            watcher.Start().unwrap();
            std::thread::sleep(Duration::from_secs(5));
            let status = watcher.Status().unwrap();
            watcher.Stop().unwrap();
            let (count, switch2) = &*seen.lock().unwrap();
            println!("{name} scan: {count} advertisements, status {}", status.0);
            for (what, n) in switch2 {
                println!("  {n} x {what}");
            }
            heard += count;
        }
        assert!(heard > 0, "nothing advertises near this PC");
    }

    /// Listens 5 s unfiltered and picks the company whose manufacturer data comes most often,
    /// then 5 s through a filter on that company like [`new_watcher`]'s (a company identifier,
    /// no data): its advertisements come through, and only those.
    #[test]
    #[ignore = "listens to Bluetooth on this PC; needs a device nearby that sends manufacturer data"]
    fn the_filter_lets_one_company_through() {
        use std::collections::BTreeMap;

        // The companies in each advertisement's manufacturer data.
        let listen = |company: Option<u16>| -> Vec<Vec<u16>> {
            let watcher = BluetoothLEAdvertisementWatcher::new().unwrap();
            watcher
                .SetScanningMode(BluetoothLEScanningMode::Passive)
                .unwrap();
            if let Some(id) = company {
                let filter = BluetoothLEManufacturerData::new().unwrap();
                filter.SetCompanyId(id).unwrap();
                watcher
                    .AdvertisementFilter()
                    .and_then(|f| f.Advertisement())
                    .and_then(|a| a.ManufacturerData())
                    .and_then(|m| m.Append(&filter))
                    .unwrap();
            }
            let seen = Arc::new(Mutex::new(Vec::new()));
            let log = seen.clone();
            watcher
                .Received(&TypedEventHandler::<
                    BluetoothLEAdvertisementWatcher,
                    BluetoothLEAdvertisementReceivedEventArgs,
                >::new(move |_, args| {
                    if let Some(args) = args.as_ref() {
                        let companies = args
                            .Advertisement()?
                            .ManufacturerData()?
                            .into_iter()
                            .map(|section| section.CompanyId())
                            .collect::<windows::core::Result<Vec<u16>>>()?;
                        log.lock().unwrap().push(companies);
                    }
                    Ok(())
                }))
                .unwrap();
            watcher.Start().unwrap();
            std::thread::sleep(Duration::from_secs(5));
            watcher.Stop().unwrap();
            seen.lock().unwrap().clone()
        };

        let all = listen(None);
        let mut counts = BTreeMap::<u16, usize>::new();
        for id in all.iter().flatten() {
            *counts.entry(*id).or_default() += 1;
        }
        println!(
            "unfiltered: {} advertisements, by company {counts:04x?}",
            all.len()
        );
        let company = counts
            .iter()
            .max_by_key(|(_, n)| **n)
            .map(|(id, _)| *id)
            .expect("no manufacturer data near this PC");

        let filtered = listen(Some(company));
        println!(
            "filtered on {company:04x}: {} advertisements",
            filtered.len()
        );
        assert!(
            !filtered.is_empty(),
            "the company's advertisements come through"
        );
        assert!(
            filtered.iter().all(|ids| ids.contains(&company)),
            "and only those: {filtered:04x?}"
        );
    }

    /// Connects a Pro Controller in pairing mode near this PC and drives it for a while; see
    /// [`switch2gatt::drive_controller`]. Run with `--nocapture`.
    #[test]
    #[ignore = "connects a Switch 2 Pro Controller in pairing mode near this PC"]
    fn switch2_pro_over_bluetooth() {
        switch2gatt::drive_controller(|| poll(true));
        let_all_go();
    }
}
