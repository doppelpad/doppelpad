//! Power and window commands: systemd (`systemctl`, `loginctl`) for power, the EWMH
//! `_NET_CLOSE_WINDOW` request for closing the focused X11 window. Screenshots are not
//! available yet. Keeping the computer awake and noticing the lock screen go through
//! systemd-logind (`systemd-inhibit`, `busctl`).

use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt, EventMask};

use super::logind::{LOCKED_HINT_QUERY, locked_hint};
use super::x11::X11;
use crate::{PlatformError, PowerAction, Result, SystemControl};

#[derive(Default)]
pub struct LinuxSystem {
    /// `systemd-inhibit` holding logind's idle lock while it runs. The command it runs reads
    /// our pipe, so it ends when the pipe closes: when remapping no longer needs the lock,
    /// or when Doppelpad ends in any way.
    inhibitor: Option<Child>,
    /// Whether the user's session is locked, followed from the first question on.
    lock: OnceLock<LockWatch>,
}

fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = super::host_command(program)
        .args(args)
        .status()
        .map_err(|source| PlatformError::Os {
            context: "starting systemctl/loginctl",
            source,
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(PlatformError::Other(format!(
            "`{program} {}` failed",
            args.join(" ")
        )))
    }
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

    fn screenshot(&mut self) -> Result<PathBuf> {
        Err(PlatformError::Unsupported("screenshots on Linux"))
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
            let spawned = super::host_command("systemd-inhibit")
                .args([
                    "--what=idle",
                    "--who=Doppelpad",
                    "--why=Remapping plays input back",
                    "--mode=block",
                    "cat",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            match spawned {
                Ok(child) => self.inhibitor = Some(child),
                Err(e) => tracing::debug!("cannot keep the computer awake (systemd-inhibit): {e}"),
            }
        } else if let Some(mut child) = self.inhibitor.take() {
            // Closing its input ends `cat`, and with it the lock.
            drop(child.stdin.take());
            std::thread::spawn(move || child.wait());
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
    let out = super::host_command("busctl")
        .args(LOCKED_HINT_QUERY)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    locked_hint(&String::from_utf8_lossy(&out.stdout))
}
