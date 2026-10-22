//! Power, screenshot and window commands that mappings can trigger.

use std::path::PathBuf;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HANDLE, HWND, LUID, POINT, RECT, SYSTEMTIME};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CAPTUREBLT, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
    GetMonitorInfoW, HBITMAP, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint,
    MonitorFromWindow, ReleaseDC, SRCCOPY, SelectObject,
};
use windows_sys::Win32::Graphics::GdiPlus::{
    GdipCreateBitmapFromHBITMAP, GdipDisposeImage, GdipSaveImageToFile, GdiplusShutdown,
    GdiplusStartup, GdiplusStartupInput, GpBitmap,
};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
    SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::Power::{
    ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetSuspendState, SetThreadExecutionState,
};
use windows_sys::Win32::System::Shutdown::{
    EWX_LOGOFF, EWX_POWEROFF, EWX_REBOOT, ExitWindowsEx, SHTDN_REASON_FLAG_PLANNED,
    SHTDN_REASON_MAJOR_OTHER,
};
use windows_sys::Win32::System::StationsAndDesktops::{
    CloseDesktop, DESKTOP_READOBJECTS, GetUserObjectInformationW, OpenInputDesktop, UOI_NAME,
};
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::Shell::{FOLDERID_Pictures, SHGetKnownFolderPath};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, PostMessageW, WM_CLOSE};
use windows_sys::core::GUID;

use super::{OwnedHandle, foreground, wide};
use crate::{ForegroundWatcher, PlatformError, PowerAction, Result, SystemControl};

/// The PNG encoder of GDI+.
const PNG_ENCODER: GUID = GUID::from_u128(0x557cf406_1a04_11d3_9a73_0000f81ef32e);

pub struct WinSystem;

/// Shutting down, restarting and sleeping need the shutdown privilege, which every
/// interactive user has but which is off by default in a process token.
fn enable_shutdown_privilege() -> Result<()> {
    let mut token: HANDLE = null_mut();
    // SAFETY: our own process and a valid out-pointer.
    let opened = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
    };
    if opened == 0 {
        return Err(PlatformError::last_os("OpenProcessToken"));
    }
    let token = OwnedHandle::new(token);
    let mut luid = LUID::default();
    // SAFETY: a NUL-terminated privilege name and a valid out-pointer.
    if unsafe { LookupPrivilegeValueW(null(), SE_SHUTDOWN_NAME, &mut luid) } == 0 {
        return Err(PlatformError::last_os("LookupPrivilegeValueW"));
    }
    let tp = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    // SAFETY: our token and a valid privilege list.
    if unsafe { AdjustTokenPrivileges(token.raw(), 0, &tp, 0, null_mut(), null_mut()) } == 0 {
        return Err(PlatformError::last_os("AdjustTokenPrivileges"));
    }
    Ok(())
}

/// `Pictures\Doppelpad`, created if needed.
fn screenshot_dir() -> Result<PathBuf> {
    let mut p: *mut u16 = null_mut();
    // SAFETY: valid out-pointer; the string is freed below.
    let found = unsafe { SHGetKnownFolderPath(&FOLDERID_Pictures, 0, null_mut(), &mut p) } == 0;
    // SAFETY: on success `p` is a NUL-terminated string, read before it is freed.
    let pictures = found.then(|| unsafe { super::from_wide_ptr(p) });
    // SAFETY: frees the string as documented (on failure too).
    unsafe { CoTaskMemFree(p.cast()) };
    let Some(pictures) = pictures else {
        return Err(PlatformError::Other(
            "cannot find the Pictures folder".into(),
        ));
    };
    let dir = PathBuf::from(pictures).join("Doppelpad");
    std::fs::create_dir_all(&dir).map_err(|source| PlatformError::Os {
        context: "creating the screenshot folder",
        source,
    })?;
    Ok(dir)
}

/// `<app> 2026-10-20 17-30-05.png`, numbered if that exists already.
fn screenshot_path(dir: &std::path::Path, app: Option<&str>) -> PathBuf {
    let mut t = SYSTEMTIME::default();
    // SAFETY: valid out-pointer.
    unsafe { GetLocalTime(&mut t) };
    let stamp = format!(
        "{:04}-{:02}-{:02} {:02}-{:02}-{:02}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    );
    let app = app
        .map(|a| a.trim_end_matches(".exe").trim_end_matches(".EXE"))
        .filter(|a| !a.is_empty())
        .unwrap_or("Screenshot");
    let mut path = dir.join(format!("{app} {stamp}.png"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{app} {stamp} ({n}).png"));
        n += 1;
    }
    path
}

/// Capture the monitor showing the foreground window and save it as PNG.
fn capture(path: &std::path::Path) -> Result<()> {
    let bitmap = grab(foreground_monitor()?)?;
    save_png(&bitmap, path)
}

/// The rectangle of the monitor that shows the foreground window (else the primary one).
fn foreground_monitor() -> Result<RECT> {
    // SAFETY: plain query.
    let fg: HWND = unsafe { GetForegroundWindow() };
    // SAFETY: plain queries; a null window is not passed on.
    let monitor = unsafe {
        if fg.is_null() {
            MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY)
        } else {
            MonitorFromWindow(fg, MONITOR_DEFAULTTOPRIMARY)
        }
    };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: a monitor handle and an out-struct with its size set.
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return Err(PlatformError::last_os("GetMonitorInfoW"));
    }
    Ok(info.rcMonitor)
}

