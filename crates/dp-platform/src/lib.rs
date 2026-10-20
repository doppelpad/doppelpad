//! Operating-system backends for Doppelpad.
//!
//! The mapping engine in `dp-core` is pure and platform-neutral. Everything that
//! touches devices or drivers lives behind the traits in this crate:
//!
//! | Responsibility            | Windows                      | Linux                        | macOS (planned)      |
//! |---------------------------|------------------------------|------------------------------|----------------------|
//! | read physical gamepads    | XInput, raw HID (Sony, Nintendo) | evdev (+ DS4/DS5/Switch motion, touchpad) | GameController.framework |
//! | hide physical gamepads    | HidHide                      | `EVIOCGRAB`                  | IOHID seize          |
//! | create virtual gamepads   | ViGEmBus (X360, DS4)         | uinput (X360)                | -                    |
//! | inject keyboard / mouse   | `SendInput`                  | uinput                       | `CGEventPost`        |
//! | capture keyboard / mouse  | low-level hooks + raw input  | evdev grab + uinput passthrough | `CGEventTap`      |
//! | foreground application    | `GetForegroundWindow`        | X11 / XWayland `_NET_ACTIVE_WINDOW` | `NSWorkspace` |

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub use dp_core::event::KbmEvent;
use dp_core::keys::{Key, MouseButton, WheelDir};
use dp_core::pad::{PadState, Rgb, Rumble};
use serde::{Deserialize, Serialize};

#[cfg(windows)]
pub mod windows;

// Built in tests everywhere so its pure mapping code is unit-tested on every OS.
#[cfg(any(target_os = "linux", test))]
pub mod linux;

pub mod joycon;
pub mod nintendo;
mod null;
pub mod padio;
pub mod sony;
pub use null::NullPlatform;

