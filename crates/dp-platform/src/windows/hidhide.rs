//! Device hiding through the open-source HidHide filter driver
//! (<https://github.com/nefarius/HidHide>).
//!
//! The driver keeps a *blacklist* of device instance IDs that are hidden from every
//! process except those on the *whitelist* of executable images. We add our own
//! executable to the whitelist (so we can still read the pads), add the selected
//! devices to the blacklist, and switch hiding on. Everything we change is tracked
//! and reverted in [`DeviceHider::restore`], leaving the user's own entries intact.
//!
//! The driver keeps its lists across restarts, so a process that dies without restoring
//! (killed, crashed, logged off) would leave the pads hidden from every program, a later
//! Doppelpad included. Before changing anything we write what we are about to add to a
//! journal file, and [`recover_stale`] undoes the journals of processes that are gone.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, FILETIME, GENERIC_READ, HANDLE, STILL_ACTIVE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    QueryDosDeviceW,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, GetProcessTimes, OpenProcess,
    PROCESS_QUERY_LIMITED_INFORMATION,
};

use super::{OwnedHandle, devnode, from_wide, wide};
use crate::{DeviceHider, DeviceInfo, PlatformError, Result};

const fn ctl_code(function: u32) -> u32 {
    const DEVICE_TYPE: u32 = 32769;
    const METHOD_BUFFERED: u32 = 0;
    const FILE_READ_DATA: u32 = 1;
    (DEVICE_TYPE << 16) | (FILE_READ_DATA << 14) | (function << 2) | METHOD_BUFFERED
}

const IOCTL_GET_WHITELIST: u32 = ctl_code(2048);
const IOCTL_SET_WHITELIST: u32 = ctl_code(2049);
const IOCTL_GET_BLACKLIST: u32 = ctl_code(2050);
const IOCTL_SET_BLACKLIST: u32 = ctl_code(2051);
const IOCTL_GET_ACTIVE: u32 = ctl_code(2052);
const IOCTL_SET_ACTIVE: u32 = ctl_code(2053);
/// Whether the application list is inverted (`HidHideCLI --inv-state`).
const IOCTL_GET_WLINVERSE: u32 = ctl_code(2054);

#[derive(Default)]
pub struct HidHide {
    added_devices: Vec<String>,
    added_app: Option<String>,
    previous_active: Option<bool>,
}

fn open() -> Result<OwnedHandle> {
    let name = wide(r"\\.\HidHide");
    // SAFETY: standard CreateFileW call.
    OwnedHandle::from_invalid_ret(unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            null_mut(),
            OPEN_EXISTING,
            0,
            null_mut(),
        )
    })
    .map_err(|e| {
        PlatformError::DriverMissing(
            "HidHide",
            format!("{e} (install it from {})", crate::HIDHIDE_RELEASES),
        )
    })
}

fn ioctl(h: &OwnedHandle, code: u32, input: &[u8], output: &mut [u8]) -> Result<usize> {
    let mut returned = 0u32;
    // SAFETY: input/output buffers are valid for their lengths.
    let ok = unsafe {
        DeviceIoControl(
            h.raw(),
            code,
            if input.is_empty() {
                std::ptr::null()
            } else {
                input.as_ptr().cast()
            },
            input.len() as u32,
            if output.is_empty() {
                null_mut()
            } else {
                output.as_mut_ptr().cast()
            },
            output.len() as u32,
            &mut returned,
            null_mut(),
        )
    };
    if ok == 0 {
        Err(PlatformError::last_os("HidHide DeviceIoControl"))
    } else {
        Ok(returned as usize)
    }
}

