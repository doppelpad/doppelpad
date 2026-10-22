//! Power and window commands: systemd (`systemctl`, `loginctl`) for power, the EWMH
//! `_NET_CLOSE_WINDOW` request for closing the focused X11 window. Screenshots are not
//! available yet.

use std::path::PathBuf;
use std::process::Command;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt, EventMask};

use super::x11::X11;
use crate::{PlatformError, PowerAction, Result, SystemControl};

pub struct LinuxSystem;

fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
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
    x11.conn.flush().map_err(|e| err(&e))?;
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
}