pub type Result<T, E = PlatformError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("{0} is not installed or not reachable: {1}")]
    DriverMissing(&'static str, String),
    #[error("not supported on this platform: {0}")]
    Unsupported(&'static str),
    #[error("OS error in {context}: {source}")]
    Os {
        context: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Other(String),
}

impl PlatformError {
    pub fn last_os(context: &'static str) -> Self {
        PlatformError::Os {
            context,
            source: std::io::Error::last_os_error(),
        }
    }
}

/// Stable identifier of a physical device, unique while it is connected.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeviceId(pub String);

impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Broad controller family; used for button labels and capability defaults.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadFamily {
    Xbox,
    DualShock4,
    DualSense,
    /// Switch / Switch 2 Pro Controller, Nintendo Switch Online pads and Switch-style
    /// third-party pads.
    SwitchPro,
    /// A single Joy-Con (held sideways) or a combined left + right pair.
    JoyCon,
    GameCube,
    Generic,
}

/// Which half of a Joy-Con pair a device is.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoyConSide {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: DeviceId,
    pub name: String,
    pub family: PadFamily,
    pub vendor_id: u16,
    pub product_id: u16,
    /// Which backend produced it (`xinput`, `hid`, `evdev`, ...).
    pub backend: String,
    /// OS device-instance paths used to hide the device (HidHide blacklist entries on
    /// Windows). A combined Joy-Con pair has one per half.
    pub instance_paths: Vec<String>,
    /// Set for a single Joy-Con, which the backend reports upright; [`joycon`] turns it
    /// sideways or pairs it.
    pub joycon: Option<JoyConSide>,
    pub has_motion: bool,
    pub has_touchpad: bool,
    pub has_rumble: bool,
}

/// One device's latest state.
#[derive(Clone, Debug)]
pub struct PadSnapshot {
    pub info: Arc<DeviceInfo>,
    pub state: PadState,
}

/// Source of physical gamepads.
pub trait GamepadBackend: Send {
    fn name(&self) -> &'static str;
    /// Enumerate connected devices and return their latest state. Must not block for long;
    /// backends with blocking reads run their own threads and return cached state here.
    fn poll(&mut self, out: &mut Vec<PadSnapshot>);
    /// Forward rumble to a physical device (if it supports it).
    fn set_rumble(&mut self, _id: &DeviceId, _rumble: Rumble) {}
    /// Show a light bar colour on a physical device (if it has one); `None` stops setting
    /// it and leaves the light bar as it is.
    fn set_lightbar(&mut self, _id: &DeviceId, _color: Option<Rgb>) {}
    /// Tell the backend which XInput slots / device paths belong to our own virtual pads
    /// so it does not read them back as physical input.
    fn ignore_virtual(&mut self, _ignored: &VirtualIgnoreList) {}
    /// Devices the backend found but cannot use, as user-facing explanations
    /// (e.g. "another program is using the controller").
    fn problems(&self) -> Vec<String> {
        Vec::new()
    }
    /// Look for devices now rather than at the next scheduled scan, and retry the ones
    /// that could not be opened (e.g. because another program has just released them).
    fn rescan(&mut self) {}
}

/// Devices created by Doppelpad itself, which input backends must skip.
#[derive(Clone, Debug, Default)]
pub struct VirtualIgnoreList {
    pub xinput_slots: Vec<u32>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VirtualPadKind {
    Xbox360,
    DualShock4,
}

/// A virtual controller visible to games.
pub trait VirtualPad: Send {
    fn kind(&self) -> VirtualPadKind;
    fn update(&mut self, state: &PadState) -> Result<()>;
    /// Latest rumble request from the game, if it changed since the last call.
    fn take_rumble(&mut self) -> Option<Rumble>;
    /// Latest light bar colour the game set (DualShock 4 only), if it changed since the
    /// last call.
    fn take_lightbar(&mut self) -> Option<Rgb> {
        None
    }
    /// XInput slot the pad occupies (Windows X360 only), so readers can skip it.
    fn xinput_slot(&self) -> Option<u32> {
        None
    }
}

pub trait VirtualPadFactory: Send {
    fn available(&self) -> Result<()>;
    fn create(&mut self, kind: VirtualPadKind) -> Result<Box<dyn VirtualPad>>;
}

/// Synthetic keyboard and mouse output.
pub trait KbmOutput: Send {
    fn key(&mut self, key: Key, down: bool);
    fn mouse_button(&mut self, button: MouseButton, down: bool);
    /// Relative motion in pixels (sub-pixel amounts are accumulated by the implementation).
    fn mouse_move(&mut self, dx: f32, dy: f32);
    /// Wheel motion in detents (fractional amounts are accumulated).
    fn wheel(&mut self, dir: WheelDir, detents: f32);
}

/// Which physical keyboard/mouse inputs should be swallowed (not reach other apps)
/// because the active profile remaps them. Written by the engine, read by capture
/// hooks on their own thread, hence lock-free.
#[derive(Debug, Default)]
pub struct CaptureFilter {
    keys: [AtomicU64; 4],
    mouse_buttons: AtomicU64,
    wheel: AtomicU64,
    pub mouse_move: AtomicBool,
    /// Master switch; when false nothing is suppressed.
    pub enabled: AtomicBool,
}

impl CaptureFilter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn bit(i: usize) -> (usize, u64) {
        (i / 64, 1u64 << (i % 64))
    }

    pub fn clear(&self) {
        for k in &self.keys {
            k.store(0, Ordering::Relaxed);
        }
        self.mouse_buttons.store(0, Ordering::Relaxed);
        self.wheel.store(0, Ordering::Relaxed);
        self.mouse_move.store(false, Ordering::Relaxed);
    }

    pub fn set_key(&self, key: Key, suppress: bool) {
        let (w, b) = Self::bit(key as usize);
        if suppress {
            self.keys[w].fetch_or(b, Ordering::Relaxed);
        } else {
            self.keys[w].fetch_and(!b, Ordering::Relaxed);
        }
    }

    pub fn key(&self, key: Key) -> bool {
        let (w, b) = Self::bit(key as usize);
        self.enabled.load(Ordering::Relaxed) && self.keys[w].load(Ordering::Relaxed) & b != 0
    }

    pub fn set_mouse_button(&self, button: MouseButton, suppress: bool) {
        let b = 1u64 << button as u32;
        if suppress {
            self.mouse_buttons.fetch_or(b, Ordering::Relaxed);
        } else {
            self.mouse_buttons.fetch_and(!b, Ordering::Relaxed);
        }
    }

    pub fn mouse_button(&self, button: MouseButton) -> bool {
        self.enabled.load(Ordering::Relaxed)
            && self.mouse_buttons.load(Ordering::Relaxed) & (1 << button as u32) != 0
    }

    pub fn set_wheel(&self, dir: WheelDir, suppress: bool) {
        let b = 1u64 << dir as u32;
        if suppress {
            self.wheel.fetch_or(b, Ordering::Relaxed);
        } else {
            self.wheel.fetch_and(!b, Ordering::Relaxed);
        }
    }

    pub fn wheel(&self, dir: WheelDir) -> bool {
        self.enabled.load(Ordering::Relaxed)
            && self.wheel.load(Ordering::Relaxed) & (1 << dir as u32) != 0
    }

    pub fn suppress_mouse_move(&self) -> bool {
        self.enabled.load(Ordering::Relaxed) && self.mouse_move.load(Ordering::Relaxed)
    }
}

/// Global keyboard/mouse capture.
pub trait KbmCapture: Send {
    /// Start capturing. Events are delivered on the returned channel; inputs selected
    /// by `filter` are swallowed before other applications see them.
    fn start(
        &mut self,
        filter: Arc<CaptureFilter>,
    ) -> Result<crossbeam_channel::Receiver<KbmEvent>>;
    fn stop(&mut self);
}

/// Hides physical devices from games so they only see the virtual pad.
pub trait DeviceHider: Send {
    fn available(&self) -> Result<()>;
    /// Hide exactly these devices (replacing whatever this hider hid before).
    fn hide(&mut self, instance_paths: &[String]) -> Result<()>;
    /// Undo everything this hider changed.
    fn restore(&mut self) -> Result<()>;
    /// Connected devices the hiding driver hides from Doppelpad itself (set up for another
    /// tool, or left by an older install), as user-facing explanations. Asks the driver,
    /// so call it now and then rather than every tick.
    fn problems(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Executable file name (e.g. `game.exe`) of the application that has focus.
pub trait ForegroundWatcher: Send {
    fn foreground_process(&mut self) -> Option<String>;
}

/// All backends for the current OS.
pub struct Platform {
    pub gamepads: Vec<Box<dyn GamepadBackend>>,
    pub virtual_pads: Box<dyn VirtualPadFactory>,
    pub kbm_out: Box<dyn KbmOutput>,
    pub kbm_capture: Box<dyn KbmCapture>,
    pub hider: Box<dyn DeviceHider>,
    pub foreground: Box<dyn ForegroundWatcher>,
}

impl Platform {
    /// Backends for the OS this binary was built for.
    pub fn native() -> Platform {
        #[cfg(windows)]
        {
            windows::platform()
        }
        #[cfg(target_os = "linux")]
        {
            linux::platform()
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            NullPlatform::platform()
        }
    }
}

/// Operating system name and version for logs and bug reports.
pub fn os_version() -> String {
    #[cfg(windows)]
    {
        windows::os_version()
    }
    #[cfg(not(windows))]
    {
        let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default();
        let name = read("/etc/os-release")
            .lines()
            .find_map(|l| l.strip_prefix("PRETTY_NAME="))
            .map(|v| v.trim_matches('"').to_string())
            .unwrap_or_else(|| std::env::consts::OS.to_string());
        match read("/proc/sys/kernel/osrelease").trim() {
            "" => name,
            kernel => format!("{name} (kernel {kernel})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_names_on_the_wire() {
        let names: Vec<String> = [PadFamily::SwitchPro, PadFamily::JoyCon, PadFamily::GameCube]
            .iter()
            .map(|f| serde_json::to_string(f).unwrap())
            .collect();
        assert_eq!(names, [r#""switch_pro""#, r#""joy_con""#, r#""game_cube""#]);
        assert_eq!(
            serde_json::to_string(&JoyConSide::Left).unwrap(),
            r#""left""#
        );
    }
}
