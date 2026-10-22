//! What the service is asked and what it answers.

use std::path::PathBuf;

use dp_core::hotkeys::Hotkeys;
use dp_ipc::{Device, Response, Status};
use dp_platform::generic::GenericLayout;
use dp_platform::groups::DeviceGroup;

use crate::dsu::DsuSettings;
use crate::httpapi::HttpSettings;
use crate::slots::{PlayerLights, RuntimeOptions};

/// What the service can be asked: everything the IPC protocol offers (`doppelpad
/// apply|stop|status|...`, see [`Service::handle_ipc`]) and more for the GUI.
#[derive(Clone, Debug, PartialEq)]
pub enum ServiceCmd {
    /// The status in IPC form ([`ServiceReply::Status`]).
    GetStatus,
    /// The connected controllers in IPC form ([`ServiceReply::Devices`]).
    GetDevices,
    /// Load and activate a config file outside the slots until something else is chosen.
    Apply(PathBuf),
    /// Stop remapping.
    Stop,
    /// Re-read the profiles directory (games or configs changed on disk).
    Reload,
    /// Use another profiles directory. With `moved_from`, the library was moved from there:
    /// slots and running configs holding its files now name them in `dir`.
    UseLibrary {
        dir: PathBuf,
        moved_from: Option<PathBuf>,
    },
    /// Acknowledge; the host then stops the service.
    Shutdown,
    /// Put a config into a slot and make that slot active. `device`: the slots of that
    /// controller (which gets slots of its own if it has none yet), else the shared ones.
    ApplyToSlot {
        device: Option<String>,
        slot: usize,
        path: PathBuf,
    },
    SelectSlot {
        device: Option<String>,
        slot: usize,
    },
    ClearSlot {
        device: Option<String>,
        slot: usize,
    },
    /// Give a controller slots of its own (it then runs its own profile and virtual pad),
    /// or let it use the shared slots again.
    OwnSlots {
        device: String,
        on: bool,
    },
    SetRemap(bool),
    SetSuspended(bool),
    SetPreferredDevice(Option<String>),
    SetAutoSwitch(bool),
    /// What the controllers' player lights and microphone LED show.
    SetLights {
        player: PlayerLights,
        mic: bool,
    },
    /// Ignore a device completely, or use it again.
    SetBlocked {
        id: String,
        blocked: bool,
    },
    /// Replace the device groups.
    SetGroups(Vec<DeviceGroup>),
    /// Turn wireless controllers off after this many minutes without input; 0 never.
    SetPowerOffAfter(u32),
    /// Store how a generic pad model's inputs map to pad controls (replaces its old one).
    SetLayout(GenericLayout),
    /// Go back to the guessed layout for a generic pad model (`vvvv:pppp`).
    ForgetLayout(String),
    /// Turn the DSU motion server on or off, or move it.
    SetDsu(DsuSettings),
    /// Sleep, focus and disconnect behaviour.
    SetRuntimeOptions(RuntimeOptions),
    /// Turn the local HTTP API on or off, or change where it listens.
    SetHttp(HttpSettings),
    /// Give the HTTP API a new token (the old one stops working).
    NewHttpToken,
    /// Turn a wireless controller off now.
    PowerOff(String),
    /// Re-probe ViGEmBus / HidHide now.
    RefreshDrivers,
    /// Look for controllers now, e.g. after another program released them.
    RescanDevices,
    /// The controller hotkeys in effect ([`ServiceReply::Hotkeys`]).
    GetHotkeys,
    /// Replace the controller hotkeys and persist them.
    SetHotkeys(Hotkeys),
    /// Ignore hotkeys while the app records a new one.
    PauseHotkeys(bool),
    /// Send one full in-game turn (`counts` of mouse motion) after a short delay, to check a
    /// profile's turn calibration.
    TestTurn {
        counts: f32,
    },
    /// Measure a controller's gyro bias while it lies still (`None`: the controller in use).
    CalibrateGyro {
        device: Option<String>,
    },
    /// Keep estimating a controller's gyro bias while it rests, or not.
    SetGyroAuto {
        device: Option<String>,
        on: bool,
    },
    /// Forget a controller's gyro bias.
    ResetGyro {
        device: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServiceReply {
    Status(Status),
    Devices(Vec<Device>),
    Ok(String),
    Err(String),
    Hotkeys(Hotkeys),
}

/// The IPC form of a reply.
impl From<ServiceReply> for Response {
    fn from(reply: ServiceReply) -> Response {
        match reply {
            ServiceReply::Status(s) => Response::Status(s),
            ServiceReply::Devices(devices) => Response::Devices { devices },
            other => other.into_result().into(),
        }
    }
}

impl ServiceReply {
    pub fn into_result(self) -> Result<String, String> {
        match self {
            ServiceReply::Ok(m) => Ok(m),
            ServiceReply::Err(m) => Err(m),
            ServiceReply::Status(s) => Ok(format!("{s:?}")),
            ServiceReply::Devices(d) => Ok(format!("{d:?}")),
            ServiceReply::Hotkeys(h) => Ok(format!("{h:?}")),
        }
    }
}
