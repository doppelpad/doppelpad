//! The system bus: one connection for BlueZ ([`super::bluez`]) and logind ([`super::logind`]),
//! and a check that tells the user when the bus is out of reach. Keeping the computer awake,
//! the lock-screen pause and turning controllers off need it, and would fail without a word: a
//! container may not share the bus, as the Arch Distrobox container on SteamOS does not.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use zbus::blocking::Connection;

/// The longest any call on the connection may take.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

static BUS: Mutex<Option<Connection>> = Mutex::new(None);

/// The system bus, connected on first use and again after [`reset`].
pub(super) fn connection() -> zbus::Result<Connection> {
    if let Some(bus) = &*bus() {
        return Ok(bus.clone());
    }
    // Not under the lock: connecting may take a while. Should two threads connect at once, the
    // first connection stays.
    let connected = zbus::blocking::connection::Builder::system()?
        .method_timeout(CALL_TIMEOUT)
        .build()?;
    Ok(bus().get_or_insert(connected).clone())
}

fn bus() -> MutexGuard<'static, Option<Connection>> {
    BUS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The connection broke: the next [`connection`] makes a new one.
pub(super) fn reset() {
    bus().take();
}

/// What the system bus cannot do for Doppelpad, for the user: nothing when logind answers.
pub(super) fn problems() -> Vec<String> {
    let unreachable = |e: &zbus::Error| {
        tracing::debug!("the system bus cannot be reached: {e}");
        vec![
            "the system bus cannot be reached, so keeping the computer awake, the pause while the \
             screen is locked and turning controllers off do not work; in a container, set \
             DBUS_SYSTEM_BUS_ADDRESS (see docs/LINUX.md)"
                .to_owned(),
        ]
    };
    let bus = match connection() {
        Ok(bus) => bus,
        Err(e) => return unreachable(&e),
    };
    let ping = bus.call_method(
        Some("org.freedesktop.login1"),
        "/org/freedesktop/login1",
        Some("org.freedesktop.DBus.Peer"),
        "Ping",
        &(),
    );
    match ping {
        Ok(_) => Vec::new(),
        // logind (or the bus) answered with an error: the bus works, logind does not.
        Err(e @ zbus::Error::MethodError(..)) => {
            tracing::debug!("systemd-logind does not answer: {e}");
            vec![
                "systemd-logind does not answer on the system bus, so keeping the computer awake \
                 and the pause while the screen is locked do not work"
                    .to_owned(),
            ]
        }
        Err(e) => {
            reset();
            unreachable(&e)
        }
    }
}
