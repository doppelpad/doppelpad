//! Foreground application through X11: `_NET_ACTIVE_WINDOW` on the root window, that
//! window's `_NET_WM_PID`, then the process name from `/proc`.
//!
//! Works on X11 sessions and, through XWayland, for X11 windows (which includes Wine /
//! Proton games) on Wayland desktops whose compositor maintains `_NET_ACTIVE_WINDOW`.
//! Native Wayland has no portable way to ask for the focused window, so without an X display
//! this reports `None`.

use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

use super::procname::process_name;
use crate::ForegroundWatcher;

const RECONNECT: Duration = Duration::from_secs(10);

struct X11 {
    conn: RustConnection,
    root: Window,
    active_window: Atom,
    wm_pid: Atom,
}

impl X11 {
    fn connect() -> Option<X11> {
        std::env::var_os("DISPLAY")?;
        let (conn, screen) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots.get(screen)?.root;
        let atom = |name: &[u8]| Some(conn.intern_atom(false, name).ok()?.reply().ok()?.atom);
        let active_window = atom(b"_NET_ACTIVE_WINDOW")?;
        let wm_pid = atom(b"_NET_WM_PID")?;
        Some(X11 {
            conn,
            root,
            active_window,
            wm_pid,
        })
    }

    fn card32(
        &self,
        window: Window,
        property: Atom,
        ty: AtomEnum,
    ) -> Result<Option<u32>, ReplyError> {
        let reply = self
            .conn
            .get_property(false, window, property, ty, 0, 1)?
            .reply()?;
        Ok(reply.value32().and_then(|mut v| v.next()))
    }

    fn foreground_pid(&self) -> Result<Option<u32>, ReplyError> {
        let Some(window) = self
            .card32(self.root, self.active_window, AtomEnum::WINDOW)?
            .filter(|&w| w != 0)
        else {
            return Ok(None);
        };
        self.card32(window, self.wm_pid, AtomEnum::CARDINAL)
    }
}

#[derive(Default)]
pub struct X11Foreground {
    x11: Option<X11>,
    retry_at: Option<Instant>,
}

impl ForegroundWatcher for X11Foreground {
    fn foreground_process(&mut self) -> Option<String> {
        if self.x11.is_none() && self.retry_at.is_none_or(|t| Instant::now() >= t) {
            self.x11 = X11::connect();
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
