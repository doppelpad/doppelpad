//! The GameCube controller adapter on Windows. Windows binds its HID driver to it; programs
//! like Dolphin need WinUSB instead (installed with Zadig). Either works here: WinUSB is
//! tried first, then the HID interface.

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use dp_core::pad::Rumble;
use nusb::descriptors::TransferType;
use nusb::transfer::{
    Buffer, ControlOut, ControlType, In, Interrupt, Out, Recipient, TransferError,
};
use nusb::{Endpoint, Interface, MaybeFuture};
use windows_sys::Win32::Devices::HumanInterfaceDevice::GUID_DEVINTERFACE_HID;

use super::devnode::{enumerate_interfaces, usb_device_of};
use super::hidio::HidIo;
use super::process;
use super::usb::{endpoint_addrs, is_busy, transfer_error};
use crate::holders::busy_message;
use crate::nintendo::NINTENDO_VID;
use crate::nintendo::gcadapter::{
    self, ADAPTER_PID, Adapter, PORTS, Port, PortKind, START, rumble_report,
};
use crate::padio::ReportIo;
use crate::scan::{RESCAN, Rescanner};
use crate::{DeviceId, DeviceInfo, GamepadBackend, PadFamily, PadSnapshot};

const READ_WAIT: Duration = Duration::from_millis(50);
/// The stronger motor at this level or more turns the plain GameCube motor on.
const RUMBLE_ON: f32 = 0.2;

struct WinUsb {
    out: Endpoint<Interrupt, Out>,
    input: Endpoint<Interrupt, In>,
    _interface: Interface,
}

impl ReportIo for WinUsb {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.out
            .transfer_blocking(Buffer::from(data.to_vec()), Duration::from_secs(1))
            .status
            .map_err(transfer_error)
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        let packet = self.input.max_packet_size().max(37);
        let done = self.input.transfer_blocking(Buffer::new(packet), timeout);
        match done.status {
            Ok(()) => {
                let n = done.actual_len.min(buf.len());
                buf[..n].copy_from_slice(&done.buffer[..n]);
                Ok(Some(n))
            }
            Err(TransferError::Cancelled) => Ok(None),
            Err(e) => Err(transfer_error(e)),
        }
    }
}

fn open_winusb(info: &nusb::DeviceInfo) -> Result<WinUsb, nusb::Error> {
    let device = info.open().wait()?;
    let interface = device.claim_interface(0).wait()?;
    // Some clones need report mode set first; others stall the request.
    let set_protocol = ControlOut {
        control_type: ControlType::Class,
        recipient: Recipient::Interface,
        request: 11,
        value: 1,
        index: 0,
        data: &[],
    };
    if let Err(e) = interface
        .control_out(set_protocol, Duration::from_secs(1))
        .wait()
    {
        tracing::debug!("GameCube adapter: SET_PROTOCOL: {e}");
    }
    let (out_addr, in_addr) = endpoint_addrs(&interface, TransferType::Interrupt, (0x02, 0x81));
    Ok(WinUsb {
        out: interface.endpoint::<Interrupt, Out>(out_addr)?,
        input: interface.endpoint::<Interrupt, In>(in_addr)?,
        _interface: interface,
    })
}

/// One adapter and its four ports, shared with its thread.
struct Shared {
    /// Device id prefix: `gcadapter:<instance>`.
    key: String,
    /// The HID instance to hide, when the adapter runs on the HID driver.
    hide: Option<String>,
    ports: Mutex<[Option<Port>; PORTS]>,
    rumble: Mutex<[bool; PORTS]>,
    infos: [Arc<DeviceInfo>; PORTS],
    stop: AtomicBool,
    done: AtomicBool,
}

