//! Driver availability and the devices the backends cannot use.

use dp_platform::GamepadBackend;
use serde::Serialize;

use super::Runtime;

/// Availability of one optional driver.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DriverState {
    pub available: bool,
    /// Why it is unavailable, if it is.
    pub error: Option<String>,
}

impl std::fmt::Display for DriverState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.error, self.available) {
            (_, true) => f.write_str("ok"),
            (Some(e), false) => write!(f, "unavailable ({e})"),
            (None, false) => f.write_str("unavailable"),
        }
    }
}

impl From<dp_platform::Result<()>> for DriverState {
    fn from(r: dp_platform::Result<()>) -> Self {
        match r {
            Ok(()) => DriverState {
                available: true,
                error: None,
            },
            Err(e) => DriverState {
                available: false,
                error: Some(e.to_string()),
            },
        }
    }
}

/// Drivers Doppelpad can use: ViGEmBus for virtual pads, HidHide for hiding devices (on
/// Linux uinput and grabbing), what keyboard and mouse need from the OS (on macOS the
/// Accessibility permission), and Bluetooth for Switch 2 controllers (on macOS its
/// permission, elsewhere an adapter that is on).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DriverStatus {
    pub virtual_pads: DriverState,
    pub device_hiding: DriverState,
    pub input_access: DriverState,
    pub bluetooth: DriverState,
}

impl Runtime {
    /// Controllers the backends found but cannot use, explained for the user.
    pub fn device_problems(&self) -> Vec<String> {
        self.pads.problems()
    }

    /// Connected controllers the hiding driver hides from Doppelpad itself, explained for
    /// the user. Asks the driver, so call it now and then rather than every tick.
    pub fn hiding_problems(&self) -> Vec<String> {
        self.platform.hider.problems()
    }
}
