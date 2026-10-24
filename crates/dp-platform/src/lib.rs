//! Operating-system backends for Doppelpad.
//!
//! The mapping engine in `dp-core` is pure and platform-neutral. Everything that
//! touches devices or drivers lives behind the traits in this crate:
//!
//! | Responsibility            | Windows                      | Linux                        | macOS (planned)      |
//! |---------------------------|------------------------------|------------------------------|----------------------|
//! | read physical gamepads    | XInput, raw HID (Sony, Nintendo, Steam, generic), USB (GameCube adapter, Switch 2) | evdev (+ DS4/DS5/Switch motion, touchpad) | GameController.framework |
//! | hide physical gamepads    | HidHide                      | `EVIOCGRAB`                  | IOHID seize          |
//! | create virtual gamepads   | ViGEmBus (X360, DS4)         | uinput (X360)                | -                    |
//! | inject keyboard / mouse   | `SendInput`                  | uinput                       | `CGEventPost`        |
//! | capture keyboard / mouse  | low-level hooks + raw input  | evdev grab + uinput passthrough | `CGEventTap`      |
//! | foreground application    | `GetForegroundWindow`        | X11 / XWayland `_NET_ACTIVE_WINDOW` | `NSWorkspace` |

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

pub use dp_core::event::KbmEvent;
use dp_core::keys::{Key, MouseButton, WheelDir};
use dp_core::pad::{PadState, Rgb, Rumble};
use dp_core::profile::{KbmSource, TriggerEffect};
use serde::{Deserialize, Serialize};

#[cfg(windows)]
pub mod windows;

// Built in tests everywhere so its pure mapping code is unit-tested on every OS.
#[cfg(any(target_os = "linux", test))]
pub mod linux;

// Built in tests everywhere, as `linux` is.
#[cfg(any(target_os = "macos", test))]
pub mod macos;

pub mod generic;
pub mod groups;
#[cfg(any(target_os = "macos", test))]
pub(crate) mod hiddesc;
#[cfg(any(windows, target_os = "macos", test))]
pub(crate) mod hidhost;
pub mod holders;
pub mod joycon;
pub mod nintendo;
mod null;
pub mod padio;

/// Microsoft's USB vendor id (Xbox controllers).
pub const MICROSOFT_VID: u16 = 0x045E;
/// Where the ViGEmBus driver (virtual pads on Windows) is downloaded.
pub const VIGEM_RELEASES: &str = "https://github.com/nefarius/ViGEmBus/releases";
/// Where the HidHide driver (hiding devices on Windows) is downloaded.
pub const HIDHIDE_RELEASES: &str = "https://github.com/nefarius/HidHide/releases";
#[cfg(any(windows, target_os = "linux", target_os = "macos", test))]
pub(crate) mod remainder;
pub(crate) mod report;
pub mod scan;
pub mod sony;
pub mod steam;
pub mod stores;
pub mod timer;
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

/// A command for a program Doppelpad starts: a `launch` action, the file manager or a
/// system tool. On Linux it leaves out what the AppImage put into Doppelpad's environment.
pub fn host_command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    #[cfg(target_os = "linux")]
    {
        linux::host_command(program)
    }
    #[cfg(not(target_os = "linux"))]
    {
        std::process::Command::new(program)
    }
}

/// Start `cmd` without waiting for it. A thread collects its exit status, or on Linux it
/// would stay behind as a zombie process until Doppelpad ends.
pub fn spawn_detached(cmd: &mut std::process::Command) -> std::io::Result<()> {
    let mut child = cmd.spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// Stable identifier of a physical device, unique while it is connected.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeviceId(pub String);

impl DeviceId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for DeviceId {
    fn from(id: String) -> DeviceId {
        DeviceId(id)
    }
}

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
    /// A virtual controller another program makes for a controller it reads, such as Steam's
    /// on Linux (for Doppelpad's own virtual pad too). It is listed and can be picked, but
    /// is never picked on its own and not announced.
    pub mirror: bool,
}

