//! Power and window commands, screenshots, keeping the Mac awake and noticing the lock screen.
//!
//! Shutting down, restarting and logging out are Apple events to `loginwindow`, which asks
//! programs to quit as usual (they may still ask to save). Sleep is `pmset sleepnow`; macOS
//! has no hibernate of its own to ask for. Closing a window presses its close button through
//! the accessibility API (the Accessibility permission). Screenshots go through `screencapture`,
//! which shows other programs' windows only with the Screen Recording permission.

use std::path::PathBuf;

use objc2_application_services::{AXError, AXUIElement};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFRetained, CFString, CFType, CGPoint};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayBounds, CGGetDisplaysWithPoint, CGMainDisplayID,
    CGSessionCopyCurrentDictionary,
};
use objc2_io_kit::{
    IOPMAssertionCreateWithName, IOPMAssertionID, IOPMAssertionRelease, kIOPMAssertionLevelOn,
    kIOReturnSuccess,
};

use super::{process, windows};
use crate::tools::{run, screenshot_file};
use crate::{PlatformError, PowerAction, Result, SystemControl};

#[derive(Default)]
pub struct MacSystem {
    /// The power assertion that keeps the Mac awake, while one is held.
    awake: Option<IOPMAssertionID>,
}

/// Send `loginwindow` one of its Apple events (`shut`, `rest`, `rlgo`).
fn ask_loginwindow(event: &str) -> Result<()> {
    let script = format!("tell application \"loginwindow\" to «event aevt{event}»");
    run("/usr/bin/osascript", &["-e", &script])
}

/// `~/Pictures/Doppelpad`, created if needed.
fn screenshot_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| PlatformError::Other("cannot find the home folder".into()))?;
    let dir = PathBuf::from(home).join("Pictures").join("Doppelpad");
    std::fs::create_dir_all(&dir).map_err(|source| PlatformError::Os {
        context: "creating the screenshot folder",
        source,
    })?;
    Ok(dir)
}

