//! BlueZ on the system bus ([`super::sysbus`]), as far as Doppelpad needs it: whether
//! Bluetooth can be used, the adapters and their discovery, the properties and signals of
//! devices, and ending a device's connection.

use std::collections::HashMap;
use std::pin::Pin;
use std::task::Poll;

use zbus::blocking::Connection;
use zbus::export::futures_core::Stream;
use zbus::message::Type as MessageType;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{MatchRule, Message, MessageStream};

use crate::nintendo::switch2ble::{self, Advert};

pub const BLUEZ: &str = "org.bluez";
pub const ADAPTER: &str = "org.bluez.Adapter1";
pub const DEVICE: &str = "org.bluez.Device1";
pub const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";

/// Signals a client may fall behind by before the connection stops reading.
const QUEUED_SIGNALS: usize = 256;

pub type Properties = HashMap<String, OwnedValue>;
type Objects = HashMap<OwnedObjectPath, HashMap<String, Properties>>;

/// The system bus (see [`super::sysbus`]).
pub fn bus() -> zbus::Result<Connection> {
    super::sysbus::connection()
}

/// Whether Bluetooth LE devices can be connected, as a driver check: BlueZ running, with a
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

/// The object paths of the Bluetooth adapters, with whether each is on.
pub fn adapters(bus: &Connection) -> zbus::Result<Vec<(String, bool)>> {
    Ok(managed_objects(bus)?
        .into_iter()
        .filter_map(|(path, ifaces)| {
            let on = flag(ifaces.get(ADAPTER)?, "Powered");
            Some((path.as_str().to_owned(), on))
        })
        .collect())
}

