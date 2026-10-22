//! Snapshot of everything a user interface shows live: devices with their current
//! input, the active profile and layer, slots, switches, warnings and driver status.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use dp_core::pad::PadState;
use dp_core::profile::{Crosshair, RadialMenu};
use dp_platform::generic::RawInput;
use dp_platform::groups::{DeviceGroup, SeenDevice};
use dp_platform::{Battery, PadFamily};
use serde::Serialize;

use crate::calibration::CalibrationStatus;
use crate::dsu::DsuSettings;
use crate::httpapi::HttpSettings;
use crate::runtime::{DriverStatus, SessionInfo};
use crate::slots::{PlayerLights, RuntimeOptions};
use crate::sync::lock_ignore_poison;

/// How often the live snapshot is refreshed, and how often its fastest readers look.
pub const LIVE_INTERVAL: Duration = Duration::from_millis(16);

/// The latest live snapshot, shared by the service thread with its readers. Its generation
/// goes up whenever it changes, so a reader looks at it only then.
#[derive(Clone, Default)]
pub struct SharedLive(Arc<Mutex<(u64, Arc<LiveState>)>>);

impl SharedLive {
    /// Store `state` if it is the first or differs from the current snapshot.
    pub(crate) fn publish(&self, state: LiveState) {
        let mut current = lock_ignore_poison(&self.0);
        if current.0 == 0 || *current.1 != state {
            current.0 += 1;
            current.1 = Arc::new(state);
        }
    }

    /// The current snapshot.
    pub fn get(&self) -> Arc<LiveState> {
        lock_ignore_poison(&self.0).1.clone()
    }

    /// The snapshot and its generation, if it changed since generation `seen` (0: never
    /// seen).
    pub fn since(&self, seen: u64) -> Option<(u64, Arc<LiveState>)> {
        let current = lock_ignore_poison(&self.0);
        (current.0 != seen).then(|| (current.0, current.1.clone()))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LiveDevice {
    pub id: String,
    pub name: String,
    pub family: PadFamily,
    pub backend: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub state: PadState,
    /// Gyro calibration, for controllers with motion sensors.
    pub gyro_calibration: Option<CalibrationStatus>,
    /// `None` when the controller does not report one.
    pub battery: Option<Battery>,
    /// Unmapped input of a generic pad, for identifying its buttons.
    pub raw: Option<RawInput>,
}

/// The local HTTP API.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct LiveHttp {
    pub settings: HttpSettings,
    /// Where it listens while it runs.
    pub address: Option<String>,
    pub error: Option<String>,
}

/// The DSU motion server.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct LiveDsu {
    pub settings: DsuSettings,
    /// Where it listens while it runs.
    pub address: Option<String>,
    /// Emulators subscribed right now.
    pub clients: usize,
    pub error: Option<String>,
}

/// An open radial menu and the sector pointed at.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LiveRadial {
    pub menu: RadialMenu,
    pub pointed: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct LiveState {
    pub devices: Vec<LiveDevice>,
    /// Controller used as input.
    pub primary: Option<String>,
    pub profile: Option<String>,
    pub profile_path: Option<String>,
    pub layer: u8,
    /// Name of the active layer, if it has one.
    pub layer_name: Option<String>,
    /// Config file per slot.
    pub slots: Vec<Option<String>>,
    pub active_slot: usize,
    pub remap_on: bool,
    pub auto_switch: bool,
    /// Autodetect chose the running config for the game in front, not the active slot.
    pub auto_active: bool,
    pub player_lights: PlayerLights,
    pub mic_led: bool,
    /// Every device the backends report, blocked and grouped ones included.
    pub seen_devices: Vec<SeenDevice>,
    /// Devices Doppelpad ignores.
    pub blocked: Vec<String>,
    pub groups: Vec<DeviceGroup>,
    /// Wireless controllers turn off after this many minutes without input; 0 never.
    pub power_off_after_min: u32,
    /// Generic pad models with a recorded layout (`vvvv:pppp`).
    pub layouts: Vec<String>,
    pub dsu: LiveDsu,
    pub http: LiveHttp,
    pub runtime: RuntimeOptions,
    /// Remapping is paused because the desktop is locked or a secure desktop shows.
    pub locked: bool,
    /// Remapping is paused because the config's game is not in front.
    pub focus_paused: bool,
    /// Remapping is paused: the editor has focus, the desktop is locked, or the config's game
    /// is not in front (`focus_paused`).
    pub suspended: bool,
    /// Executable of the foreground application.
    pub foreground: Option<String>,
    pub virtual_pad: Option<String>,
    pub hidden_devices: usize,
    pub warnings: Vec<String>,
    pub drivers: DriverStatus,
    /// Controllers with slots of their own.
    pub device_slots: Vec<LiveDeviceSlots>,
    /// Every running profile, the default session first.
    pub sessions: Vec<SessionInfo>,
    /// The crosshair to draw over the game.
    pub crosshair: Option<Crosshair>,
    /// The radial menu open over the game.
    pub radial: Option<LiveRadial>,
    /// The running profile has radial menus (their window can be made ready).
    pub has_radial: bool,
    /// The live controller overlay is on.
    pub pad_overlay: bool,
    /// The overlay listing the current layer's mappings is on.
    pub mappings_overlay: bool,
    /// How often the app-navigation hotkey fired; the app turns its controller navigation
    /// on or off at each change.
    pub app_navigation: u32,
}

/// The slots of a controller that has its own.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct LiveDeviceSlots {
    pub id: String,
    pub slots: Vec<Option<String>>,
    pub active_slot: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generation_moves_only_on_change() {
        let live = SharedLive::default();
        assert!(live.since(0).is_none(), "nothing published yet");
        live.publish(LiveState::default());
        let (seen, _) = live.since(0).unwrap();
        assert_eq!(seen, 1, "the first state counts, even the default one");
        live.publish(LiveState::default());
        assert!(live.since(seen).is_none(), "the same state again");
        live.publish(LiveState {
            layer: 2,
            ..LiveState::default()
        });
        assert_eq!(live.since(seen).unwrap().1.layer, 2);
        assert_eq!(live.get().layer, 2);
    }
}
