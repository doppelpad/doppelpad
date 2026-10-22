//! Shared helpers for evdev device nodes and uinput.

use std::collections::HashMap;
use std::ffi::CString;
use std::io;
use std::os::fd::RawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use evdev::{AbsInfo, AbsoluteAxisCode, Device, KeyCode, RelativeAxisCode};

use super::padmap::AbsRange;
use crate::{PlatformError, Result};

/// `phys` prefix of every uinput device Doppelpad creates.
pub const OWN_PHYS: &str = "doppelpad/";

pub fn own_phys(role: &str) -> CString {
    CString::new(format!("{OWN_PHYS}{role}")).expect("no NUL in phys")
}

/// Whether the device is one of our own uinput devices (virtual pad, output, passthrough).
pub fn is_own(dev: &Device) -> bool {
    dev.physical_path().is_some_and(|p| p.starts_with(OWN_PHYS))
}

/// `/dev/input/event*` nodes, in numeric order.
pub fn event_nodes() -> Vec<PathBuf> {
    let Ok(dir) = std::fs::read_dir("/dev/input") else {
        return Vec::new();
    };
    let mut nodes: Vec<(u32, PathBuf)> = dir
        .flatten()
        .filter_map(|e| {
            let n = e
                .file_name()
                .to_str()?
                .strip_prefix("event")?
                .parse()
                .ok()?;
            Some((n, e.path()))
        })
        .collect();
    nodes.sort();
    nodes.into_iter().map(|(_, p)| p).collect()
}

/// How long a node that could not be opened is left alone.
const RETRY_FAILED: Duration = Duration::from_secs(30);

/// The nodes a scanner leaves alone: ones it does not want, until they disappear, and ones it
/// could not open, for [`RETRY_FAILED`].
#[derive(Default)]
pub struct NodeTracker {
    /// `true` marks nodes that only wait for something else (see [`NodeTracker::wait`]).
    ignored: HashMap<PathBuf, bool>,
    /// With the time of the failure.
    failed: HashMap<PathBuf, Instant>,
}

impl NodeTracker {
    /// Forget the nodes that are gone and the failures old enough to retry.
    pub fn prune(&mut self, present: &[PathBuf], now: Instant) {
        self.ignored.retain(|p, _| present.contains(p));
        self.failed
            .retain(|p, t| present.contains(p) && now - *t < RETRY_FAILED);
    }

    /// Whether the scanner leaves `path` alone.
    pub fn skips(&self, path: &Path) -> bool {
        self.ignored.contains_key(path) || self.failed.contains_key(path)
    }

    /// Leave `path` alone until it disappears.
    pub fn ignore(&mut self, path: PathBuf) {
        self.ignored.insert(path, false);
    }

    /// Leave `path` alone until [`NodeTracker::retry_waiting`].
    pub fn wait(&mut self, path: PathBuf) {
        self.ignored.insert(path, true);
    }

    /// Look at the nodes left waiting again.
    pub fn retry_waiting(&mut self) {
        self.ignored.retain(|_, waiting| !*waiting);
    }

    /// `path` could not be opened at `now`.
    pub fn failed(&mut self, path: PathBuf, now: Instant) {
        self.failed.insert(path, now);
    }

    /// Retry every node that could not be opened.
    pub fn retry_failed(&mut self) {
        self.failed.clear();
    }
}

/// Whether `dev` reports key or button `code`.
pub fn has_key(dev: &Device, code: u16) -> bool {
    dev.supported_keys()
        .is_some_and(|k| k.contains(KeyCode(code)))
}

/// Whether `dev` reports absolute axis `code`.
pub fn has_abs(dev: &Device, code: u16) -> bool {
    dev.supported_absolute_axes()
        .is_some_and(|a| a.contains(AbsoluteAxisCode(code)))
}

/// Whether `dev` reports relative axis `code`.
pub fn has_rel(dev: &Device, code: u16) -> bool {
    dev.supported_relative_axes()
        .is_some_and(|r| r.contains(RelativeAxisCode(code)))
}

