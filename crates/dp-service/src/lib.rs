//! The Doppelpad remapping service, shared by the `doppelpad` CLI/daemon and the GUI:
//! the device/engine runtime, slots, automatic per-application switching, IPC handling
//! and live snapshots for user interfaces.

mod anticheat;
mod calibration;
mod client;
mod dsu;
mod host;
mod hotkeys;
mod httpapi;
mod live;
mod net;
mod runtime;
mod service;
mod slots;
mod sync;
mod timing;

pub use calibration::CalibrationStatus;
pub use dsu::DsuSettings;
pub use host::{HostOptions, Ipc, ServiceHandle, spawn, spawn_with};
pub use httpapi::HttpSettings;
pub use live::{
    LIVE_INTERVAL, LiveDevice, LiveDeviceSlots, LiveRadial, LiveScreenshot, LiveState, SharedLive,
};
pub use runtime::{DriverState, DriverStatus, Runtime, SessionInfo, SessionOutput};
pub use service::{ServiceCmd, ServiceOptions, ServiceReply, ipc_devices, on_off};
pub use slots::{PlayerLights, RuntimeOptions, SLOTS, SlotSet, SlotState};
pub use timing::TICK;