fn get_list(h: &OwnedHandle, code: u32) -> Result<Vec<String>> {
    // First call reports the required size; read with exactly that. A generous fixed
    // buffer does not work: the driver fails a non-empty list read into 64 KiB with
    // ERROR_INVALID_PARAMETER.
    let needed = ioctl(h, code, &[], &mut []).unwrap_or(16 * 1024).max(2);
    let mut buf = vec![0u8; needed];
    let n = ioctl(h, code, &[], &mut buf)?;
    let words: Vec<u16> = buf[..n]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&c| u16::from_le_bytes(c))
        .collect();
    Ok(words
        .split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect())
}

/// Encode as MULTI_SZ: each string NUL-terminated, plus a final NUL.
fn multi_sz(list: &[String]) -> Vec<u8> {
    let mut words: Vec<u16> = Vec::new();
    for s in list {
        words.extend(s.encode_utf16());
        words.push(0);
    }
    words.push(0);
    if list.is_empty() {
        words.push(0); // an empty MULTI_SZ is two NULs
    }
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

fn set_list(h: &OwnedHandle, code: u32, list: &[String]) -> Result<()> {
    ioctl(h, code, &multi_sz(list), &mut []).map(drop)
}

fn get_active(h: &OwnedHandle) -> Result<bool> {
    let mut b = [0u8; 1];
    ioctl(h, IOCTL_GET_ACTIVE, &[], &mut b)?;
    Ok(b[0] != 0)
}

fn set_active(h: &OwnedHandle, active: bool) -> Result<()> {
    ioctl(h, IOCTL_SET_ACTIVE, &[u8::from(active)], &mut []).map(drop)
}

/// In inverse mode the application list names the programs that may *not* see hidden
/// devices. Older drivers do not know the request and have no inverse mode.
fn get_inverse(h: &OwnedHandle) -> bool {
    let mut b = [0u8; 1];
    ioctl(h, IOCTL_GET_WLINVERSE, &[], &mut b).is_ok_and(|_| b[0] != 0)
}

/// Take `devices` off the blacklist and `app` off the whitelist, then set the
/// cloaking switch back to `active`.
fn undo(
    h: &OwnedHandle,
    devices: &[String],
    app: Option<&str>,
    active: Option<bool>,
) -> Result<()> {
    if !devices.is_empty() {
        let mut blacklist = get_list(h, IOCTL_GET_BLACKLIST)?;
        blacklist.retain(|b| !devices.iter().any(|d| d.eq_ignore_ascii_case(b)));
        set_list(h, IOCTL_SET_BLACKLIST, &blacklist)?;
    }
    if let Some(app) = app {
        let mut whitelist = get_list(h, IOCTL_GET_WHITELIST)?;
        whitelist.retain(|w| !w.eq_ignore_ascii_case(app));
        set_list(h, IOCTL_SET_WHITELIST, &whitelist)?;
    }
    if let Some(active) = active {
        set_active(h, active)?;
    }
    Ok(())
}

/// `C:\dir\app.exe` -> `\Device\HarddiskVolume3\dir\app.exe` (the form HidHide compares against).
fn to_device_path(path: &std::path::Path) -> Option<String> {
    let s = path.to_str()?;
    let s = s.strip_prefix(r"\\?\").unwrap_or(s);
    let (drive, rest) = (s.get(..2)?, s.get(2..)?);
    let mut buf = [0u16; 512];
    // SAFETY: buffer length given in characters.
    let n = unsafe { QueryDosDeviceW(wide(drive).as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
    (n != 0).then(|| format!("{}{}", from_wide(&buf), rest))
}

/// Our own executable as it appears on the whitelist.
fn own_image() -> Result<String> {
    let exe = std::env::current_exe().map_err(|e| PlatformError::Other(e.to_string()))?;
    to_device_path(&exe).ok_or_else(|| PlatformError::Other("cannot resolve own image path".into()))
}

/// Instance IDs of connected devices that HidHide hides from this process: cloaking is
/// on, they are on the blacklist and our executable is not on the whitelist. That is how
/// a pad goes missing from Doppelpad: hidden for another tool, or for an older Doppelpad
/// at a different path. Devices we hide ourselves never show up here, because hiding
/// whitelists us first. Inverse mode is left out: there a listed program is the one that
/// cannot see hidden devices, and Doppelpad never sets that up.
fn hidden_from_us() -> Result<Vec<String>> {
    let h = open()?;
    if !get_active(&h)? || get_inverse(&h) {
        return Ok(Vec::new());
    }
    let image = own_image()?;
    if get_list(&h, IOCTL_GET_WHITELIST)?
        .iter()
        .any(|w| w.eq_ignore_ascii_case(&image))
    {
        return Ok(Vec::new());
    }
    Ok(get_list(&h, IOCTL_GET_BLACKLIST)?
        .into_iter()
        .filter(|id| devnode::present_devnode(id).is_some())
        .collect())
}

fn hidden_warning(device: &str) -> String {
    format!(
        "HidHide hides {device} from Doppelpad — un-hide it in the HidHide Configuration \
         Client or add Doppelpad to its applications"
    )
}

/// `%LOCALAPPDATA%\doppelpad\hidhide-restore`, holding one `<pid>.txt` per process that
/// is hiding devices. Local rather than roaming: the driver's lists belong to this machine.
fn journal_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|base| {
        PathBuf::from(base)
            .join("doppelpad")
            .join("hidhide-restore")
    })
}

/// Creation time of a process (100 ns ticks), which tells a recycled PID apart.
fn creation_time(process: HANDLE) -> Option<u64> {
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: valid process handle and out-pointers.
    let ok = unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) };
    (ok != 0).then(|| u64::from(created.dwHighDateTime) << 32 | u64::from(created.dwLowDateTime))
}

