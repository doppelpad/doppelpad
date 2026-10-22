//! Foreground application through X11: `_NET_ACTIVE_WINDOW` on the root window, that
//! window's `_NET_WM_PID`, then the process name from `/proc`.
//!
//! Works on X11 sessions and, through XWayland, for X11 windows (which includes Wine /
//! Proton games) on Wayland desktops whose compositor maintains `_NET_ACTIVE_WINDOW`.
//! Native Wayland has no portable way to ask for the focused window, so without an X display
//! this reports `None`.

use std::time::{Duration, Instant};

use x11rb::errors::ReplyError;
use x11rb::protocol::xproto::{Atom, AtomEnum};

use super::procname::process_name;
use super::x11::X11;
use crate::ForegroundWatcher;

const RECONNECT: Duration = Duration::from_secs(10);

/// The connection and the atom the watcher needs.
struct Watch {
    x11: X11,
    wm_pid: Atom,
}

impl Watch {
    fn connect() -> Option<Watch> {
        let x11 = X11::connect().ok()?;
        let wm_pid = x11.atom(b"_NET_WM_PID").ok()?;
        Some(Watch { x11, wm_pid })
    }

    /// The active window's own `_NET_WM_PID`. Unlike on Windows, a dialog is not traced back
    /// to the window that owns it (`WM_TRANSIENT_FOR`): it counts as the process that drew it.
    fn foreground_pid(&self) -> Result<Option<u32>, ReplyError> {
        let Some(window) = self.x11.active_window()? else {
            return Ok(None);
        };
        self.x11.card32(window, self.wm_pid, AtomEnum::CARDINAL)
    }
}

#[derive(Default)]
pub struct X11Foreground {
    x11: Option<Watch>,
    retry_at: Option<Instant>,
}

impl ForegroundWatcher for X11Foreground {
    fn foreground_process(&mut self) -> Option<String> {
        if self.x11.is_none() && self.retry_at.is_none_or(|t| Instant::now() >= t) {
            self.x11 = Watch::connect();
            if self.x11.is_some() {
                tracing::debug!("watching the foreground window through X11");
            } else {
                self.retry_at = Some(Instant::now() + RECONNECT);
            }
        }
        match self.x11.as_ref()?.foreground_pid() {
            Ok(pid) => process_name(pid?),
            // The window went away between the two queries.
            Err(ReplyError::X11Error(_)) => None,
            Err(e) => {
                tracing::debug!("X11 connection lost: {e}");
                self.x11 = None;
                self.retry_at = Some(Instant::now() + RECONNECT);
                None
            }
        }
    }

    /// Uses the connection `foreground_process` keeps up; `None` without one.
    fn foreground_pid(&mut self) -> Option<u32> {
        self.x11.as_ref()?.foreground_pid().ok().flatten()
    }
}