/// The local time as `2026-10-20 17-30-05`.
fn stamp() -> String {
    // SAFETY: plain calls with valid pointers; `localtime_r` fills `tm`.
    let tm = unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        tm
    };
    format!(
        "{:04}-{:02}-{:02} {:02}-{:02}-{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

/// The display that shows `point`, else the main one.
fn display_at(point: CGPoint) -> CGDirectDisplayID {
    let mut display: CGDirectDisplayID = 0;
    let mut count = 0u32;
    // SAFETY: room for one display and a valid count.
    unsafe { CGGetDisplaysWithPoint(point, 1, &mut display, &mut count) };
    if count == 0 {
        CGMainDisplayID()
    } else {
        display
    }
}

/// An attribute of an accessibility element that is itself an element.
fn ax_element(element: &AXUIElement, attribute: &'static str) -> Option<CFRetained<AXUIElement>> {
    let mut value: *const CFType = std::ptr::null();
    // SAFETY: a valid element, attribute and out-pointer; the value comes retained.
    let err = unsafe {
        element.copy_attribute_value(
            &CFString::from_static_str(attribute),
            std::ptr::NonNull::from(&mut value),
        )
    };
    if err != AXError::Success {
        return None;
    }
    let value = std::ptr::NonNull::new(value.cast_mut())?;
    // SAFETY: a retained value we own now.
    let value: CFRetained<CFType> = unsafe { CFRetained::from_raw(value) };
    value.downcast::<AXUIElement>().ok()
}

impl SystemControl for MacSystem {
    fn power(&mut self, action: PowerAction) -> Result<()> {
        match action {
            PowerAction::Shutdown => ask_loginwindow("shut"),
            PowerAction::Restart => ask_loginwindow("rest"),
            PowerAction::LogOff => ask_loginwindow("rlgo"),
            PowerAction::Sleep => run("/usr/bin/pmset", &["sleepnow"]),
            PowerAction::Hibernate => Err(PlatformError::Unsupported(
                "hibernating on macOS (sleep keeps the memory on disk as well)",
            )),
        }
    }

    fn screenshot(&mut self) -> crate::Screenshot {
        Box::new(|| {
            let front = windows::front();
            let app = front.as_ref().and_then(|w| process::process_name(w.pid));
            let path = screenshot_file(&screenshot_dir()?, app.as_deref(), &stamp());
            let centre = front.map_or(CGPoint { x: 0.0, y: 0.0 }, |w| CGPoint {
                x: w.bounds.origin.x + w.bounds.size.width / 2.0,
                y: w.bounds.origin.y + w.bounds.size.height / 2.0,
            });
            let r = CGDisplayBounds(display_at(centre));
            let rect = format!(
                "{},{},{},{}",
                r.origin.x, r.origin.y, r.size.width, r.size.height
            );
            // Without the permission screencapture fails, or saves the desktop without windows.
            let taken = run(
                "/usr/sbin/screencapture",
                &["-x", "-t", "png", "-R", &rect, &path.display().to_string()],
            );
            match taken {
                Ok(()) if path.exists() => Ok(path),
                _ => Err(PlatformError::Other(
                    "no screenshot was taken: Doppelpad needs the Screen Recording permission \
                 (System Settings › Privacy & Security › Screen & System Audio Recording)"
                        .into(),
                )),
            }
        })
    }

    fn close_foreground_window(&mut self) -> Result<()> {
        let window = windows::front()
            .ok_or_else(|| PlatformError::Other("no window has the focus".into()))?;
        let pid = i32::try_from(window.pid)
            .map_err(|_| PlatformError::Other("no window has the focus".into()))?;
        // SAFETY: an element for a process id; it fails gracefully if the process is gone.
        let app = unsafe { AXUIElement::new_application(pid) };
        let button = ax_element(&app, "AXFocusedWindow")
            .and_then(|w| ax_element(&w, "AXCloseButton"))
            .ok_or_else(|| {
                PlatformError::Other(
                    "cannot reach the window's close button (Doppelpad needs the Accessibility \
                     permission)"
                        .into(),
                )
            })?;
        // SAFETY: a valid element and action name.
        let err = unsafe { button.perform_action(&CFString::from_static_str("AXPress")) };
        if err == AXError::Success {
            Ok(())
        } else {
            Err(PlatformError::Other(format!(
                "the close button did not respond ({})",
                err.0
            )))
        }
    }

    fn keep_awake(&mut self, on: bool) {
        match (on, self.awake) {
            (true, None) => {
                let mut id: IOPMAssertionID = 0;
                // SAFETY: valid strings and out-pointer.
                let rc = unsafe {
                    IOPMAssertionCreateWithName(
                        Some(&CFString::from_static_str("PreventUserIdleSystemSleep")),
                        kIOPMAssertionLevelOn,
                        Some(&CFString::from_static_str("Doppelpad is remapping input")),
                        &mut id,
                    )
                };
                if rc == kIOReturnSuccess {
                    self.awake = Some(id);
                } else {
                    tracing::debug!("cannot keep the Mac awake: IOKit error {rc:#x}");
                }
            }
            (false, Some(id)) => {
                IOPMAssertionRelease(id);
                self.awake = None;
            }
            _ => {}
        }
    }

    /// `false` while the screen is locked or another user's session is on the screen.
    fn desktop_active(&self) -> bool {
        let Some(session) = CGSessionCopyCurrentDictionary() else {
            return true; // not a graphical session: nothing to pause for
        };
        // SAFETY: the session dictionary has string keys.
        let session: &CFDictionary<CFString, CFType> = unsafe { session.cast_unchecked() };
        let flag = |key: &'static str| {
            session
                .get(&CFString::from_static_str(key))
                .and_then(|v| v.downcast_ref::<CFBoolean>().map(CFBoolean::as_bool))
        };
        flag("CGSSessionScreenIsLocked") != Some(true)
            && flag("kCGSSessionOnConsoleKey") != Some(false)
    }
}

impl Drop for MacSystem {
    fn drop(&mut self) {
        self.keep_awake(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_stamps() {
        assert_eq!("2026-10-20 17-30-05".len(), stamp().len());
    }
}