/// Whether the process that wrote a journal still runs. When that cannot be told
/// (e.g. another account's process), assume it does and leave its changes alone.
fn still_running(pid: u32, started: u64) -> bool {
    // SAFETY: plain query; the handle is owned and closed on return.
    let process = match OwnedHandle::from_null_ret(unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid)
    }) {
        Ok(p) => p,
        Err(e) => return e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32),
    };
    let mut code = 0u32;
    // SAFETY: valid process handle and out-pointer.
    if unsafe { GetExitCodeProcess(process.raw(), &mut code) } == 0 {
        return true;
    }
    code == STILL_ACTIVE as u32
        && creation_time(process.raw()).is_none_or(|t| started == 0 || t == started)
}

/// What one process added to the driver's lists, kept on disk until it is undone.
#[derive(Debug, Default, PartialEq)]
struct Journal {
    pid: u32,
    started: u64,
    devices: Vec<String>,
    app: Option<String>,
    previous_active: Option<bool>,
}

impl Journal {
    fn encode(&self) -> String {
        let mut s = format!("pid={}\nstarted={}\n", self.pid, self.started);
        if let Some(active) = self.previous_active {
            let _ = writeln!(s, "active={}", u8::from(active));
        }
        if let Some(app) = &self.app {
            let _ = writeln!(s, "app={app}");
        }
        for d in &self.devices {
            let _ = writeln!(s, "device={d}");
        }
        s
    }

    fn decode(text: &str) -> Option<Journal> {
        let mut j = Journal::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key {
                "pid" => j.pid = value.parse().ok()?,
                "started" => j.started = value.parse().ok()?,
                "active" => j.previous_active = Some(value == "1"),
                "app" => j.app = Some(value.to_owned()),
                "device" => j.devices.push(value.to_owned()),
                _ => {}
            }
        }
        (j.pid != 0).then_some(j)
    }

    /// Written to a temporary file and renamed, so a reader never sees half a journal.
    fn save(&self) -> std::io::Result<()> {
        let dir = journal_dir().ok_or_else(|| std::io::Error::other("LOCALAPPDATA is not set"))?;
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join(format!("{}.tmp", self.pid));
        std::fs::write(&tmp, self.encode())?;
        std::fs::rename(&tmp, dir.join(format!("{}.txt", self.pid)))
    }
}