impl Shared {
    fn new(instance: &str, hide: Option<String>) -> Shared {
        let key = format!("gcadapter:{instance}");
        let infos = std::array::from_fn(|i| {
            Arc::new(DeviceInfo {
                id: DeviceId(format!("{key}:{}", i + 1)),
                name: format!("GameCube Controller (adapter port {})", i + 1),
                family: PadFamily::GameCube,
                vendor_id: NINTENDO_VID,
                product_id: ADAPTER_PID,
                backend: "gcadapter".into(),
                instance_paths: hide.iter().cloned().collect(),
                joycon: None,
                has_motion: false,
                has_touchpad: false,
                has_rumble: true,
            })
        });
        Shared {
            key,
            hide,
            ports: Mutex::new([None; PORTS]),
            rumble: Mutex::new([false; PORTS]),
            infos,
            stop: AtomicBool::new(false),
            done: AtomicBool::new(false),
        }
    }
}

fn run(mut io: Box<dyn ReportIo + Send>, shared: &Shared) {
    if let Err(e) = io.write(&START) {
        tracing::warn!("GameCube adapter did not start: {e}");
        return;
    }
    tracing::info!(adapter = %shared.key, hid = shared.hide.is_some(), "GameCube adapter connected");
    let mut adapter = Adapter::default();
    let mut sent = [false; PORTS];
    let mut buf = [0u8; 64];
    while !shared.stop.load(Ordering::Relaxed) {
        match io.read(&mut buf, READ_WAIT) {
            Ok(Some(n)) => {
                if let Some(ports) = adapter.parse(&buf[..n]) {
                    *shared.ports.lock().unwrap() = ports;
                }
            }
            Ok(None) => {}
            Err(e) => {
                tracing::info!(adapter = %shared.key, "GameCube adapter gone: {e}");
                return;
            }
        }
        let ports = *shared.ports.lock().unwrap();
        let mut want = *shared.rumble.lock().unwrap();
        for (w, p) in want.iter_mut().zip(ports) {
            // A WaveBird has no motor, and an empty port nothing to shake.
            *w &= p.is_some_and(|p| p.kind == PortKind::Wired);
        }
        if want != sent {
            if let Err(e) = io.write(&rumble_report(want)) {
                tracing::debug!("GameCube adapter rumble: {e}");
            }
            sent = want;
        }
    }
    if sent.iter().any(|&m| m) {
        let _ = io.write(&rumble_report([false; PORTS]));
    }
}

/// An adapter's thread and what it shares.
struct Running {
    shared: Arc<Shared>,
    thread: JoinHandle<()>,
}

type Adapters = Arc<Mutex<HashMap<String, Running>>>;

pub struct GcAdapterBackend {
    adapters: Adapters,
    problems: Arc<Mutex<Vec<String>>>,
    scanner: Rescanner,
}

impl GcAdapterBackend {
    pub fn new() -> Self {
        let adapters: Adapters = Arc::default();
        let problems: Arc<Mutex<Vec<String>>> = Arc::default();
        let (a, p) = (adapters.clone(), problems.clone());
        let scanner = Rescanner::spawn("dp-gcadapter-scan", RESCAN, move |_| scan(&a, &p));
        GcAdapterBackend {
            adapters,
            problems,
            scanner,
        }
    }
}

impl Default for GcAdapterBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn start(adapters: &Adapters, instance: &str, hide: Option<String>, io: Box<dyn ReportIo + Send>) {
    let shared = Arc::new(Shared::new(instance, hide));
    let thread_shared = shared.clone();
    let spawned = std::thread::Builder::new()
        .name("dp-gcadapter".into())
        .spawn(move || {
            run(io, &thread_shared);
            *thread_shared.ports.lock().unwrap() = [None; PORTS];
            thread_shared.done.store(true, Ordering::Relaxed);
        });
    match spawned {
        Ok(thread) => {
            adapters
                .lock()
                .unwrap()
                .insert(instance.to_ascii_uppercase(), Running { shared, thread });
        }
        Err(e) => tracing::warn!("cannot start the GameCube adapter thread: {e}"),
    }
}

