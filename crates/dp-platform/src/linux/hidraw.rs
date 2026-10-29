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

use super::codes::BUS_USB;
use super::dev::{poll_until, pollfd, set_grab};
use super::sysfs::parse_uevent;
use crate::hidhost::{HidHandle, Wait, Wake, read_via_wait};
use crate::nintendo::{self, Model, Switch2Model};
use crate::padio::ReportIo;

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
                pollfd(self.file.as_raw_fd(), libc::POLLIN),
                pollfd(self.wake.0.as_raw_fd(), libc::POLLIN),
            ];
            poll_until(&mut fds, until)?;
            if fds[0].revents & libc::POLLIN != 0 {
                match self.file.read(&mut self.report) {
                    // Unplugged: the node goes away.
                    Ok(0) => return Err(io::ErrorKind::NotConnected.into()),
                    Ok(n) => {
                        self.len = n;
                        return Ok(Wait::Report);
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
        read_via_wait(self, buf, timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_ups_end_a_wait() {
        let fd = EventFd::new().unwrap();
        fd.wake();
        let ready = || {
            poll_until(
                &mut [pollfd(fd.0.as_raw_fd(), libc::POLLIN)],
                Instant::now(),
            )
        };
        assert_eq!(ready().unwrap(), 1);
        fd.drain();
        assert_eq!(ready().unwrap(), 0, "drained");
    }
}