/// Undo what earlier Doppelpad processes hid and never restored because they ended
/// without cleaning up. Call before enumerating devices, or the hidden pads stay invisible.
pub fn recover_stale() {
    let Some(entries) = journal_dir().and_then(|dir| std::fs::read_dir(dir).ok()) else {
        return;
    };
    let mut driver = None;
    for path in entries.flatten().map(|e| e.path()) {
        if path.extension().is_none_or(|ext| ext != "txt") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(journal) = Journal::decode(&text) else {
            let _ = std::fs::remove_file(&path);
            continue;
        };
        if still_running(journal.pid, journal.started) {
            continue;
        }
        if driver.is_none() {
            match open() {
                Ok(h) => driver = Some(h),
                // Without the driver there is nothing to undo now; try again next start.
                Err(_) => return,
            }
        }
        let h = driver.as_ref().expect("opened above");
        match undo(
            h,
            &journal.devices,
            journal.app.as_deref(),
            journal.previous_active,
        ) {
            Ok(()) => {
                let _ = std::fs::remove_file(&path);
                tracing::info!(
                    pid = journal.pid,
                    devices = ?journal.devices,
                    "undid device hiding left behind by an earlier run"
                );
            }
            Err(e) => tracing::warn!(
                pid = journal.pid,
                "cannot undo device hiding left behind by an earlier run: {e}"
            ),
        }
    }
}

impl HidHide {
    pub fn new() -> Self {
        Self::default()
    }

    fn write_journal(&self) {
        let journal = Journal {
            pid: std::process::id(),
            // SAFETY: the pseudo handle of the current process needs no closing.
            started: creation_time(unsafe { GetCurrentProcess() }).unwrap_or(0),
            devices: self.added_devices.clone(),
            app: self.added_app.clone(),
            previous_active: self.previous_active,
        };
        if let Err(e) = journal.save() {
            tracing::warn!("cannot record the HidHide changes for crash recovery: {e}");
        }
    }

    fn remove_journal() {
        if let Some(dir) = journal_dir() {
            let _ = std::fs::remove_file(dir.join(format!("{}.txt", std::process::id())));
        }
    }
}

impl DeviceHider for HidHide {
    fn available(&self) -> Result<()> {
        let h = open()?;
        get_active(&h).map(drop)
    }

    /// XInput cannot tell which device a pad is, so hiding one hides every physical XUSB
    /// device.
    fn expand(&self, device: &DeviceInfo, out: &mut Vec<String>) {
        out.extend(device.instance_paths.iter().cloned());
        if device.backend == "xinput" {
            out.extend(super::physical_xusb_instances());
        }
    }

    fn hide(&mut self, instance_paths: &[String]) -> Result<()> {
        self.restore()?;
        if instance_paths.is_empty() {
            return Ok(());
        }
        let h = open()?;

        let image = own_image()?;
        let mut whitelist = get_list(&h, IOCTL_GET_WHITELIST)?;
        if !whitelist.iter().any(|w| w.eq_ignore_ascii_case(&image)) {
            whitelist.push(image.clone());
            self.added_app = Some(image);
        }

        let mut blacklist = get_list(&h, IOCTL_GET_BLACKLIST)?;
        for p in instance_paths {
            if !blacklist.iter().any(|b| b.eq_ignore_ascii_case(p)) {
                blacklist.push(p.clone());
                self.added_devices.push(p.clone());
            }
        }
        self.previous_active = Some(get_active(&h)?);

        // Journal first: if this process dies after the changes below, the next start
        // can still undo them.
        self.write_journal();
        if self.added_app.is_some() {
            set_list(&h, IOCTL_SET_WHITELIST, &whitelist)?;
        }
        set_list(&h, IOCTL_SET_BLACKLIST, &blacklist)?;
        set_active(&h, true)?;
        tracing::info!(devices = ?instance_paths, "devices hidden via HidHide");
        Ok(())
    }