/// Keep a discovery running on an adapter: Doppelpad's own, which BlueZ ends when Doppelpad
/// does. Asking again while it runs changes nothing.
///
/// Bluetooth LE only, and every advertisement told: what BlueZ knows of a device may be
/// minutes old, so only a fresh advertisement says a controller is in pairing mode now.
pub fn discover(bus: &Connection, adapter: &str) {
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

/// End Doppelpad's discovery on an adapter.
pub fn stop_discovery(bus: &Connection, adapter: &str) {
    if let Err(e) = bus.call_method(Some(BLUEZ), adapter, Some(ADAPTER), "StopDiscovery", &()) {
        tracing::debug!(adapter, "Bluetooth: cannot stop the discovery: {e}");
    }
}

/// Ask BlueZ to end the connection of the device at `device` (its object path), which turns a
/// controller off.
pub fn disconnect(device: &str) -> zbus::Result<()> {
    bus()?
        .call_method(Some(BLUEZ), device, Some(DEVICE), "Disconnect", &())
        .map(drop)
}

/// A text property of a BlueZ object.
pub fn text_property(
    bus: &Connection,
    path: &str,
    iface: &str,
    name: &str,
) -> zbus::Result<String> {
    let value: OwnedValue = bus
        .call_method(Some(BLUEZ), path, Some(PROPERTIES), "Get", &(iface, name))?
        .body()
        .deserialize()?;
    Ok(String::try_from(value)?)
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

/// The name of the D-Bus error BlueZ (or the bus) answered with.
pub fn error_name(e: &zbus::Error) -> Option<&str> {
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

/// What BlueZ told about its devices.
#[derive(Debug)]
pub enum Signal {
    /// A device was found (`InterfacesAdded`), or properties of one changed.
    Device { path: String, props: Properties },
    /// BlueZ forgot a device.
    Removed { path: String },
    /// Why a device's link ended (BlueZ 5.78 and later).
    Disconnected {
        path: String,
        reason: String,
        message: String,
    },
}

impl Signal {
    /// The signal a message carries, if it is about a device.
    fn of(message: &Message) -> Option<Signal> {
        let header = message.header();
        let path = header.path()?.as_str().to_owned();
        let body = message.body();
        match header.member()?.as_str() {
            "PropertiesChanged" => {
                let (iface, props, _) = body
                    .deserialize::<(String, Properties, Vec<String>)>()
                    .ok()?;
                (iface == DEVICE).then_some(Signal::Device { path, props })
            }
            "InterfacesAdded" => {
                let (path, mut ifaces) = body
                    .deserialize::<(OwnedObjectPath, HashMap<String, Properties>)>()
                    .ok()?;
                let props = ifaces.remove(DEVICE)?;
                Some(Signal::Device {
                    path: path.as_str().to_owned(),
                    props,
                })
            }
            "InterfacesRemoved" => {
                let (path, ifaces) = body.deserialize::<(OwnedObjectPath, Vec<String>)>().ok()?;
                ifaces.iter().any(|i| i == DEVICE).then(|| Signal::Removed {
                    path: path.as_str().to_owned(),
                })
            }
            "Disconnected" => {
                let (reason, message) = body.deserialize::<(String, String)>().ok()?;
                Some(Signal::Disconnected {
                    path,
                    reason,
                    message,
                })
            }
            _ => None,
        }
    }
}

/// The signals a client follows, and no more of BlueZ's: devices' properties (what a discovery
/// finds, links that end), devices BlueZ adds and forgets, and why a link ended.
fn rules() -> zbus::Result<[MatchRule<'static>; 3]> {
    let signal = || {
        MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender(BLUEZ)
    };
    Ok([
        signal()?
            .interface(PROPERTIES)?
            .member("PropertiesChanged")?
            .arg(0, DEVICE)?
            .build(),
        signal()?.interface(OBJECT_MANAGER)?.build(),
        signal()?.interface(DEVICE)?.member("Disconnected")?.build(),
    ])
}

/// BlueZ's signals about its devices, as they come, until the connection breaks (`None`).
pub struct Signals {
    streams: Vec<MessageStream>,
    /// The stream asked first next time, so that none is starved.
    next: usize,
}

impl Signals {
    pub fn follow(bus: &Connection) -> zbus::Result<Signals> {
        let mut streams = Vec::new();
        for rule in rules()? {
            let stream = MessageStream::for_match_rule(rule, bus.inner(), Some(QUEUED_SIGNALS));
            streams.push(zbus::block_on(stream)?);
        }
        Ok(Signals { streams, next: 0 })
    }
}

impl Iterator for Signals {
    type Item = zbus::Result<Signal>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let message = zbus::block_on(std::future::poll_fn(|cx| {
                let n = self.streams.len();
                for i in 0..n {
                    let at = (self.next + i) % n;
                    if let Poll::Ready(m) = Pin::new(&mut self.streams[at]).poll_next(cx) {
                        self.next = (at + 1) % n;
                        return Poll::Ready(m);
                    }
                }
                Poll::Pending
            }))?;
            match message {
                Ok(m) => {
                    if let Some(signal) = Signal::of(&m) {
                        return Some(Ok(signal));
                    }
                }
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

/// The Switch 2 controller a device's manufacturer data (`a{qv}`, by company) describes.
pub fn advert(manufacturer: &Value<'_>) -> Option<Advert> {
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use zbus::blocking::MessageIterator;

    use super::*;
    use crate::nintendo::{Switch2Model, pid};

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

    /// The manufacturer data survives D-Bus: serialised as BlueZ sends it, and read back as a
    /// signal.
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
        let Some(Signal::Device { path, props }) = Signal::of(&msg) else {
            panic!("not a device's properties");
        };
        assert_eq!(path, "/org/bluez/hci0/dev_98_B6_E9_12_34_56");
        assert!(advert(&props["ManufacturerData"]).unwrap().pairing);
    }

    #[test]
    fn devices_bluez_forgets() {
        let msg = zbus::Message::signal("/", OBJECT_MANAGER, "InterfacesRemoved")
            .unwrap()
            .build(&(
                OwnedObjectPath::try_from("/org/bluez/hci0/dev_98_B6_E9_12_34_56").unwrap(),
                vec![DEVICE.to_owned()],
            ))
            .unwrap();
        assert!(matches!(
            Signal::of(&msg),
            Some(Signal::Removed { path }) if path.ends_with("dev_98_B6_E9_12_34_56")
        ));
    }

    #[test]
    fn rules_are_narrow() {
        let rules: Vec<String> = rules().unwrap().iter().map(ToString::to_string).collect();
        assert!(
            rules[0].contains("arg0='org.bluez.Device1'"),
            "{}",
            rules[0]
        );
        assert!(rules.iter().all(|r| r.contains("sender='org.bluez'")));
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
        let signals = Signals::follow(&bus).unwrap();
        for (path, _) in adapters(&bus).expect("BlueZ answers") {
            discover(&bus, &path);
        }
        let all = MessageIterator::for_match_rule(
            MatchRule::builder()
                .msg_type(MessageType::Signal)
                .sender(BLUEZ)
                .unwrap()
                .build(),
            &bus,
            Some(1024),
        )
        .unwrap();
        let counted = std::sync::Arc::new(std::sync::Mutex::new((0usize, Vec::new())));
        let (log, started) = (counted.clone(), Instant::now());
        std::thread::spawn(move || {
            for _ in all.flatten() {
                log.lock().unwrap().0 += 1;
            }
        });
        let log = counted.clone();
        std::thread::spawn(move || {
            for signal in signals.flatten() {
                if let Signal::Device { path, props } = signal
                    && let Some(a) = props.get("ManufacturerData").and_then(|m| advert(m))
                {
                    log.lock().unwrap().1.push(format!(
                        "{:5.2} s {path}: {a:?}",
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
}