/// One device's latest state.
#[derive(Clone, Debug)]
pub struct PadSnapshot {
    pub info: Arc<DeviceInfo>,
    pub state: PadState,
    /// `None` when the device does not report one.
    pub battery: Option<Battery>,
    /// The unmapped input of a generic pad (for identifying its buttons).
    pub raw: Option<Arc<generic::RawInput>>,
}

/// Trigger effects, left then right; `None` leaves a trigger as it is.
pub type TriggerEffects = [Option<TriggerEffect>; 2];

/// Indicator lights besides the light bar. `None` leaves a light to the controller.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leds {
    /// Player number to show, 1–4; 0 turns the player lights off.
    pub player: Option<u8>,
    /// The microphone LED (DualSense).
    pub mic: Option<bool>,
}

/// A controller's battery as far as it reports it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Battery {
    /// Charge in percent; controllers that report coarse steps give their step's value.
    pub level: u8,
    pub charging: bool,
    /// Runs on the cable without a battery (wired pads); `level` means nothing then.
    pub wired: bool,
}

impl Battery {
    /// Running on the battery (not charging, not wired).
    pub fn draining(&self) -> bool {
        !self.charging && !self.wired
    }

    /// The emptiest of `batteries` (the first on a tie): controllers used together run out
    /// with their first member.
    pub fn emptiest(batteries: impl IntoIterator<Item = Battery>) -> Option<Battery> {
        batteries.into_iter().min_by_key(|b| b.level)
    }
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
    /// Show player lights and the microphone LED (if the device has them).
    fn set_leds(&mut self, _id: &DeviceId, _leds: Leds) {}
    /// Play trigger effects, left then right (DualSense); `None` leaves a trigger as it is.
    fn set_trigger_effects(&mut self, _id: &DeviceId, _effects: TriggerEffects) {}
    /// Turn a wireless controller off. `false` when this backend cannot (wired, unknown).
    fn power_off(&mut self, _id: &DeviceId) -> bool {
        false
    }
    /// Tell the backend which XInput slots / device paths belong to our own virtual pads
    /// so it does not read them back as physical input.
    fn ignore_virtual(&mut self, _ignored: &VirtualIgnoreList) {}
    /// How generic pads' inputs map to pad controls, by model.
    fn set_layouts(&mut self, _layouts: Arc<Vec<generic::GenericLayout>>) {}
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

impl VirtualPadKind {
    /// The name as profiles and the API write it (`xbox360`, `dual_shock4`).
    pub fn name(self) -> &'static str {
        match self {
            VirtualPadKind::Xbox360 => "xbox360",
            VirtualPadKind::DualShock4 => "dual_shock4",
        }
    }
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
    /// Move the cursor to a point of the primary screen (`x`, `y` in 0–1). Not every platform
    /// can; the default does nothing.
    fn mouse_to(&mut self, _x: f32, _y: f32) {
        tracing::debug!("absolute cursor moves are not supported here");
    }
    /// Keep the cursor inside a rectangle of the primary screen (`[x1, y1, x2, y2]` in 0–1),
    /// or (`None`) let it go. The default does nothing.
    fn clip(&mut self, _rect: Option<[f32; 4]>) {
        tracing::debug!("cursor clipping is not supported here");
    }
}

/// Which physical keyboard/mouse inputs should be swallowed (not reach other apps)
/// because the active profile remaps them. Written by the engine, read by capture
/// hooks on their own thread, hence lock-free.
#[derive(Debug, Default)]
pub struct CaptureFilter {
    keys: [AtomicU64; 4],
    mouse_buttons: AtomicU64,
    wheel: AtomicU64,
    mouse_move: AtomicBool,
    /// Master switch; when false nothing is suppressed.
    enabled: AtomicBool,
    /// Which events are input at all: [`KbmSource`] as a number.
    source: AtomicU8,
}

