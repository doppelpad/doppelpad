//! Switch 2 controllers on USB through hidraw. The kernel has no driver that starts them, so
//! their HID interface stays silent until their command channel ([`crate::switch2usb`]) has
//! started them; then the HID backend reads its input reports here. `usbhid` gives the
//! interface a generic input node too, which is grabbed to hide the controller.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::dev::set_grab;
use crate::hidhost::{HidHandle, Wait, Wake};
use crate::nintendo::{self, Model, Switch2Model};
use crate::padio::ReportIo;

/// The bus of a USB device in `HID_ID`.
const BUS_USB: u32 = 0x0003;
/// The controllers' output reports (rumble); shorter ones are zero-padded to it.
const OUTPUT_LEN: usize = 64;
/// Room for the largest input report.
const INPUT_LEN: usize = 256;

/// A Switch 2 controller's HID interface on USB, as sysfs lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    /// `/dev/hidrawN`.
    pub node: PathBuf,
    pub vid: u16,
    pub pid: u16,
    pub model: Switch2Model,
    /// Where it is plugged in (`usb-0000:04:00.3-1.3/input0`), the same when it is plugged in
    /// there again.
    pub phys: String,
    /// The USB device in sysfs, which has the command channel too.
    pub usb_device: PathBuf,
    /// The input nodes the kernel made for the interface.
    pub event_nodes: Vec<PathBuf>,
}

/// The Switch 2 controllers on USB.
pub(crate) fn switch2_devices() -> Vec<Entry> {
    let Ok(dir) = std::fs::read_dir("/sys/class/hidraw") else {
        return Vec::new();
    };
    let mut out: Vec<Entry> = dir.flatten().filter_map(|e| entry(&e.path())).collect();
    out.sort_by(|a, b| a.node.cmp(&b.node));
    out
}

/// The controller behind `/sys/class/hidraw/hidrawN`, if it is a Switch 2 controller on USB.
fn entry(class: &Path) -> Option<Entry> {
    let uevent = std::fs::read_to_string(class.join("device/uevent")).ok()?;
    let ids = parse_uevent(&uevent)?;
    if ids.bus != BUS_USB {
        return None;
    }
    let Some(Model::Switch2(model)) = nintendo::model_for(ids.vid, ids.pid) else {
        return None;
    };
    let hid = std::fs::canonicalize(class.join("device")).ok()?;
    // The HID device sits on its USB interface, which sits on the USB device.
    let usb_device = hid.parent()?.parent()?.to_path_buf();
    Some(Entry {
        node: Path::new("/dev").join(class.file_name()?),
        vid: ids.vid,
        pid: ids.pid,
        model,
        phys: ids.phys,
        usb_device,
        event_nodes: input_nodes(&hid),
    })
}

/// What a HID device's `uevent` file says about it.
#[derive(Debug, PartialEq, Eq)]
struct Uevent {
    bus: u32,
    vid: u16,
    pid: u16,
    phys: String,
}

/// `HID_ID=0003:0000057E:00002069` and `HID_PHYS=…` from a HID device's `uevent` file.
fn parse_uevent(text: &str) -> Option<Uevent> {
    let field = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
    };
    let mut ids = field("HID_ID")?.split(':');
    let mut hex = || u32::from_str_radix(ids.next()?, 16).ok();
    let (bus, vid, pid) = (hex()?, hex()?, hex()?);
    Some(Uevent {
        bus,
        vid: u16::try_from(vid).ok()?,
        pid: u16::try_from(pid).ok()?,
        phys: field("HID_PHYS").unwrap_or_default().to_owned(),
    })
}

/// The `/dev/input/eventN` nodes of a HID device (sysfs `<hid>/input/inputM/eventN`).
fn input_nodes(hid: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for input in std::fs::read_dir(hid.join("input"))
        .into_iter()
        .flatten()
        .flatten()
    {
        for node in std::fs::read_dir(input.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            let name = node.file_name();
            if name.to_string_lossy().starts_with("event") {
                out.push(Path::new("/dev/input").join(name));
            }
        }
    }
    out.sort();
    out
}

/// An open hidraw node, as the controller's device thread uses it.
pub(crate) struct Hidraw {
    file: File,
    wake: Arc<EventFd>,
    report: Vec<u8>,
    len: usize,
    event_nodes: Vec<PathBuf>,
    /// The input nodes, open and grabbed while the controller is hidden.
    grabbed: Vec<File>,
}

impl Hidraw {
    pub fn open(entry: &Entry) -> io::Result<Hidraw> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&entry.node)?;
        Ok(Hidraw {
            file,
            wake: Arc::new(EventFd::new()?),
            report: vec![0; INPUT_LEN],
            len: 0,
            event_nodes: entry.event_nodes.clone(),
            grabbed: Vec::new(),
        })
    }
}

/// Wakes a device thread waiting in [`Hidraw::wait`].
struct EventFd(OwnedFd);