/// How long a reader thread waits for events before it looks at its stop flag again.
pub const READ_TIMEOUT_MS: i32 = 100;

/// What a reader thread found on its node.
pub enum Fetch<T> {
    /// Events, as the fetch collected them.
    Ready(T),
    /// None yet: look at the stop flag and wait again.
    Idle,
    /// The node failed or is gone (unplugged).
    Closed(io::Error),
}

/// Wait up to [`READ_TIMEOUT_MS`] for `fd` to be readable, then `fetch` its events.
pub fn fetch<T>(fd: RawFd, fetch: impl FnOnce() -> io::Result<T>) -> Fetch<T> {
    match wait_readable(fd, READ_TIMEOUT_MS) {
        Ok(true) => {}
        Ok(false) => return Fetch::Idle,
        Err(e) => return Fetch::Closed(e),
    }
    match fetch() {
        Ok(events) => Fetch::Ready(events),
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Fetch::Idle,
        Err(e) => Fetch::Closed(e),
    }
}

/// Wait up to `timeout_ms` for `fd` to have data (or an error / hangup, which the next read
/// then reports).
fn wait_readable(fd: RawFd, timeout_ms: i32) -> io::Result<bool> {
    let mut p = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: a single valid pollfd; the descriptor is owned by the caller.
    let n = unsafe { libc::poll(&mut p, 1, timeout_ms) };
    if n < 0 {
        let e = io::Error::last_os_error();
        return if e.kind() == io::ErrorKind::Interrupted {
            Ok(false)
        } else {
            Err(e)
        };
    }
    Ok(n > 0)
}

const EVIOCGRAB: libc::Ioctl = libc::_IOW::<libc::c_int>(b'E' as u32, 0x90);

/// Grab (or release) an event node for exclusive use. The grab belongs to the open file, so
/// it covers every duplicate of `fd` and ends at the latest when the file is closed.
///
/// Used instead of `evdev::Device::grab`, whose `Drop` complains on stderr when the device
/// has been unplugged in the meantime.
pub fn set_grab(fd: RawFd, grab: bool) -> io::Result<()> {
    // SAFETY: EVIOCGRAB takes its int argument by value.
    if unsafe { libc::ioctl(fd, EVIOCGRAB, libc::c_int::from(grab)) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Sysfs path of the device that owns this input node (the HID device or USB interface).
/// The gamepad, motion-sensor and touchpad nodes of one controller share it.
pub fn parent_device(node: &Path) -> Option<PathBuf> {
    let name = node.file_name()?;
    std::fs::canonicalize(
        Path::new("/sys/class/input")
            .join(name)
            .join("device/device"),
    )
    .ok()
}

/// Supported absolute axes with their ranges and current values.
pub fn abs_axes(dev: &Device) -> Vec<(u16, AbsRange, i32)> {
    let Ok(axes) = dev.get_absinfo() else {
        return Vec::new();
    };
    axes.map(|(code, info): (AbsoluteAxisCode, AbsInfo)| {
        let range = AbsRange {
            min: info.minimum(),
            max: info.maximum(),
            resolution: info.resolution(),
        };
        (code.0, range, info.value())
    })
    .collect()
}

pub fn uinput_error(e: io::Error) -> PlatformError {
    match e.kind() {
        io::ErrorKind::NotFound => PlatformError::DriverMissing(
            "uinput",
            "/dev/uinput does not exist; load the module with `sudo modprobe uinput`".into(),
        ),
        io::ErrorKind::PermissionDenied => PlatformError::DriverMissing(
            "uinput",
            "no write access to /dev/uinput; install the udev rule from docs/LINUX.md".into(),
        ),
        _ => PlatformError::Os {
            context: "uinput",
            source: e,
        },
    }
}

/// Whether we may create uinput devices.
pub fn check_uinput() -> Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/uinput")
        .map(drop)
        .map_err(uinput_error)
}

/// uinput takes 80-byte names, but the evdev crate asserts `len + 1 < 80`: at most 78 bytes,
/// cut at a character boundary.
pub fn uinput_name(name: &str) -> String {
    let mut end = name.len().min(78);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].to_owned()
}