// One bit per key in `CaptureFilter::keys`.
const _: () = assert!(Key::ALL.len() <= 256);

/// Set or clear bit `i` of a bit set.
fn set_bit(set: &AtomicU64, i: u32, on: bool) {
    let b = 1u64 << i;
    if on {
        set.fetch_or(b, Ordering::Relaxed);
    } else {
        set.fetch_and(!b, Ordering::Relaxed);
    }
}

/// Whether bit `i` of a bit set is set.
fn has_bit(set: &AtomicU64, i: u32) -> bool {
    set.load(Ordering::Relaxed) & (1u64 << i) != 0
}

impl CaptureFilter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// The word of `keys` that holds `key`, and its bit there.
    fn key_bit(&self, key: Key) -> (&AtomicU64, u32) {
        let i = key as usize;
        (&self.keys[i / 64], (i % 64) as u32)
    }

    fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// The master switch; when off nothing is suppressed.
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    /// Which keyboard and mouse events count as input.
    pub fn set_source(&self, source: KbmSource) {
        self.source.store(source as u8, Ordering::Relaxed);
    }

    /// Whether an event goes to the mappings (and may be suppressed); `injected` when another
    /// program sent it.
    pub fn accepts(&self, injected: bool) -> bool {
        match self.source.load(Ordering::Relaxed) {
            s if s == KbmSource::Physical as u8 => !injected,
            s if s == KbmSource::Scripts as u8 => injected,
            _ => true,
        }
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
        let (word, bit) = self.key_bit(key);
        set_bit(word, bit, suppress);
    }

    pub fn key(&self, key: Key) -> bool {
        let (word, bit) = self.key_bit(key);
        self.enabled() && has_bit(word, bit)
    }

    pub fn set_mouse_button(&self, button: MouseButton, suppress: bool) {
        set_bit(&self.mouse_buttons, button as u32, suppress);
    }

    pub fn mouse_button(&self, button: MouseButton) -> bool {
        self.enabled() && has_bit(&self.mouse_buttons, button as u32)
    }

    pub fn set_wheel(&self, dir: WheelDir, suppress: bool) {
        set_bit(&self.wheel, dir as u32, suppress);
    }

    pub fn wheel(&self, dir: WheelDir) -> bool {
        self.enabled() && has_bit(&self.wheel, dir as u32)
    }

    pub fn set_mouse_move(&self, suppress: bool) {
        self.mouse_move.store(suppress, Ordering::Relaxed);
    }

    pub fn suppress_mouse_move(&self) -> bool {
        self.enabled() && self.mouse_move.load(Ordering::Relaxed)
    }

    /// Whether `event` is swallowed rather than passed on to other applications.
    pub fn suppresses(&self, event: &KbmEvent) -> bool {
        match *event {
            KbmEvent::Key { key, .. } => self.key(key),
            KbmEvent::MouseButton { button, .. } => self.mouse_button(button),
            KbmEvent::MouseMove { .. } => self.suppress_mouse_move(),
            KbmEvent::Wheel { dir, .. } => self.wheel(dir),
        }
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
    /// Add the instance paths to hide for `device` to `out`: its own, and whatever else the
    /// platform must hide with it.
    fn expand(&self, device: &DeviceInfo, out: &mut Vec<String>) {
        out.extend(device.instance_paths.iter().cloned());
    }
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
    /// Process id of the application that has focus (for a dialog, of the window that owns
    /// it on Windows; Linux reports the dialog's own process), where the platform can tell.
    fn foreground_pid(&mut self) -> Option<u32> {
        None
    }
}

/// Ending the session or putting the computer to sleep.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PowerAction {
    Shutdown,
    Restart,
    Sleep,
    Hibernate,
    LogOff,
}