    fn restore(&mut self) -> Result<()> {
        if self.added_devices.is_empty()
            && self.added_app.is_none()
            && self.previous_active.is_none()
        {
            return Ok(());
        }
        let h = open()?;
        undo(
            &h,
            &self.added_devices,
            self.added_app.as_deref(),
            self.previous_active,
        )?;
        self.added_devices.clear();
        self.added_app = None;
        self.previous_active = None;
        Self::remove_journal();
        tracing::info!("HidHide state restored");
        Ok(())
    }

    fn problems(&self) -> Vec<String> {
        let ids = match hidden_from_us() {
            Ok(ids) => ids,
            // Without a reachable driver nothing is hidden; `available` says why.
            Err(e) => {
                tracing::debug!("cannot check HidHide for hidden controllers: {e}");
                return Vec::new();
            }
        };
        let mut out: Vec<String> = ids
            .iter()
            .map(|id| {
                let name = devnode::present_devnode(id).and_then(devnode::product_name);
                hidden_warning(name.as_deref().unwrap_or(id))
            })
            .collect();
        // One pad is often listed twice (its USB and HID nodes).
        out.sort();
        out.dedup();
        out
    }
}

impl Drop for HidHide {
    fn drop(&mut self) {
        if let Err(e) = self.restore() {
            tracing::warn!("could not restore HidHide state: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_codes() {
        // CTL_CODE(32769, 2048, METHOD_BUFFERED, FILE_READ_DATA)
        assert_eq!(IOCTL_GET_WHITELIST, 0x8001_6000);
        assert_eq!(IOCTL_SET_ACTIVE, 0x8001_6014);
        assert_eq!(IOCTL_GET_WLINVERSE, 0x8001_6018);
    }

    #[test]
    fn hidden_warning_names_device_and_fix() {
        let w = hidden_warning("DualSense Wireless Controller");
        assert!(w.starts_with("HidHide hides DualSense Wireless Controller from Doppelpad — "));
        assert!(w.ends_with("HidHide Configuration Client or add Doppelpad to its applications"));
    }

    #[test]
    fn multi_sz_encoding() {
        let b = multi_sz(&["AB".into(), "C".into()]);
        assert_eq!(b, vec![b'A', 0, b'B', 0, 0, 0, b'C', 0, 0, 0, 0, 0]);
        assert_eq!(multi_sz(&[]), vec![0, 0, 0, 0]);
    }

    #[test]
    fn journal_round_trip() {
        let j = Journal {
            pid: 4242,
            started: 133_000_000_000_000_000,
            devices: vec![
                r"HID\VID_054C&PID_0CE6&MI_03\7&299DE27F&0&0000".into(),
                r"USB\VID_045E&PID_028E\01".into(),
            ],
            app: Some(r"\Device\HarddiskVolume3\Program Files\Doppelpad\doppelpad-gui.exe".into()),
            previous_active: Some(false),
        };
        assert_eq!(Journal::decode(&j.encode()), Some(j));

        let bare = Journal {
            pid: 7,
            ..Journal::default()
        };
        assert_eq!(Journal::decode(&bare.encode()), Some(bare));
    }

    #[test]
    fn journal_rejects_garbage() {
        assert_eq!(Journal::decode(""), None);
        assert_eq!(Journal::decode("device=HID\\X\n"), None);
        assert_eq!(Journal::decode("pid=abc\n"), None);
        let j = Journal::decode("\npid=9\nfuture=1\n").unwrap();
        assert_eq!(j.pid, 9);
    }

    #[test]
    fn own_process_counts_as_running() {
        // SAFETY: pseudo handle of the current process.
        let started = creation_time(unsafe { GetCurrentProcess() }).unwrap();
        assert!(still_running(std::process::id(), started));
        // Same PID, different start: a recycled PID, so the writer is gone.
        assert!(!still_running(std::process::id(), started + 1));
    }
}
