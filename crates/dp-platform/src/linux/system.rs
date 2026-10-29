//! Power and window commands: systemd (`systemctl`, `loginctl`) for power, the EWMH
//! `_NET_CLOSE_WINDOW` request for closing the focused X11 window. Screenshots are not
//! available yet. Keeping the computer awake and noticing the lock screen go through
//! systemd-logind on the system bus ([`super::logind`]).

use std::os::fd::OwnedFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt, EventMask};

use super::x11::X11;
use crate::tools::run;
use crate::{PlatformError, PowerAction, Result, SystemControl};

#[derive(Default)]
pub struct LinuxSystem {
    /// logind's idle lock, held while this descriptor is open: until remapping no longer needs
    /// it, or Doppelpad ends in any way.
    inhibitor: Option<OwnedFd>,
    /// Whether the user's session is locked, followed from the first question on.
    lock: OnceLock<LockWatch>,
}

fn close_active_x11_window() -> Result<()> {
    let err = |e: &dyn std::fmt::Display| PlatformError::Other(format!("X11: {e}"));
    let x11 = X11::connect().map_err(|e| err(&e))?;
    let close = x11.atom(b"_NET_CLOSE_WINDOW").map_err(|e| err(&e))?;
    let window = x11
        .active_window()
        .map_err(|e| err(&e))?
        .ok_or_else(|| PlatformError::Other("no window has the focus".into()))?;
    // Timestamp 0 (current time), source 2 (a pager or similar tool).
    let event = ClientMessageEvent::new(32, window, close, [0u32, 2, 0, 0, 0]);
    x11.conn
        .send_event(
            false,
            x11.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )
        .map_err(|e| err(&e))?;
    // Wait for a reply, so the server has handled the request before the connection closes:
    // it drops the requests it has not handled yet when a client goes away.
    x11.conn
        .get_input_focus()
        .map_err(|e| err(&e))?
        .reply()
        .map_err(|e| err(&e))?;
    Ok(())
}

impl SystemControl for LinuxSystem {
    fn power(&mut self, action: PowerAction) -> Result<()> {
        match action {
            PowerAction::Shutdown => run("systemctl", &["poweroff"]),
            PowerAction::Restart => run("systemctl", &["reboot"]),
            PowerAction::Sleep => run("systemctl", &["suspend"]),
            PowerAction::Hibernate => run("systemctl", &["hibernate"]),
            PowerAction::LogOff => match std::env::var("XDG_SESSION_ID") {
                Ok(id) if !id.is_empty() => run("loginctl", &["terminate-session", &id]),
                _ => Err(PlatformError::Other("no login session to end".into())),
            },
        }
    }

    fn screenshot(&mut self) -> crate::Screenshot {
        Box::new(|| Err(PlatformError::Unsupported("screenshots on Linux")))
    }

    fn close_foreground_window(&mut self) -> Result<()> {
        close_active_x11_window()
    }

    /// Takes logind's idle lock: the computer does not count as idle, so it does not go to
    /// sleep on its own. Whether the desktop's own power settings follow the lock is up to
    /// the desktop.
    fn keep_awake(&mut self, on: bool) {
        if on == self.inhibitor.is_some() {
            return;
        }
        if on {
            match super::logind::inhibit_idle("Remapping plays input back") {
                Ok(fd) => self.inhibitor = Some(fd),
                Err(e) => tracing::debug!("cannot keep the computer awake: {e}"),
            }
        } else {
            // Closing the descriptor gives the lock back.
            self.inhibitor = None;
        }
    }

    fn desktop_active(&self) -> bool {
        !self
            .lock
            .get_or_init(LockWatch::start)
            .locked
            .load(Ordering::Relaxed)
    }
}

/// How often to ask logind whether the session is locked.
const LOCK_POLL: Duration = Duration::from_secs(1);

/// Answers in a row that logind may fail before the watch gives up.
const LOCK_TRIES: u32 = 3;

/// A thread asking logind for the session's `LockedHint` until it is dropped. Without an
/// answer (no systemd, no session) it gives up, and the desktop counts as active.
struct LockWatch {
    locked: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl LockWatch {
    fn start() -> LockWatch {
        let locked = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (locked2, stop2) = (locked.clone(), stop.clone());
        let spawned = std::thread::Builder::new()
            .name("dp-logind-lock".into())
            .spawn(move || {
                let mut failed = 0;
                while !stop2.load(Ordering::Relaxed) && failed < LOCK_TRIES {
                    match locked_now() {
                        Some(hint) => {
                            failed = 0;
                            locked2.store(hint, Ordering::Relaxed);
                        }
                        None => failed += 1,
                    }
                    std::thread::sleep(LOCK_POLL);
                }
                if failed >= LOCK_TRIES {
                    tracing::debug!("logind does not tell whether the session is locked");
                    locked2.store(false, Ordering::Relaxed);
                }
            });
        if let Err(e) = spawned {
            tracing::debug!("cannot watch for the lock screen: {e}");
        }
        LockWatch { locked, stop }
    }
}

impl Drop for LockWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn locked_now() -> Option<bool> {
    super::logind::locked()
        .inspect_err(|e| tracing::trace!("logind does not say whether the session is locked: {e}"))
        .ok()
}
