//! Asking systemd-logind about the user's session, and for its idle lock, on the system bus
//! ([`super::sysbus`]).

use std::os::fd::OwnedFd;

use zbus::zvariant::{self, OwnedValue};

const LOGIN1: &str = "org.freedesktop.login1";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

/// Whether the session Doppelpad runs in is locked, or the user's graphical session when it
/// runs outside one (`session/auto`): the desktop sets its `LockedHint` while its lock screen
/// shows.
pub fn locked() -> zbus::Result<bool> {
    let value: OwnedValue = super::sysbus::connection()?
        .call_method(
            Some(LOGIN1),
            "/org/freedesktop/login1/session/auto",
            Some(PROPERTIES),
            "Get",
            &("org.freedesktop.login1.Session", "LockedHint"),
        )?
        .body()
        .deserialize()?;
    Ok(bool::try_from(value)?)
}

/// Take logind's idle lock: the computer does not count as idle while the returned file
/// descriptor is open, and closing it (also when Doppelpad ends in any way) gives the lock
/// back.
pub fn inhibit_idle(why: &str) -> zbus::Result<OwnedFd> {
    let fd: zvariant::OwnedFd = super::sysbus::connection()?
        .call_method(
            Some(LOGIN1),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "Inhibit",
            &("idle", "Doppelpad", why, "block"),
        )?
        .body()
        .deserialize()?;
    Ok(fd.into())
}
