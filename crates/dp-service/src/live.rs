//! Snapshot of everything a user interface shows live: devices with their current
//! input, the active profile and layer, slots, switches, warnings and driver status.

use dp_core::pad::PadState;
use dp_core::profile::{Crosshair, RadialMenu};
use dp_platform::generic::RawInput;
use dp_platform::groups::{DeviceGroup, SeenDevice};
use dp_platform::{Battery, PadFamily};
use serde::Serialize;

use crate::calibration::CalibrationStatus;
use crate::dsu::DsuSettings;
use crate::runtime::{DriverStatus, SessionInfo};
use crate::slots::PlayerLights;

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
    /// Remapping is paused (the editor has focus).
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
}

/// The slots of a controller that has its own.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct LiveDeviceSlots {
    pub id: String,
    pub slots: Vec<Option<String>>,
    pub active_slot: usize,
}
