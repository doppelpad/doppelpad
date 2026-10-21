//! Detect a running reWASD and stop it when the user asks. reWASD's service and driver
//! keep controllers to themselves, so Doppelpad cannot read them while reWASD runs.
//!
//! Only the Windows service manager and plain process APIs are used; Doppelpad never
//! talks to reWASD. The `jlserv` service's image path tells where reWASD is installed,
//! and every running program started from that folder belongs to it. Stopping a service
//! needs administrator rights, so unless Doppelpad has them, an elevated `sc.exe` stops
//! it and Windows asks the user first.

use std::io;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use serde::Serialize;
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_CANCELLED, ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SERVICE_NOT_ACTIVE,
    HWND, LPARAM, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
};
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, ControlService, OpenSCManagerW, OpenServiceW, QUERY_SERVICE_CONFIGW,
    QueryServiceConfigW, QueryServiceStatusEx, SC_HANDLE, SC_MANAGER_CONNECT,
    SC_STATUS_PROCESS_INFO, SERVICE_CONTROL_STOP, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS,
    SERVICE_STATUS, SERVICE_STATUS_PROCESS, SERVICE_STOP, SERVICE_STOPPED,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, GetExitCodeProcess, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, PostMessageW, SW_HIDE, WM_CLOSE,
};
use windows_sys::core::BOOL;

use super::{OwnedHandle, process, wide};

const SERVICE: &str = "jlserv";
/// How long the service gets to stop.
const SERVICE_TIMEOUT: Duration = Duration::from_secs(20);
/// How long the programs get to close on their own before they are terminated.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

/// What of reWASD is running.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Rewasd {
    /// Where reWASD is installed.
    pub dir: PathBuf,
    /// The service's display name, e.g. "Mapping Service".
    pub service_name: String,
    /// The service is running (or starting or stopping).
    pub service_running: bool,
    /// reWASD programs other than the service, e.g. its tray agent and editor.
    pub processes: Vec<Process>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Process {
    pub pid: u32,
    /// Executable file name, e.g. `reWASDUI.exe`.
    pub name: String,
}