impl EventFd {
    fn new() -> io::Result<EventFd> {
        // SAFETY: plain eventfd creation; the descriptor is owned at once.
        let fd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a descriptor that was just created and is owned by nothing else.
        Ok(EventFd(unsafe { OwnedFd::from_raw_fd(fd) }))
    }

    /// Take the wake-ups that came, so the next wait blocks again.
    fn drain(&self) {
        let mut count = 0u64;
        // SAFETY: reading the eventfd's 8-byte counter into a u64.
        unsafe { libc::read(self.0.as_raw_fd(), (&raw mut count).cast(), 8) };
    }
}

impl Wake for EventFd {
    fn wake(&self) {
        let one = 1u64;
        // SAFETY: writing an 8-byte count from a u64 to an eventfd.
        unsafe { libc::write(self.0.as_raw_fd(), (&raw const one).cast(), 8) };
    }
}

impl HidHandle for Hidraw {
    fn wait(&mut self, timeout: Duration) -> io::Result<Wait> {
        let until = Instant::now() + timeout;
        loop {
            let mut fds = [
                libc::pollfd {
                    fd: self.file.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.wake.0.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            let left = until.saturating_duration_since(Instant::now());
            // Round up, so a wait of less than a millisecond still waits.
            let ms = left.as_micros().div_ceil(1000).min(i32::MAX as u128) as libc::c_int;
            // SAFETY: two valid pollfd entries that live through the call.
            let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, ms) };
            if n < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            }
            if fds[0].revents & libc::POLLIN != 0 {
                match self.file.read(&mut self.report) {
                    // Unplugged: the node goes away.
                    Ok(0) => return Err(io::ErrorKind::NotConnected.into()),
                    Ok(n) => {
                        self.len = n;
                        return Ok(Wait::Report(n));
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(e) => return Err(e),
                }
            }
            if fds[0].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err(io::ErrorKind::NotConnected.into());
            }
            if fds[1].revents & libc::POLLIN != 0 {
                self.wake.drain();
                return Ok(Wait::Woken);
            }
            return Ok(Wait::Timeout);
        }
    }

    fn report(&self) -> &[u8] {
        &self.report[..self.len]
    }

    fn waker(&self) -> Arc<dyn Wake> {
        self.wake.clone()
    }

    /// Hide the controller by grabbing its input nodes; games that open its hidraw node
    /// themselves still see it.
    fn set_exclusive(&mut self, on: bool) -> io::Result<()> {
        // Closing a node lets go of it.
        self.grabbed.clear();
        if !on {
            return Ok(());
        }
        for node in &self.event_nodes {
            let file = File::open(node)?;
            set_grab(file.as_raw_fd(), true)?;
            self.grabbed.push(file);
        }
        Ok(())
    }
}

impl ReportIo for Hidraw {
    fn write(&mut self, report: &[u8]) -> io::Result<()> {
        let mut out = report.to_vec();
        if out.len() < OUTPUT_LEN {
            out.resize(OUTPUT_LEN, 0);
        }
        // One write is one report.
        let n = self.file.write(&out)?;
        if n < out.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "the output report went out short",
            ));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uevents() {
        // A Pro Controller 2 as the Steam Deck's kernel lists it.
        let pro = "DRIVER=hid-generic\nHID_ID=0003:0000057E:00002069\n\
                   HID_NAME=Nintendo Switch 2 Pro Controller\n\
                   HID_PHYS=usb-0000:04:00.3-1.3/input0\nHID_UNIQ=\n\
                   MODALIAS=hid:b0003g0001v0000057Ep00002069\n";
        assert_eq!(
            parse_uevent(pro),
            Some(Uevent {
                bus: BUS_USB,
                vid: 0x057E,
                pid: 0x2069,
                phys: "usb-0000:04:00.3-1.3/input0".into(),
            })
        );
        // The touchscreen on I2C.
        let i2c = "HID_ID=0018:00002808:00001015\nHID_PHYS=i2c-FTS3528:00\n";
        assert_eq!(parse_uevent(i2c).unwrap().bus, 0x18);
        assert_eq!(parse_uevent("HID_NAME=no ids\n"), None);
        assert_eq!(parse_uevent("HID_ID=0003:0000057E\n"), None, "cut short");
        assert_eq!(
            parse_uevent("HID_ID=0003:0001057E:00002069\n"),
            None,
            "no u16"
        );
    }

    #[test]
    fn wake_ups_end_a_wait() {
        let fd = EventFd::new().unwrap();
        fd.wake();
        let mut p = libc::pollfd {
            fd: fd.0.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd.
        assert_eq!(unsafe { libc::poll(&raw mut p, 1, 0) }, 1);
        fd.drain();
        // SAFETY: as above.
        assert_eq!(unsafe { libc::poll(&raw mut p, 1, 0) }, 0, "drained");
    }
}