fn scan(adapters: &Adapters, problems: &Mutex<Vec<String>>) {
    adapters
        .lock()
        .unwrap()
        .retain(|_, a| !a.shared.done.load(Ordering::Relaxed));
    let open = |instance: &str| {
        adapters
            .lock()
            .unwrap()
            .contains_key(&instance.to_ascii_uppercase())
    };
    // On the HID driver (Windows' default). Keyed by the USB device, as WinUSB ones are.
    let pattern = format!("VID_{NINTENDO_VID:04X}&PID_{ADAPTER_PID:04X}");
    for entry in enumerate_interfaces(&GUID_DEVINTERFACE_HID) {
        if !entry.instance_id.to_ascii_uppercase().contains(&pattern) {
            continue;
        }
        let instance = usb_device_of(entry.devinst).unwrap_or_else(|| entry.instance_id.clone());
        if open(&instance) {
            continue;
        }
        match HidIo::open(&entry.path) {
            Ok(io) if io.caps().output_len > 0 => start(
                adapters,
                &instance,
                Some(entry.instance_id.clone()),
                Box::new(io),
            ),
            Ok(_) => {}
            Err(e) => tracing::debug!("GameCube adapter HID interface: {e}"),
        }
    }
    // On WinUSB (as Dolphin needs it).
    let (mut unusable, mut busy) = (0, false);
    if let Ok(list) = nusb::list_devices().wait() {
        for info in list.filter(|d| gcadapter::is_adapter(d.vendor_id(), d.product_id())) {
            let instance = info.instance_id().to_string_lossy().into_owned();
            if open(&instance) {
                continue;
            }
            match open_winusb(&info) {
                Ok(io) => start(adapters, &instance, None, Box::new(io)),
                Err(e) => {
                    tracing::debug!("GameCube adapter cannot be opened: {e}");
                    unusable += 1;
                    busy |= is_busy(&e);
                }
            }
        }
    }
    let mut p = problems.lock().unwrap();
    p.clear();
    if busy {
        p.push(busy_message(
            "the GameCube adapter",
            process::likely_holder(),
        ));
    } else if unusable > 0 {
        p.push(
            "the GameCube adapter cannot be opened: another program may be using it, or it \
             needs the WinUSB driver (for example installed with Zadig)"
                .into(),
        );
    }
}

impl Drop for GcAdapterBackend {
    fn drop(&mut self) {
        // No new adapter may start once the list is drained.
        self.scanner.stop();
        let adapters: Vec<Running> = self
            .adapters
            .lock()
            .unwrap()
            .drain()
            .map(|(_, a)| a)
            .collect();
        for a in &adapters {
            a.shared.stop.store(true, Ordering::Relaxed);
        }
        // Each thread turns the motors off before it ends.
        for a in adapters {
            let _ = a.thread.join();
        }
    }
}

impl GamepadBackend for GcAdapterBackend {
    fn name(&self) -> &'static str {
        "gcadapter"
    }

    fn poll(&mut self, out: &mut Vec<PadSnapshot>) {
        for a in self.adapters.lock().unwrap().values() {
            let a = &a.shared;
            let ports = *a.ports.lock().unwrap();
            for (port, info) in ports.iter().zip(&a.infos) {
                if let Some(p) = port {
                    out.push(PadSnapshot {
                        info: info.clone(),
                        state: p.state,
                        battery: None,
                        raw: None,
                    });
                }
            }
        }
    }

    fn set_rumble(&mut self, id: &DeviceId, rumble: Rumble) {
        for a in self.adapters.lock().unwrap().values() {
            let a = &a.shared;
            if let Some(i) = a.infos.iter().position(|info| &info.id == id) {
                a.rumble.lock().unwrap()[i] = rumble.low.max(rumble.high) >= RUMBLE_ON;
                return;
            }
        }
    }

    fn problems(&self) -> Vec<String> {
        self.problems.lock().unwrap().clone()
    }

    fn rescan(&mut self) {
        self.scanner.wake();
    }
}
