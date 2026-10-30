//! Shared helpers for evdev device nodes and uinput.

use std::collections::HashMap;
use std::ffi::CString;
use std::io;
use std::os::fd::RawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use evdev::{AbsInfo, AbsoluteAxisCode, Device, KeyCode, RelativeAxisCode};

use super::padmap::{AbsRange, bitmap_has};
use super::sysfs::{self, LedWrite};
use crate::{Battery, PlatformError, Result};

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
    let mut p = [pollfd(fd, libc::POLLIN)];
    let until = Instant::now() + Duration::from_millis(u64::try_from(timeout_ms).unwrap_or(0));
    Ok(poll_until(&mut p, until)? > 0)
}

/// A `pollfd` waiting for `events` on `fd`.
pub fn pollfd(fd: RawFd, events: libc::c_short) -> libc::pollfd {
    libc::pollfd {
        fd,
        events,
        revents: 0,
    }
}

/// Wait until one of `fds` is ready (see their `revents`) or `until` passes: the number that
/// are, 0 once the time is up. A wait a signal interrupts goes on until `until`.
pub fn poll_until(fds: &mut [libc::pollfd], until: Instant) -> io::Result<usize> {
    loop {
        let left = until.saturating_duration_since(Instant::now());
        // Round up, so a wait of less than a millisecond still waits.
        let ms = left.as_micros().div_ceil(1000).min(i32::MAX as u128) as libc::c_int;
        // SAFETY: `fds.len()` valid pollfd entries that live through the call.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, ms) };
        if let Ok(n) = usize::try_from(n) {
            return Ok(n);
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
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

/// A file of the node's device in sysfs (`/sys/class/input/eventN/device/<attr>`). Anyone may
/// read these, also without access to the node.
fn sysfs_attr(node: &Path, attr: &str) -> Option<String> {
    let name = node.file_name()?;
    std::fs::read_to_string(
        Path::new("/sys/class/input")
            .join(name)
            .join("device")
            .join(attr),
    )
    .ok()
}

/// Whether sysfs lists key or button `code` for the node. Unlike [`has_key`], this works
/// without access to the node.
pub fn sysfs_has_key(node: &Path, code: u16) -> bool {
    sysfs_attr(node, "capabilities/key").is_some_and(|bits| bitmap_has(&bits, usize::BITS, code))
}

/// The device name sysfs gives for the node.
pub fn sysfs_name(node: &Path) -> Option<String> {
    sysfs_attr(node, "name").map(|n| n.trim().to_owned())
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

/// The battery a controller's driver reports under its device (see [`parent_device`]).
pub fn read_battery(device: &Path) -> Option<Battery> {
    let supplies = std::fs::read_dir(device.join("power_supply")).ok()?;
    supplies.flatten().find_map(|s| {
        let attr = |name: &str| std::fs::read_to_string(s.path().join(name)).ok();
        sysfs::battery(
            attr("capacity").as_deref(),
            attr("capacity_level").as_deref(),
            attr("status").as_deref(),
        )
    })
}

/// Names of the LEDs a controller's driver registers under its device.
pub fn led_names(device: &Path) -> Vec<String> {
    let Ok(leds) = std::fs::read_dir(device.join("leds")) else {
        return Vec::new();
    };
    let mut names: Vec<String> = leds
        .flatten()
        .filter_map(|l| l.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

/// Write LED attributes under a controller's device; stops at the first that fails.
pub fn write_leds(device: &Path, writes: &[LedWrite]) -> io::Result<()> {
    for (led, attr, value) in writes {
        std::fs::write(device.join("leds").join(led).join(attr), value)?;
    }
    Ok(())
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
