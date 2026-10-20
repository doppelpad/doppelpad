//! Snapshot of everything a user interface shows live: devices with their current
//! input, the active profile and layer, slots, switches, warnings and driver status.

use dp_core::pad::PadState;
use dp_platform::PadFamily;
use serde::Serialize;

use crate::runtime::DriverStatus;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LiveDevice {
    pub id: String,
    pub name: String,
    pub family: PadFamily,
    pub backend: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub state: PadState,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct LiveState {
    pub devices: Vec<LiveDevice>,
    /// Controller used as input.
    pub primary: Option<String>,
    pub profile: Option<String>,
    pub profile_path: Option<String>,
    pub layer: u8,
    /// Config file per slot.
    pub slots: Vec<Option<String>>,
    pub active_slot: usize,
    pub remap_on: bool,
    pub auto_switch: bool,
    /// Remapping is paused (the editor has focus).
    pub suspended: bool,
    /// Executable of the foreground application.
    pub foreground: Option<String>,
    pub virtual_pad: Option<String>,
    pub hidden_devices: usize,
    pub warnings: Vec<String>,
    pub drivers: DriverStatus,
}
