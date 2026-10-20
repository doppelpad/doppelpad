//! The Doppelpad remapping service, shared by the `doppelpad` CLI/daemon and the GUI:
//! the device/engine runtime, slots, automatic per-application switching, IPC handling
//! and live snapshots for user interfaces.

pub mod calibration;
pub mod host;
pub mod hotkeys;
pub mod live;
pub mod runtime;
pub mod service;
pub mod slots;

pub use host::{HostOptions, ServiceHandle, spawn, spawn_with};
pub use live::{LiveDevice, LiveDeviceSlots, LiveRadial, LiveState};
pub use runtime::{DriverState, DriverStatus, Runtime, SessionInfo, TimerResolution};
pub use service::{Service, ServiceCmd, ServiceOptions, ServiceReply, ipc_devices};
pub use slots::{DeviceSlots, SLOTS, SlotState};
