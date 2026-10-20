//! Power, screenshot and window commands that mappings can trigger.

use std::path::PathBuf;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HANDLE, HWND, LUID, POINT, RECT, SYSTEMTIME};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CAPTUREBLT, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
    GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint, MonitorFromWindow,
    ReleaseDC, SRCCOPY, SelectObject,
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
use windows_sys::Win32::System::Power::SetSuspendState;
use windows_sys::Win32::System::Shutdown::{
    EWX_LOGOFF, EWX_POWEROFF, EWX_REBOOT, ExitWindowsEx, SHTDN_REASON_FLAG_PLANNED,
    SHTDN_REASON_MAJOR_OTHER,
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
    // SAFETY: plain token calls on our own process; the handle is closed by OwnedHandle.
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        ) == 0
        {
            return Err(PlatformError::last_os("OpenProcessToken"));
        }
        let token = OwnedHandle(token);
        let mut luid = LUID::default();
        if LookupPrivilegeValueW(null(), SE_SHUTDOWN_NAME, &mut luid) == 0 {
            return Err(PlatformError::last_os("LookupPrivilegeValueW"));
        }
        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        if AdjustTokenPrivileges(token.0, 0, &tp, 0, null_mut(), null_mut()) == 0 {
            return Err(PlatformError::last_os("AdjustTokenPrivileges"));
        }
    }
    Ok(())
}

/// `Pictures\Doppelpad`, created if needed.
fn screenshot_dir() -> Result<PathBuf> {
    // SAFETY: the returned string is freed with CoTaskMemFree as documented.
    let pictures = unsafe {
        let mut p: *mut u16 = null_mut();
        if SHGetKnownFolderPath(&FOLDERID_Pictures, 0, null_mut(), &mut p) != 0 {
            CoTaskMemFree(p.cast());
            return Err(PlatformError::Other(
                "cannot find the Pictures folder".into(),
            ));
        }
        let len = (0..).take_while(|&i| *p.add(i) != 0).count();
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
        CoTaskMemFree(p.cast());
        s
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
    // SAFETY: GDI objects are created, used and released in this block; GDI+ is started
    // and shut down around its use.
    unsafe {
        let fg: HWND = GetForegroundWindow();
        let monitor = if fg.is_null() {
            MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY)
        } else {
            MonitorFromWindow(fg, MONITOR_DEFAULTTOPRIMARY)
        };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT::default(),
            rcWork: RECT::default(),
            dwFlags: 0,
        };
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return Err(PlatformError::last_os("GetMonitorInfoW"));
        }
        let r = info.rcMonitor;
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bitmap);
        let copied = BitBlt(mem, 0, 0, w, h, screen, r.left, r.top, SRCCOPY | CAPTUREBLT);
        SelectObject(mem, old);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);
        if copied == 0 {
            DeleteObject(bitmap);
            return Err(PlatformError::last_os("BitBlt"));
        }

        let mut token = 0usize;
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            ..Default::default()
        };
        let status = GdiplusStartup(&mut token, &input, null_mut());
        if status != 0 {
            DeleteObject(bitmap);
            return Err(PlatformError::Other(format!(
                "GDI+ did not start ({status})"
            )));
        }
        let mut image: *mut GpBitmap = null_mut();
        let mut status = GdipCreateBitmapFromHBITMAP(bitmap, null_mut(), &mut image);
        if status == 0 {
            let file = wide(&path.display().to_string());
            status = GdipSaveImageToFile(image.cast(), file.as_ptr(), &PNG_ENCODER, null());
            GdipDisposeImage(image.cast());
        }
        GdiplusShutdown(token);
        DeleteObject(bitmap);
        if status != 0 {
            return Err(PlatformError::Other(format!(
                "cannot save the screenshot (GDI+ status {status})"
            )));
        }
    }
    Ok(())
}

impl SystemControl for WinSystem {
    fn power(&mut self, action: PowerAction) -> Result<()> {
        let reason = SHTDN_REASON_MAJOR_OTHER | SHTDN_REASON_FLAG_PLANNED;
        // Applications may still ask to save their work; nothing is forced.
        // SAFETY: plain Win32 calls.
        let ok = unsafe {
            match action {
                PowerAction::LogOff => ExitWindowsEx(EWX_LOGOFF, reason) != 0,
                PowerAction::Shutdown => {
                    enable_shutdown_privilege()?;
                    ExitWindowsEx(EWX_POWEROFF, reason) != 0
                }
                PowerAction::Restart => {
                    enable_shutdown_privilege()?;
                    ExitWindowsEx(EWX_REBOOT, reason) != 0
                }
                PowerAction::Sleep | PowerAction::Hibernate => {
                    enable_shutdown_privilege()?;
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
        // SAFETY: posting WM_CLOSE is what the window's own close button does.
        unsafe {
            let fg = GetForegroundWindow();
            if fg.is_null() {
                return Err(PlatformError::Other("no window has the focus".into()));
            }
            if PostMessageW(fg, WM_CLOSE, 0, 0) == 0 {
                return Err(PlatformError::last_os("PostMessageW"));
            }
        }
        Ok(())
    }
}