/// Desktop actions that mappings can trigger.
pub trait SystemControl: Send {
    /// Applications are asked to close as usual; nothing is forced.
    fn power(&mut self, action: PowerAction) -> Result<()>;
    /// Save a picture of the monitor showing the focused window; returns the file.
    fn screenshot(&mut self) -> Result<std::path::PathBuf>;
    /// Ask the focused window to close, as its close button does.
    fn close_foreground_window(&mut self) -> Result<()>;
    /// Keep the computer from going to sleep while `on` (played-back input is not user
    /// activity to the power manager).
    fn keep_awake(&mut self, _on: bool) {}
    /// Whether the user's own desktop takes input: `false` while the session is locked or a
    /// secure desktop (sign-in, Ctrl+Alt+Del, elevation prompt) shows.
    fn desktop_active(&self) -> bool {
        true
    }
}

/// All backends for the current OS.
/// What a driver probe found (see [`Platform::probe_drivers`]).
#[derive(Debug)]
pub struct DriverReport {
    /// Whether virtual controllers can be created.
    pub virtual_pads: Result<()>,
    /// Whether devices can be hidden.
    pub device_hiding: Result<()>,
    /// Connected devices the hiding driver keeps from Doppelpad, explained for the user.
    pub hiding_problems: Vec<String>,
}

pub struct Platform {
    /// Check the drivers the backends need. It keeps no state, so it can run on a thread of
    /// its own while the backends serve the engine loop.
    pub probe_drivers: fn() -> DriverReport,
    pub gamepads: Vec<Box<dyn GamepadBackend>>,
    pub virtual_pads: Box<dyn VirtualPadFactory>,
    pub kbm_out: Box<dyn KbmOutput>,
    pub kbm_capture: Box<dyn KbmCapture>,
    pub hider: Box<dyn DeviceHider>,
    pub foreground: Box<dyn ForegroundWatcher>,
    pub system: Box<dyn SystemControl>,
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
        #[cfg(target_os = "macos")]
        {
            macos::platform()
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            NullPlatform::platform()
        }
    }
}

/// A program with a visible window, offered when choosing the programs of a game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowedProgram {
    /// Executable file name, e.g. `Game.exe`.
    pub exe: String,
    /// Title of its main window.
    pub title: String,
}

/// The programs that show a window now (Windows only; empty elsewhere for now).
pub fn windowed_programs() -> Vec<WindowedProgram> {
    #[cfg(windows)]
    {
        windows::windowed_programs()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// The games the local stores (Steam, Epic, GOG) have installed (Windows only; none elsewhere
/// for now).
pub fn installed_games() -> Vec<stores::InstalledGame> {
    #[cfg(windows)]
    {
        windows::stores::installed_games()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// The executable file name of the program in front (Windows only; `None` elsewhere for now).
pub fn foreground_program() -> Option<String> {
    #[cfg(windows)]
    {
        windows::foreground_program()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Whether Steam runs: its Steam Input may take a virtual controller and remap it again
/// (Windows only; `false` elsewhere for now).
pub fn steam_running() -> bool {
    #[cfg(windows)]
    {
        windows::program_running("steam.exe")
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Operating system name and version for logs and bug reports.
pub fn os_version() -> String {
    #[cfg(windows)]
    {
        windows::os_version()
    }
    #[cfg(target_os = "linux")]
    {
        linux::os_version()
    }
    #[cfg(target_os = "macos")]
    {
        macos::os_version()
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        std::env::consts::OS.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_pad_names_match_serde() {
        for k in [VirtualPadKind::Xbox360, VirtualPadKind::DualShock4] {
            assert_eq!(serde_json::to_value(k).unwrap(), k.name());
        }
    }

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

    #[test]
    fn input_source_decides_what_counts() {
        let f = CaptureFilter::new();
        assert!(f.accepts(false) && f.accepts(true), "all by default");
        f.set_source(KbmSource::Scripts);
        assert!(f.accepts(true) && !f.accepts(false));
        f.set_source(KbmSource::Physical);
        assert!(!f.accepts(true) && f.accepts(false));
    }
}
