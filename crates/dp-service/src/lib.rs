//! The Doppelpad remapping service, shared by the `doppelpad` CLI/daemon and the GUI:
//! the device/engine runtime, slots, automatic per-application switching, IPC handling
//! and live snapshots for user interfaces.

pub mod anticheat;
pub mod calibration;
mod client;
pub mod dsu;
pub mod host;
pub mod hotkeys;
pub mod httpapi;
pub mod live;
mod net;
pub mod runtime;
pub mod service;
pub mod slots;
mod timing;

pub use host::{HostOptions, ServiceHandle, spawn, spawn_with};
pub use live::{LiveDevice, LiveDeviceSlots, LiveRadial, LiveState};
pub use runtime::{DriverState, DriverStatus, Runtime, SessionInfo, TimerResolution};
pub use service::{ServiceCmd, ServiceOptions, ServiceReply, ipc_devices, on_off};
pub use slots::{PlayerLights, SLOTS, SlotSet, SlotState};
pub use timing::TICK;