/// A GDI bitmap, deleted on drop.
struct Bitmap(HBITMAP);

impl Drop for Bitmap {
    fn drop(&mut self) {
        // SAFETY: our bitmap, not selected into any device context any more.
        unsafe { DeleteObject(self.0) };
    }
}

/// The screen inside `r`, copied into a bitmap.
fn grab(r: RECT) -> Result<Bitmap> {
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    // SAFETY: the screen and a memory device context, both released here; the bitmap is
    // selected into the memory one only for the copy.
    unsafe {
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let bitmap = Bitmap(CreateCompatibleBitmap(screen, w, h));
        let old = SelectObject(mem, bitmap.0);
        let copied = BitBlt(mem, 0, 0, w, h, screen, r.left, r.top, SRCCOPY | CAPTUREBLT);
        SelectObject(mem, old);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);
        if copied == 0 {
            return Err(PlatformError::last_os("BitBlt"));
        }
        Ok(bitmap)
    }
}

/// Save `bitmap` as a PNG file with GDI+.
fn save_png(bitmap: &Bitmap, path: &std::path::Path) -> Result<()> {
    let file = wide(&path.display().to_string());
    let mut token = 0usize;
    let input = GdiplusStartupInput {
        GdiplusVersion: 1,
        ..Default::default()
    };
    // SAFETY: valid input and out-pointer; shut down below.
    let status = unsafe { GdiplusStartup(&mut token, &input, null_mut()) };
    if status != 0 {
        return Err(PlatformError::Other(format!(
            "GDI+ did not start ({status})"
        )));
    }
    let mut image: *mut GpBitmap = null_mut();
    // SAFETY: our bitmap and a valid out-pointer; the image is disposed of below.
    let mut status = unsafe { GdipCreateBitmapFromHBITMAP(bitmap.0, null_mut(), &mut image) };
    if status == 0 {
        // SAFETY: the image made above, a NUL-terminated path and the PNG encoder's id.
        status = unsafe { GdipSaveImageToFile(image.cast(), file.as_ptr(), &PNG_ENCODER, null()) };
        // SAFETY: disposes of the image made above.
        unsafe { GdipDisposeImage(image.cast()) };
    }
    // SAFETY: balances GdiplusStartup.
    unsafe { GdiplusShutdown(token) };
    if status != 0 {
        return Err(PlatformError::Other(format!(
            "cannot save the screenshot (GDI+ status {status})"
        )));
    }
    Ok(())
}

impl SystemControl for WinSystem {
    fn keep_awake(&mut self, on: bool) {
        let flags = if on {
            ES_CONTINUOUS | ES_SYSTEM_REQUIRED
        } else {
            ES_CONTINUOUS
        };
        // SAFETY: plain call; the state belongs to this (long-lived) thread.
        unsafe { SetThreadExecutionState(flags) };
    }

    fn desktop_active(&self) -> bool {
        // The secure desktops refuse ordinary processes.
        // SAFETY: plain query; the handle is closed below.
        let desk = unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS) };
        if desk.is_null() {
            return false;
        }
        let mut name = [0u16; 64];
        let mut needed = 0u32;
        // SAFETY: an open desktop; the buffer length is given in bytes.
        let ok = unsafe {
            GetUserObjectInformationW(
                desk,
                UOI_NAME,
                name.as_mut_ptr().cast(),
                size_of_val(&name) as u32,
                &mut needed,
            )
        };
        // SAFETY: closes the desktop opened above.
        unsafe { CloseDesktop(desk) };
        ok == 0 || super::from_wide(&name).eq_ignore_ascii_case("Default")
    }

    fn power(&mut self, action: PowerAction) -> Result<()> {
        let reason = SHTDN_REASON_MAJOR_OTHER | SHTDN_REASON_FLAG_PLANNED;
        if action != PowerAction::LogOff {
            enable_shutdown_privilege()?;
        }
        // Applications may still ask to save their work; nothing is forced.
        // SAFETY: plain calls.
        let ok = unsafe {
            match action {
                PowerAction::LogOff => ExitWindowsEx(EWX_LOGOFF, reason) != 0,
                PowerAction::Shutdown => ExitWindowsEx(EWX_POWEROFF, reason) != 0,
                PowerAction::Restart => ExitWindowsEx(EWX_REBOOT, reason) != 0,
                PowerAction::Sleep | PowerAction::Hibernate => {
                    SetSuspendState(action == PowerAction::Hibernate, false, false)
                }
            }
        };
        if ok {
            Ok(())
        } else {
            Err(PlatformError::last_os("power command"))
        }
    }

    fn screenshot(&mut self) -> Result<PathBuf> {
        let app = foreground::Foreground.foreground_process();
        let path = screenshot_path(&screenshot_dir()?, app.as_deref());
        capture(&path)?;
        Ok(path)
    }

    fn close_foreground_window(&mut self) -> Result<()> {
        // SAFETY: plain query.
        let fg = unsafe { GetForegroundWindow() };
        if fg.is_null() {
            return Err(PlatformError::Other("no window has the focus".into()));
        }
        // SAFETY: posting WM_CLOSE is what the window's own close button does.
        if unsafe { PostMessageW(fg, WM_CLOSE, 0, 0) } == 0 {
            return Err(PlatformError::last_os("PostMessageW"));
        }
        Ok(())
    }
}