impl Rewasd {
    pub fn running(&self) -> bool {
        self.service_running || !self.processes.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StopError {
    /// The user said no to the administrator prompt; nothing was stopped.
    #[error("administrator permission to stop the reWASD service was declined")]
    Declined,
    #[error("{0}")]
    Failed(String),
}

/// reWASD's state, or `None` when it is not installed.
pub fn detect() -> Option<Rewasd> {
    let svc = open_service(SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG)
        .map_err(|e| {
            if e.raw_os_error() != Some(ERROR_SERVICE_DOES_NOT_EXIST as i32) {
                tracing::debug!("cannot open the reWASD service: {e}");
            }
        })
        .ok()?;
    let (image, service_name) = config(&svc)
        .map_err(|e| tracing::debug!("cannot read the reWASD service config: {e}"))
        .ok()?;
    let dir = Path::new(exe_of(&image)).parent()?.to_path_buf();
    let status = status(&svc).ok();
    Some(Rewasd {
        processes: processes_in(&dir, status.map_or(0, |s| s.dwProcessId)),
        service_running: status.is_some_and(|s| s.dwCurrentState != SERVICE_STOPPED),
        service_name,
        dir,
    })
}

/// Stop the service, then close reWASD's programs. When the user declines the
/// administrator prompt nothing is closed. `parent` is the window the prompt belongs to
/// (0 for none).
pub fn stop(found: &Rewasd, parent: isize) -> Result<(), StopError> {
    if found.service_running {
        stop_service(parent as HWND)?;
    }
    let failed = close_processes(&found.processes);
    if !failed.is_empty() {
        return Err(StopError::Failed(format!(
            "could not close {}",
            failed.join(", ")
        )));
    }
    tracing::info!("reWASD stopped");
    Ok(())
}

struct ScHandle(SC_HANDLE);

impl Drop for ScHandle {
    fn drop(&mut self) {
        // SAFETY: we own the handle.
        unsafe { CloseServiceHandle(self.0) };
    }
}

fn open_service(access: u32) -> io::Result<ScHandle> {
    // SAFETY: plain calls; the handles are owned and closed on return.
    unsafe {
        let scm = OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err(io::Error::last_os_error());
        }
        let scm = ScHandle(scm);
        let name = wide(SERVICE);
        let svc = OpenServiceW(scm.0, name.as_ptr(), access);
        if svc.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(ScHandle(svc))
    }
}

fn status(svc: &ScHandle) -> io::Result<SERVICE_STATUS_PROCESS> {
    // SAFETY: plain data; all-zero is valid.
    let mut st: SERVICE_STATUS_PROCESS = unsafe { std::mem::zeroed() };
    let mut needed = 0u32;
    // SAFETY: the buffer is a SERVICE_STATUS_PROCESS of the size passed.
    let ok = unsafe {
        QueryServiceStatusEx(
            svc.0,
            SC_STATUS_PROCESS_INFO,
            (&raw mut st).cast(),
            size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut needed,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(st)
}

/// The service's command line and display name.
fn config(svc: &ScHandle) -> io::Result<(String, String)> {
    let mut needed = 0u32;
    // SAFETY: a size query; it fails with ERROR_INSUFFICIENT_BUFFER and sets `needed`.
    unsafe { QueryServiceConfigW(svc.0, null_mut(), 0, &mut needed) };
    // u64 elements keep the struct at the start of the buffer aligned.
    let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
    // SAFETY: the buffer holds `needed` bytes.
    let ok = unsafe { QueryServiceConfigW(svc.0, buf.as_mut_ptr().cast(), needed, &mut needed) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: filled in above; its strings point into `buf`, which outlives this block.
    unsafe {
        let cfg = &*buf.as_ptr().cast::<QUERY_SERVICE_CONFIGW>();
        Ok((pwstr(cfg.lpBinaryPathName), pwstr(cfg.lpDisplayName)))
    }
}

/// Read a NUL-terminated UTF-16 string (null reads as empty).
///
/// # Safety
/// `p` is null or points to a NUL-terminated string.
unsafe fn pwstr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0;
    // SAFETY: the caller guarantees a terminating NUL.
    unsafe {
        while *p.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
    }
}

/// The executable of a service command line: `"C:\dir\a.exe" -x` -> `C:\dir\a.exe`.
fn exe_of(cmdline: &str) -> &str {
    let s = cmdline.trim();
    if let Some(rest) = s.strip_prefix('"') {
        return rest.split('"').next().unwrap_or(rest);
    }
    match s.to_ascii_lowercase().find(".exe") {
        Some(i) => &s[..i + 4],
        None => s,
    }
}

/// Whether `path` lies inside `dir` (Windows paths, any case).
fn in_dir(path: &str, dir: &str) -> bool {
    let dir = dir.trim_end_matches('\\').to_lowercase() + "\\";
    path.to_lowercase().starts_with(&dir)
}

/// Running programs started from `dir`, apart from `skip` (the service) and ourselves.
fn processes_in(dir: &Path, skip: u32) -> Vec<Process> {
    let dir = dir.to_string_lossy();
    // SAFETY: returns our own process ID.
    let own = unsafe { GetCurrentProcessId() };
    process::list()
        .into_iter()
        .filter(|&(pid, _)| {
            pid != 0
                && pid != skip
                && pid != own
                && image_path(pid).is_some_and(|p| in_dir(&p, &dir))
        })
        .map(|(pid, name)| Process { pid, name })
        .collect()
}

fn image_path(pid: u32) -> Option<String> {
    // SAFETY: plain query; the handle is owned and closed on return.
    let process = OwnedHandle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) });
    if !process.valid() {
        return None;
    }
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: valid handle; `len` is the buffer size in characters.
    let ok = unsafe {
        QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
    };
    (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
}

/// Stop the service directly when Doppelpad may, else through an elevated `sc.exe`;
/// then wait until it has stopped.
fn stop_service(parent: HWND) -> Result<(), StopError> {
    let failed = |e: io::Error| StopError::Failed(format!("cannot stop the reWASD service: {e}"));
    match open_service(SERVICE_STOP | SERVICE_QUERY_STATUS) {
        Ok(svc) => {
            // SAFETY: plain data; all-zero is valid.
            let mut st: SERVICE_STATUS = unsafe { std::mem::zeroed() };
            // SAFETY: valid service handle and out-pointer.
            if unsafe { ControlService(svc.0, SERVICE_CONTROL_STOP, &mut st) } == 0 {
                let e = io::Error::last_os_error();
                if e.raw_os_error() != Some(ERROR_SERVICE_NOT_ACTIVE as i32) {
                    return Err(failed(e));
                }
            }
        }
        Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {
            stop_service_elevated(parent)?;
        }
        Err(e) => return Err(failed(e)),
    }
    let svc = open_service(SERVICE_QUERY_STATUS).map_err(failed)?;
    let deadline = Instant::now() + SERVICE_TIMEOUT;
    loop {
        if status(&svc).map_err(failed)?.dwCurrentState == SERVICE_STOPPED {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(StopError::Failed(format!(
                "the reWASD service did not stop within {} s",
                SERVICE_TIMEOUT.as_secs()
            )));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Run `sc.exe stop jlserv` as administrator; Windows asks the user first.
fn stop_service_elevated(parent: HWND) -> Result<(), StopError> {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let sc = wide(&Path::new(&root).join(r"System32\sc.exe").to_string_lossy());
    let verb = wide("runas");
    let params = wide(&format!("stop {SERVICE}"));
    // SAFETY: plain data; all-zero is the documented starting point.
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    info.hwnd = parent;
    info.lpVerb = verb.as_ptr();
    info.lpFile = sc.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_HIDE;
    // The shell may hand the request to COM components, so COM must be initialised.
    // SAFETY: balanced with CoUninitialize below when it succeeded.
    let com = unsafe {
        CoInitializeEx(
            null(),
            (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
        )
    } >= 0;
    // SAFETY: `info` and the strings it points to live until the call returns.
    let ok = unsafe { ShellExecuteExW(&mut info) } != 0;
    let err = io::Error::last_os_error();
    if com {
        // SAFETY: balances the successful CoInitializeEx above.
        unsafe { CoUninitialize() };
    }
    if !ok {
        return Err(if err.raw_os_error() == Some(ERROR_CANCELLED as i32) {
            StopError::Declined
        } else {
            StopError::Failed(format!("cannot run sc.exe as administrator: {err}"))
        });
    }
    if info.hProcess.is_null() {
        return Ok(()); // the caller still waits for the service to stop
    }
    let process = OwnedHandle(info.hProcess);
    let mut code = 0u32;
    // SAFETY: valid process handle and out-pointer.
    unsafe {
        WaitForSingleObject(process.0, SERVICE_TIMEOUT.as_millis() as u32);
        GetExitCodeProcess(process.0, &mut code);
    }
    match code {
        0 => Ok(()),
        c if c == ERROR_SERVICE_NOT_ACTIVE => Ok(()),
        c => Err(StopError::Failed(format!(
            "sc.exe could not stop the reWASD service: {}",
            io::Error::from_raw_os_error(c as i32)
        ))),
    }
}

/// Ask the programs to close (as their window's close button would), terminate those
/// still running after [`CLOSE_TIMEOUT`], and return the names of any that survived.
fn close_processes(processes: &[Process]) -> Vec<String> {
    let mut failed = Vec::new();
    let mut open = Vec::new();
    for p in processes {
        // SAFETY: plain call; the handle is owned and closed on drop.
        let h =
            OwnedHandle(unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, p.pid) });
        if h.valid() {
            open.push((p, h));
        } else {
            tracing::warn!(name = %p.name, "cannot close reWASD program: {}", io::Error::last_os_error());
            failed.push(p.name.clone());
        }
    }
    if open.is_empty() {
        return failed;
    }
    let pids: Vec<u32> = open.iter().map(|(p, _)| p.pid).collect();
    // SAFETY: the callback only reads `pids`, which outlives the call.
    unsafe { EnumWindows(Some(post_close), (&raw const pids) as LPARAM) };
    let deadline = Instant::now() + CLOSE_TIMEOUT;
    for (p, h) in &open {
        let left = deadline.saturating_duration_since(Instant::now());
        // SAFETY: valid process handles.
        let exited = unsafe { WaitForSingleObject(h.0, left.as_millis() as u32) } == WAIT_OBJECT_0;
        if exited {
            continue;
        }
        // SAFETY: as above.
        let killed = unsafe { TerminateProcess(h.0, 1) } != 0
            && unsafe { WaitForSingleObject(h.0, 2000) } == WAIT_OBJECT_0;
        if !killed {
            tracing::warn!(name = %p.name, "cannot terminate reWASD program");
            failed.push(p.name.clone());
        }
    }
    failed
}

/// `EnumWindows` callback: post `WM_CLOSE` to top-level windows of the given processes.
unsafe extern "system" fn post_close(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `&Vec<u32>` passed by `close_processes`.
    let pids = unsafe { &*(lparam as *const Vec<u32>) };
    let mut pid = 0u32;
    // SAFETY: valid window handle from EnumWindows and out-pointer.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pids.contains(&pid) {
        // SAFETY: posting a message has no memory-safety requirements.
        unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_executable() {
        assert_eq!(
            exe_of(r#""C:\Program Files\reWASD\jlserv.exe""#),
            r"C:\Program Files\reWASD\jlserv.exe"
        );
        assert_eq!(
            exe_of(r#""C:\Program Files\reWASD\jlserv.exe" -service"#),
            r"C:\Program Files\reWASD\jlserv.exe"
        );
        assert_eq!(
            exe_of(r"C:\reWASD\JLSERV.EXE /run"),
            r"C:\reWASD\JLSERV.EXE"
        );
        assert_eq!(exe_of(r"C:\reWASD\jlserv"), r"C:\reWASD\jlserv");
    }

    #[test]
    fn inside_install_dir() {
        let dir = r"C:\Program Files\reWASD";
        assert!(in_dir(r"C:\Program Files\reWASD\Engine.exe", dir));
        assert!(in_dir(r"c:\program files\rewasd\reWASDUI.exe", dir));
        assert!(in_dir(
            r"C:\Program Files\reWASD\avrdude\avrdude.exe",
            r"C:\Program Files\reWASD\"
        ));
        assert!(!in_dir(r"C:\Program Files\reWASD Tools\Engine.exe", dir));
        assert!(!in_dir(r"C:\Games\Engine.exe", dir));
    }
}
