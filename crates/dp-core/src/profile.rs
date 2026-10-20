//! The Doppelpad profile model: what the user configures, independent of how it is stored.
//!
//! Profiles are written in TOML (see `profiles/` for examples). JSON profiles
//! (schema 3.0) are imported into this model by `dp-config`.

use serde::{Deserialize, Deserializer, Serialize};

use crate::control::{Control, Dir, TouchGesture};
use crate::keys::{Key, MouseButton, WheelDir};
use crate::pad::{PadButton, Stick, Trigger};

/// One complete remapping profile.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Executable names (e.g. `game.exe`) that activate this profile automatically.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub processes: Vec<String>,
    #[serde(skip_serializing_if = "is_default")]
    pub output: Output,
    #[serde(skip_serializing_if = "is_default")]
    pub timing: Timing,
    #[serde(skip_serializing_if = "is_default")]
    pub sticks: Sticks,
    #[serde(skip_serializing_if = "is_default")]
    pub triggers: Triggers,
    /// How the motion sensors become gyro controls (`[gyro]`).
    #[serde(skip_serializing_if = "is_default")]
    pub gyro: GyroSettings,
    /// How the touchpad becomes region and gesture controls (`[touchpad]`).
    #[serde(skip_serializing_if = "is_default")]
    pub touchpad: TouchpadSettings,
    /// The physical mouse as a source (`[mouse]`).
    #[serde(skip_serializing_if = "is_default")]
    pub mouse: MouseInput,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Layer>,
    /// What held controls do when the layer changes (`[layer_change]`).
    #[serde(skip_serializing_if = "is_default")]
    pub layer_change: LayerChange,
    pub mappings: Vec<Mapping>,
    /// Continuous analog routes (stick → mouse, mouse → stick, gyro → mouse, ...).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub analog: Vec<AnalogMapping>,
    /// Settings of the virtual devices Doppelpad drives (`[virtual.keyboard]`, ...).
    #[serde(rename = "virtual", skip_serializing_if = "is_default")]
    pub virtual_devices: VirtualDevices,
    /// Crosshairs drawn over the game (`[[crosshairs]]`, at most ten).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub crosshairs: Vec<Crosshair>,
    /// Radial menus (`[[radial_menus]]`), each shown while its layer is active.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub radial_menus: Vec<RadialMenu>,
}

/// A ring of sectors over the game. It opens when its layer becomes active; a stick, the
/// mouse or commands point at a sector, and leaving the layer (or the apply command) runs it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RadialMenu {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The menu is open while this layer is active (0: only as a sub-menu).
    pub layer: u8,
    pub select: RadialSelect,
    pub apply: RadialApply,
    /// Show only after the layer has been active this long, so quick presses don't flash it.
    pub delay_ms: u32,
    /// Darken the screen behind the menu this much, 0–1.
    pub tint: f32,
    /// Size, 0.5–2 (1: 400 logical pixels across).
    pub scale: f32,
    /// Pixels from the centre of the primary monitor, right and down.
    pub offset: [i16; 2],
    pub sectors: Vec<RadialSector>,
}

impl Default for RadialMenu {
    fn default() -> Self {
        RadialMenu {
            name: String::new(),
            layer: 1,
            select: RadialSelect::RightStick,
            apply: RadialApply::Release,
            delay_ms: 0,
            tint: 0.0,
            scale: 1.0,
            offset: [0, 0],
            sectors: Vec::new(),
        }
    }
}

/// What points at a sector of an open radial menu.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RadialSelect {
    LeftStick,
    #[default]
    RightStick,
    Mouse,
    /// Only the radial commands (next, previous, up, down, ...).
    Commands,
}

/// When the pointed sector runs.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RadialApply {
    /// When the menu's layer ends (the opening button is let go).
    #[default]
    Release,
    /// Only with the `radial_apply` command.
    Command,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RadialSector {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// An icon name of the app's set, or the path of an image file.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub icon: String,
    /// `#rrggbb`; empty for the accent colour.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub color: String,
    /// What the sector does, played once.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<Action>,
    /// Open this menu (1-based) instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submenu: Option<u8>,
    /// Close the menu after running (with the apply command).
    pub close: bool,
}

impl Default for RadialSector {
    fn default() -> Self {
        RadialSector {
            label: String::new(),
            icon: String::new(),
            color: String::new(),
            actions: Vec::new(),
            submenu: None,
            close: true,
        }
    }
}

/// A crosshair drawn over the game, shown by a command or while one of its layers is active.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Crosshair {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub shape: CrosshairShape,
    /// `#rrggbb`.
    pub color: String,
    /// 0–1.
    pub opacity: f32,
    /// Width and height in logical pixels, 10–200.
    pub size: u16,
    /// Line width in percent of the widest line (a fifth of the size), 10–100.
    pub thickness: u8,
    /// Outline width in pixels, 0 (none) to 10.
    pub outline: u8,
    pub outline_color: String,
    /// Pixels from the centre of the screen, right and down.
    pub offset: [i16; 2],
    /// Draw the crosshair; off, the entry only magnifies.
    pub visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magnifier: Option<Magnifier>,
    /// Shown while one of these layers is active.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<u8>,
}

impl Default for Crosshair {
    fn default() -> Self {
        Crosshair {
            name: String::new(),
            shape: CrosshairShape::Cross,
            color: "#00ff66".into(),
            opacity: 1.0,
            size: 30,
            thickness: 50,
            outline: 0,
            outline_color: "#000000".into(),
            offset: [0, 0],
            visible: true,
            magnifier: None,
            layers: Vec::new(),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrosshairShape {
    #[default]
    Cross,
    /// A cross with an open centre.
    CrossGap,
    Dot,
    Circle,
    CircleDot,
    CircleCross,
    X,
    T,
    Chevron,
    Square,
    Brackets,
}

/// The screen around the crosshair, enlarged.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Magnifier {
    /// 1–16.
    pub zoom: f32,
    /// Diameter as a fraction of the screen height, 0.1–1.
    pub size: f32,
}

impl Default for Magnifier {
    fn default() -> Self {
        Magnifier {
            zoom: 2.0,
            size: 0.2,
        }
    }
}

/// Settings of the virtual devices.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VirtualDevices {
    #[serde(skip_serializing_if = "is_default")]
    pub keyboard: VirtualKeyboard,
    #[serde(skip_serializing_if = "is_default")]
    pub sticks: VirtualSticks,
    #[serde(skip_serializing_if = "is_default")]
    pub motion: VirtualMotion,
    #[serde(skip_serializing_if = "is_default")]
    pub mouse: VirtualMouse,
}

/// How mappings push the virtual sticks (`[virtual.sticks.left]`, ...).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VirtualSticks {
    pub left: VirtualStick,
    pub right: VirtualStick,
}

impl VirtualSticks {
    pub fn get(&self, s: Stick) -> &VirtualStick {
        match s {
            Stick::Left => &self.left,
            Stick::Right => &self.right,
        }
    }
}

/// The physical mouse as a source of controls.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MouseInput {
    /// Speeds (counts per second) where the low, medium and high `MouseMove:*` zones start;
    /// the low one is also where `MouseMove:Up` and the other directions start.
    pub zones: [f32; 3],
}

impl Default for MouseInput {
    fn default() -> Self {
        MouseInput {
            zones: [200.0, 1000.0, 3000.0],
        }
    }
}

/// How the touchpad becomes region controls.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TouchpadSettings {
    /// The left and right halves are two trackpads (`Trackpad*` controls) instead of one.
    pub split: bool,
    pub zones: TouchZones,
    /// Radial zones: radius of the centre region, as a fraction of half the pad.
    pub centre: f32,
    /// Region controls only while the pad is clicked.
    pub click_required: bool,
}

impl Default for TouchpadSettings {
    fn default() -> Self {
        TouchpadSettings {
            split: false,
            zones: TouchZones::Radial,
            centre: 0.35,
            click_required: false,
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchZones {
    /// Sectors around a centre region.
    #[default]
    Radial,
    /// A 3 × 3 grid of cells.
    Grid,
}

/// How the motion sensors become gyro controls and feed gyro routes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GyroSettings {
    pub mode: GyroMode,
    /// What `GyroLeft`/`GyroRight` follow.
    pub steering: SteeringAxis,
    /// Gyro controls and routes are on when the profile starts (commands switch them).
    pub initially_on: bool,
    /// How gyro routes read turning.
    pub space: GyroSpace,
    /// Where the low, medium and high zones start in motion mode, in degrees per second.
    pub motion_zones: [f32; 3],
    /// Where the low, medium and high zones start in tilt mode, in degrees from the centre.
    pub tilt_zones: [f32; 3],
}

impl Default for GyroSettings {
    fn default() -> Self {
        GyroSettings {
            mode: GyroMode::Motion,
            steering: SteeringAxis::Roll,
            initially_on: true,
            space: GyroSpace::Local,
            motion_zones: [60.0, 250.0, 550.0],
            tilt_zones: [10.0, 22.0, 34.0],
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GyroMode {
    /// Gyro controls fire while the controller turns (rotation rate).
    #[default]
    Motion,
    /// The controller is a stick: controls follow how far it is tilted from its centre pose.
    Tilt,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SteeringAxis {
    /// Rolling the controller (like a steering wheel).
    #[default]
    Roll,
    /// Turning it left or right.
    Yaw,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GyroSpace {
    /// The controller's own axes.
    #[default]
    Local,
    /// Turning around gravity, however the controller is held.
    Player,
}

/// The virtual mouse: how the game turns with it (flick-stick routes, turn actions) and how
/// held wheel actions repeat.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VirtualMouse {
    /// Mouse counts for one full 360° turn in the game.
    pub counts_per_turn: f32,
    /// How long a flick or turn action takes.
    pub turn_ms: u32,
    /// While a mapping holds a wheel action, repeat it this often (0 = once per press).
    pub wheel_repeat_ms: u32,
}

impl Default for VirtualMouse {
    fn default() -> Self {
        VirtualMouse {
            counts_per_turn: 18000.0,
            turn_ms: 100,
            wheel_repeat_ms: 0,
        }
    }
}

/// The virtual controller's motion sensors.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VirtualMotion {
    /// Strength of a shake action, in g.
    pub shake_g: f32,
    /// Length of a shake action.
    pub shake_ms: u32,
}

impl Default for VirtualMotion {
    fn default() -> Self {
        VirtualMotion {
            shake_g: 2.0,
            shake_ms: 150,
        }
    }
}

/// A small offset that keeps moving around the virtual stick's output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Jitter {
    pub enabled: bool,
    pub pattern: JitterPattern,
    /// Smallest and largest offset, as a fraction of full deflection.
    pub min: f32,
    pub max: f32,
    /// Shortest and longest time before the offset moves on.
    pub delay_min_ms: u32,
    pub delay_max_ms: u32,
}

impl Default for Jitter {
    fn default() -> Self {
        Jitter {
            enabled: false,
            pattern: JitterPattern::Circle,
            min: 0.03,
            max: 0.04,
            delay_min_ms: 30,
            delay_max_ms: 40,
        }
    }
}

/// The points a jitter offset moves through.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JitterPattern {
    /// Eight points around a circle.
    #[default]
    Circle,
    /// The four corners of a square.
    Square,
    /// Right, left, up, down.
    Cross,
}

impl JitterPattern {
    /// Unit offsets the pattern steps through, in order.
    pub fn points(self) -> &'static [(f32, f32)] {
        const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
        match self {
            JitterPattern::Circle => &[
                (1.0, 0.0),
                (D, D),
                (0.0, 1.0),
                (-D, D),
                (-1.0, 0.0),
                (-D, -D),
                (0.0, -1.0),
                (D, -D),
            ],
            JitterPattern::Square => &[(1.0, 1.0), (1.0, -1.0), (-1.0, -1.0), (-1.0, 1.0)],
            JitterPattern::Cross => &[(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)],
        }
    }
}

/// One virtual stick: how far and how fast buttons and keys push it, and its output curve.
/// The defaults push at once to the mapped deflection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VirtualStick {
    /// Deflection a button starts at, `[0, 1]`.
    pub initial: f32,
    /// Largest deflection buttons reach, `[0, 1]`.
    pub max: f32,
    /// Growth of the deflection per second while held.
    pub speed: f32,
    /// Growth of that speed per second.
    pub acceleration: f32,
    /// Scale of speed and acceleration on the horizontal axis.
    pub multiplier_x: f32,
    /// Scale of speed and acceleration on the vertical axis.
    pub multiplier_y: f32,
    /// Output curve of the whole virtual stick (`[deflection, output]` points, linear in
    /// between); applies to every source, empty = linear.
    pub curve: Vec<[f32; 2]>,
    /// Small moving offset on top of the output.
    pub jitter: Jitter,
}

impl Default for VirtualStick {
    fn default() -> Self {
        VirtualStick {
            initial: 1.0,
            max: 1.0,
            speed: 0.0,
            acceleration: 0.0,
            multiplier_x: 1.0,
            multiplier_y: 1.0,
            curve: Vec::new(),
            jitter: Jitter::default(),
        }
    }
}

impl VirtualStick {
    /// Deflection a button pushing along one axis has reached after `held_s` seconds.
    pub fn ramp(&self, held_s: f32, multiplier: f32) -> f32 {
        let grown = (self.speed + 0.5 * self.acceleration * held_s) * held_s * multiplier;
        (self.initial + grown).min(self.max).clamp(0.0, 1.0)
    }
}

/// A layer's own settings for one virtual stick; every field left out is the base layer's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VirtualStickPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acceleration: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplier_x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplier_y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curve: Option<Vec<[f32; 2]>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jitter: Option<Jitter>,
}

impl VirtualStickPatch {
    pub fn apply(&self, base: &VirtualStick) -> VirtualStick {
        VirtualStick {
            initial: self.initial.unwrap_or(base.initial),
            max: self.max.unwrap_or(base.max),
            speed: self.speed.unwrap_or(base.speed),
            acceleration: self.acceleration.unwrap_or(base.acceleration),
            multiplier_x: self.multiplier_x.unwrap_or(base.multiplier_x),
            multiplier_y: self.multiplier_y.unwrap_or(base.multiplier_y),
            curve: self.curve.clone().unwrap_or_else(|| base.curve.clone()),
            jitter: self.jitter.clone().unwrap_or_else(|| base.jitter.clone()),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == VirtualStickPatch::default()
    }

    /// The patch that turns `base` into `target`.
    pub fn between(base: &VirtualStick, target: &VirtualStick) -> VirtualStickPatch {
        fn d<T: PartialEq + Clone>(b: &T, t: &T) -> Option<T> {
            (b != t).then(|| t.clone())
        }
        VirtualStickPatch {
            initial: d(&base.initial, &target.initial),
            max: d(&base.max, &target.max),
            speed: d(&base.speed, &target.speed),
            acceleration: d(&base.acceleration, &target.acceleration),
            multiplier_x: d(&base.multiplier_x, &target.multiplier_x),
            multiplier_y: d(&base.multiplier_y, &target.multiplier_y),
            curve: d(&base.curve, &target.curve),
            jitter: d(&base.jitter, &target.jitter),
        }
    }
}

/// A layer's own virtual device settings (`[layers.virtual.sticks.left]`, ...).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayerVirtual {
    #[serde(skip_serializing_if = "is_default")]
    pub sticks: LayerVirtualSticks,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayerVirtualSticks {
    #[serde(skip_serializing_if = "VirtualStickPatch::is_empty")]
    pub left: VirtualStickPatch,
    #[serde(skip_serializing_if = "VirtualStickPatch::is_empty")]
    pub right: VirtualStickPatch,
}

impl LayerVirtualSticks {
    pub fn apply(&self, base: &VirtualSticks) -> VirtualSticks {
        VirtualSticks {
            left: self.left.apply(&base.left),
            right: self.right.apply(&base.right),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VirtualKeyboard {
    /// How keys that mappings hold down repeat.
    pub repeat: KeyRepeat,
}

/// Auto-repeat of held virtual keys (`"native"`, `"fast"`, `"slow"`, `"off"`, or an
/// interval in milliseconds).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum KeyRepeat {
    /// Like a physical keyboard: a delay, then about 30 ms with some jitter.
    #[default]
    Native,
    /// Every 10 ms.
    Fast,
    /// Every 50 ms.
    Slow,
    /// Never: one key-down per press.
    Off,
    /// Every this many milliseconds.
    Every(u32),
}

impl KeyRepeat {
    /// Allowed custom intervals.
    pub const MS_RANGE: std::ops::RangeInclusive<u32> = 1..=1000;
}

impl Serialize for KeyRepeat {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            KeyRepeat::Native => s.serialize_str("native"),
            KeyRepeat::Fast => s.serialize_str("fast"),
            KeyRepeat::Slow => s.serialize_str("slow"),
            KeyRepeat::Off => s.serialize_str("off"),
            KeyRepeat::Every(ms) => s.serialize_u32(*ms),
        }
    }
}

impl<'de> Deserialize<'de> for KeyRepeat {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Name(String),
            Ms(i64),
        }
        match Raw::deserialize(d)? {
            Raw::Name(n) => match n.as_str() {
                "native" => Ok(KeyRepeat::Native),
                "fast" => Ok(KeyRepeat::Fast),
                "slow" => Ok(KeyRepeat::Slow),
                "off" => Ok(KeyRepeat::Off),
                other => Err(serde::de::Error::custom(format!(
                    "unknown key repeat `{other}` (native, fast, slow, off or milliseconds)"
                ))),
            },
            Raw::Ms(ms) => u32::try_from(ms)
                .ok()
                .filter(|ms| KeyRepeat::MS_RANGE.contains(ms))
                .map(KeyRepeat::Every)
                .ok_or_else(|| serde::de::Error::custom("key repeat must be 1–1000 ms")),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VirtualPadType {
    #[default]
    Xbox360,
    #[serde(alias = "ds4")]
    DualShock4,
    /// No virtual pad: keyboard/mouse output only.
    None,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Output {
    pub virtual_pad: VirtualPadType,
    /// Hide the physical controller from games while the profile is active.
    pub hide_physical: bool,
    /// Forward physical inputs that no mapping consumes to the virtual pad.
    pub passthrough: bool,
    /// Swallow keyboard keys and mouse buttons that trigger a mapping, so only the
    /// mapping's output reaches applications. Mappings without actions always mute.
    pub mute_mapped_inputs: bool,
}

impl Default for Output {
    fn default() -> Self {
        Output {
            virtual_pad: VirtualPadType::Xbox360,
            hide_physical: true,
            passthrough: true,
            mute_mapped_inputs: true,
        }
    }
}

/// Profile-wide timing defaults; individual mappings may override them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Timing {
    /// Hold time after which a press counts as a long press.
    pub long_press_ms: u32,
    /// After releasing a first tap, a second press within this time is a double press.
    pub double_tap_ms: u32,
    /// After releasing a first tap, a third press within this time is a triple press.
    pub triple_tap_ms: u32,
    /// Delay between consecutive key events inside macros, and the length of a "tap".
    pub key_delay_ms: u32,
    /// Default turbo period (one press + one release).
    pub turbo_ms: u32,
    /// Window in which the buttons of a combo must all be pressed.
    pub combo_window_ms: u32,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            long_press_ms: 300,
            double_tap_ms: 150,
            triple_tap_ms: 300,
            key_delay_ms: 10,
            turbo_ms: 200,
            combo_window_ms: 50,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sticks {
    pub left: StickConfig,
    pub right: StickConfig,
}

impl Sticks {
    pub fn get(&self, s: Stick) -> &StickConfig {
        match s {
            Stick::Left => &self.left,
            Stick::Right => &self.right,
        }
    }
}

/// Shaping of a physical stick (applied before mappings and passthrough).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StickConfig {
    /// Radial inner deadzone of the stick's output, `[0, 1)`. Zero passes the raw
    /// stick through (the game applies its own deadzone).
    pub deadzone: f32,
    /// Deflection treated as fully pushed, `(deadzone, 1]`.
    pub outer: f32,
    /// Minimum output once outside the deadzone (counteracts in-game deadzones).
    pub anti_deadzone: f32,
    /// Output multiplier (clamped to the unit circle).
    pub sensitivity: f32,
    /// Response curve as `[input, output]` points in `[0, 1]`; empty = linear.
    pub curve: Vec<[f32; 2]>,
    pub invert_x: bool,
    pub invert_y: bool,
    /// Rotation in degrees (positive = counter-clockwise).
    pub rotation: f32,
    /// Where direction controls and the zone controls activate.
    pub zones: StickZones,
    /// Separate diagonal directions (`LeftStickUpLeft`, ...): the stick then points in
    /// exactly one of eight directions; otherwise a diagonal presses both cardinals.
    pub diagonals: bool,
    /// Degrees on each side of an axis that count as that cardinal direction alone,
    /// `(0, 45)`. 22.5 gives eight equal sectors.
    pub direction_width: f32,
}

impl Default for StickConfig {
    fn default() -> Self {
        StickConfig {
            deadzone: 0.0,
            outer: 1.0,
            anti_deadzone: 0.0,
            sensitivity: 1.0,
            curve: Vec::new(),
            invert_x: false,
            invert_y: false,
            rotation: 0.0,
            zones: StickZones::default(),
            diagonals: false,
            direction_width: 22.5,
        }
    }
}

impl StickConfig {
    /// Tangent of [`direction_width`](Self::direction_width), the ratio the direction
    /// sectors are tested with.
    pub fn direction_ratio(&self) -> f32 {
        self.direction_width.clamp(1.0, 44.0).to_radians().tan()
    }
}

/// A layer's own settings for one stick; every field left out is the base layer's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StickPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadzone: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outer: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anti_deadzone: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitivity: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curve: Option<Vec<[f32; 2]>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invert_x: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invert_y: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zones: Option<StickZones>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagonals: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction_width: Option<f32>,
}

impl StickPatch {
    /// The base settings with this patch's fields put over them.
    pub fn apply(&self, base: &StickConfig) -> StickConfig {
        StickConfig {
            deadzone: self.deadzone.unwrap_or(base.deadzone),
            outer: self.outer.unwrap_or(base.outer),
            anti_deadzone: self.anti_deadzone.unwrap_or(base.anti_deadzone),
            sensitivity: self.sensitivity.unwrap_or(base.sensitivity),
            curve: self.curve.clone().unwrap_or_else(|| base.curve.clone()),
            invert_x: self.invert_x.unwrap_or(base.invert_x),
            invert_y: self.invert_y.unwrap_or(base.invert_y),
            rotation: self.rotation.unwrap_or(base.rotation),
            zones: self.zones.clone().unwrap_or_else(|| base.zones.clone()),
            diagonals: self.diagonals.unwrap_or(base.diagonals),
            direction_width: self.direction_width.unwrap_or(base.direction_width),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == StickPatch::default()
    }

    /// The patch that turns `base` into `target` (only the fields that differ).
    pub fn between(base: &StickConfig, target: &StickConfig) -> StickPatch {
        fn d<T: PartialEq + Clone>(b: &T, t: &T) -> Option<T> {
            (b != t).then(|| t.clone())
        }
        StickPatch {
            deadzone: d(&base.deadzone, &target.deadzone),
            outer: d(&base.outer, &target.outer),
            anti_deadzone: d(&base.anti_deadzone, &target.anti_deadzone),
            sensitivity: d(&base.sensitivity, &target.sensitivity),
            curve: d(&base.curve, &target.curve),
            invert_x: d(&base.invert_x, &target.invert_x),
            invert_y: d(&base.invert_y, &target.invert_y),
            rotation: d(&base.rotation, &target.rotation),
            zones: d(&base.zones, &target.zones),
            diagonals: d(&base.diagonals, &target.diagonals),
            direction_width: d(&base.direction_width, &target.direction_width),
        }
    }
}

/// A layer's own settings for one trigger; every field left out is the base layer's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TriggerPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadzone: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zones: Option<TriggerZones>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_pull: Option<f32>,
}

impl TriggerPatch {
    pub fn apply(&self, base: &TriggerConfig) -> TriggerConfig {
        TriggerConfig {
            deadzone: self.deadzone.unwrap_or(base.deadzone),
            zones: self.zones.clone().unwrap_or_else(|| base.zones.clone()),
            full_pull: self.full_pull.unwrap_or(base.full_pull),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == TriggerPatch::default()
    }

    /// The patch that turns `base` into `target`.
    pub fn between(base: &TriggerConfig, target: &TriggerConfig) -> TriggerPatch {
        TriggerPatch {
            deadzone: (base.deadzone != target.deadzone).then_some(target.deadzone),
            zones: (base.zones != target.zones).then(|| target.zones.clone()),
            full_pull: (base.full_pull != target.full_pull).then_some(target.full_pull),
        }
    }
}

/// A layer's own stick settings (`[layers.sticks.left]`, ...).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayerSticks {
    #[serde(skip_serializing_if = "StickPatch::is_empty")]
    pub left: StickPatch,
    #[serde(skip_serializing_if = "StickPatch::is_empty")]
    pub right: StickPatch,
}

impl LayerSticks {
    pub fn get(&self, s: Stick) -> &StickPatch {
        match s {
            Stick::Left => &self.left,
            Stick::Right => &self.right,
        }
    }

    /// The effective settings on the layer, given the base layer's.
    pub fn apply(&self, base: &Sticks) -> Sticks {
        Sticks {
            left: self.left.apply(&base.left),
            right: self.right.apply(&base.right),
        }
    }
}

/// A layer's own trigger settings (`[layers.triggers.left]`, ...).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayerTriggers {
    #[serde(skip_serializing_if = "TriggerPatch::is_empty")]
    pub left: TriggerPatch,
    #[serde(skip_serializing_if = "TriggerPatch::is_empty")]
    pub right: TriggerPatch,
}

impl LayerTriggers {
    pub fn get(&self, t: Trigger) -> &TriggerPatch {
        match t {
            Trigger::Left => &self.left,
            Trigger::Right => &self.right,
        }
    }

    pub fn apply(&self, base: &Triggers) -> Triggers {
        Triggers {
            left: self.left.apply(&base.left),
            right: self.right.apply(&base.right),
        }
    }
}

/// Threshold of a stick zone as a fraction of full deflection: one value for a
/// circle, or `[x, y]` for an ellipse with separate horizontal and vertical radii.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ZoneRadius {
    pub x: f32,
    pub y: f32,
}

impl ZoneRadius {
    pub const fn round(r: f32) -> Self {
        ZoneRadius { x: r, y: r }
    }

    pub fn is_round(&self) -> bool {
        self.x == self.y
    }
}

impl Serialize for ZoneRadius {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.is_round() {
            self.x.serialize(s)
        } else {
            [self.x, self.y].serialize(s)
        }
    }
}

impl<'de> Deserialize<'de> for ZoneRadius {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Round(f32),
            Axes([f32; 2]),
        }
        Ok(match Repr::deserialize(d)? {
            Repr::Round(r) => ZoneRadius::round(r),
            Repr::Axes([x, y]) => ZoneRadius { x, y },
        })
    }
}

/// `low` plus `k` of the way from `low` to `max`; unset zone thresholds split the
/// range above the low threshold into thirds.
fn zone_split(low: f32, max: f32, k: f32) -> f32 {
    low + (max - low) * k
}

/// Zone thresholds of a stick. Each zone reaches from its threshold to the next
/// one; `low` is also where the stick leaves its deadzone for digital mappings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StickZones {
    pub low: ZoneRadius,
    /// Start of the medium zone (default: a third of the way from `low` to full deflection).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub med: Option<ZoneRadius>,
    /// Start of the high zone (default: two thirds of the way from `low` to full deflection).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<ZoneRadius>,
}

impl Default for StickZones {
    fn default() -> Self {
        StickZones {
            low: ZoneRadius::round(0.24),
            med: None,
            high: None,
        }
    }
}

impl StickConfig {
    /// Effective low, medium and high thresholds.
    pub fn zone_thresholds(&self) -> [ZoneRadius; 3] {
        let z = &self.zones;
        let at = |k: f32| ZoneRadius {
            x: zone_split(z.low.x, 1.0, k),
            y: zone_split(z.low.y, 1.0, k),
        };
        [
            z.low,
            z.med.unwrap_or(at(1.0 / 3.0)),
            z.high.unwrap_or(at(2.0 / 3.0)),
        ]
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Triggers {
    pub left: TriggerConfig,
    pub right: TriggerConfig,
}

impl Triggers {
    pub fn get(&self, t: Trigger) -> &TriggerConfig {
        match t {
            Trigger::Left => &self.left,
            Trigger::Right => &self.right,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TriggerConfig {
    /// Output deadzone: pulls below this read as 0 and the rest is rescaled.
    pub deadzone: f32,
    /// Where `LT`/`RT` and the zone controls activate.
    pub zones: TriggerZones,
    /// Pull at or above this reads as 1 and activates `LTFull`/`RTFull`.
    pub full_pull: f32,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        TriggerConfig {
            deadzone: 0.0,
            zones: TriggerZones::default(),
            full_pull: 0.99,
        }
    }
}

impl TriggerConfig {
    /// Effective low, medium and high thresholds.
    pub fn zone_thresholds(&self) -> [f32; 3] {
        let z = &self.zones;
        let at = |k: f32| zone_split(z.low, self.full_pull, k);
        [
            z.low,
            z.med.unwrap_or(at(1.0 / 3.0)),
            z.high.unwrap_or(at(2.0 / 3.0)),
        ]
    }
}

/// Zone thresholds of a trigger as fractions of a full pull. Each zone reaches from
/// its threshold to the next one; `low` is also where the digital `LT`/`RT` press.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TriggerZones {
    pub low: f32,
    /// Start of the medium zone (default: a third of the way from `low` to `full_pull`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub med: Option<f32>,
    /// Start of the high zone (default: two thirds of the way from `low` to `full_pull`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<f32>,
}

impl Default for TriggerZones {
    fn default() -> Self {
        TriggerZones {
            low: 0.03,
            med: None,
            high: None,
        }
    }
}

/// A layer ("shift"). Layer 0 is the implicit base layer and is not listed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Layer {
    pub id: u8,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Mappings of the base layer apply here unless this layer maps the same trigger.
    pub inherit: bool,
    /// Triggers whose base-layer mapping must *not* be inherited (they do nothing here
    /// unless mapped in this layer).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub not_inherited: Vec<Combo>,
    /// Stick settings of this layer; whatever they leave out is the base layer's (stick
    /// and trigger settings always build on the base layer, whatever `inherit` says).
    #[serde(skip_serializing_if = "is_default")]
    pub sticks: LayerSticks,
    #[serde(skip_serializing_if = "is_default")]
    pub triggers: LayerTriggers,
    /// Virtual stick settings of this layer, on top of the base layer's.
    #[serde(rename = "virtual", skip_serializing_if = "is_default")]
    pub virtual_devices: LayerVirtual,
    /// Give the physical keyboard and/or mouse back while this layer is active: their input
    /// is not swallowed and the base layer's keyboard/mouse mappings are not inherited.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlock: Option<Unlock>,
}

/// Which physical inputs an unlock layer gives back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unlock {
    All,
    Keyboard,
    Mouse,
}

impl Unlock {
    /// True if `c` is a physical input this unlock gives back.
    pub fn covers(self, c: Control) -> bool {
        match c {
            Control::Key(_) => self != Unlock::Mouse,
            Control::Mouse(_)
            | Control::Wheel(_)
            | Control::MouseMove(_)
            | Control::MouseZone(..) => self != Unlock::Keyboard,
            _ => false,
        }
    }
}

/// What held controls do when the layer changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayerChange {
    /// Release held controls and press them again under the new layer. Off, they keep
    /// what they do until released and use the new layer from their next press.
    pub repress: bool,
}

impl Default for LayerChange {
    fn default() -> Self {
        LayerChange { repress: true }
    }
}

impl Default for Layer {
    fn default() -> Self {
        Layer {
            id: 1,
            name: String::new(),
            inherit: true,
            not_inherited: Vec::new(),
            sticks: LayerSticks::default(),
            triggers: LayerTriggers::default(),
            virtual_devices: LayerVirtual::default(),
            unlock: None,
        }
    }
}

/// One or more controls that must be held together. Written as a single string
/// (`"A"`) or a list (`["LB", "RB"]`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Combo(pub Vec<Control>);

impl Serialize for Combo {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.0.as_slice() {
            [one] => one.serialize(s),
            many => many.serialize(s),
        }
    }
}

impl Combo {
    pub fn single(c: Control) -> Self {
        Combo(vec![c])
    }

    /// Sorted, deduplicated form used to compare combos.
    pub fn normalized(&self) -> Combo {
        let mut v = self.0.clone();
        v.sort();
        v.dedup();
        Combo(v)
    }
}

impl<'de> Deserialize<'de> for Combo {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            One(Control),
            Many(Vec<Control>),
        }
        match Repr::deserialize(d)? {
            Repr::One(c) => Ok(Combo(vec![c])),
            Repr::Many(v) if v.is_empty() => Err(serde::de::Error::custom("empty trigger combo")),
            Repr::Many(v) => Ok(Combo(v)),
        }
    }
}

impl std::fmt::Display for Combo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, c) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("+")?;
            }
            write!(f, "{c}")?;
        }
        Ok(())
    }
}

/// Which press pattern fires a mapping.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activator {
    /// Ordinary press. Fires immediately unless the same trigger also has a
    /// long/double/triple mapping, in which case it fires once those are ruled out.
    #[default]
    Single,
    Double,
    Triple,
    /// Held for at least `long_press_ms`.
    Long,
    /// Fires on every press, immediately, regardless of other activators.
    Start,
    /// Fires when the trigger is released.
    Release,
}

/// What the actions do while the activator is satisfied.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Outputs are held while the trigger is held (classic remap).
    #[default]
    Hold,
    /// Outputs are pressed and released repeatedly while the trigger is held.
    Turbo,
    /// First activation holds the outputs, the next one releases them.
    Toggle,
    /// The action sequence runs once to completion.
    Once,
    /// The action sequence runs `repeat` times.
    Repeat,
    /// Like `Repeat` but keeps repeating while the trigger is held.
    WhileHeld,
}

/// How a key/button action behaves inside the sequence.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Press {
    /// In `hold`/`toggle`/`turbo` mode: held for as long as the mapping is active.
    /// In sequence modes (`once`/`repeat`): a tap (down, `key_delay_ms`, up).
    #[default]
    Auto,
    /// Down, `key_delay_ms`, up.
    Tap,
    /// Press and keep pressed (until an explicit `up` or the profile is released).
    Down,
    /// Release.
    Up,
}

/// Something a mapping does.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum Action {
    Key {
        key: Key,
        #[serde(default, skip_serializing_if = "is_default")]
        action: Press,
    },
    Pad {
        pad: PadButton,
        #[serde(default, skip_serializing_if = "is_default")]
        action: Press,
    },
    Mouse {
        mouse: MouseButton,
        #[serde(default, skip_serializing_if = "is_default")]
        action: Press,
    },
    Wheel {
        wheel: WheelDir,
        #[serde(default = "one", skip_serializing_if = "is_one")]
        clicks: f32,
    },
    /// Deflect a virtual stick (`x`, `y` in `[-1, 1]`, up positive). Added to the physical
    /// stick, or with `absolute` replacing the axes it names (its non-zero ones; both for a
    /// centred action).
    Stick {
        stick: Stick,
        x: f32,
        y: f32,
        #[serde(default, skip_serializing_if = "is_default")]
        absolute: bool,
        #[serde(default, skip_serializing_if = "is_default")]
        action: Press,
    },
    /// Pull a virtual trigger.
    Trigger {
        trigger: Trigger,
        #[serde(default = "one", skip_serializing_if = "is_one")]
        value: f32,
        #[serde(default, skip_serializing_if = "is_default")]
        action: Press,
    },
    /// Move the mouse continuously in a direction while active (pixels per second).
    MouseMove {
        mouse_move: Dir,
        #[serde(default = "default_mouse_speed")]
        speed: f32,
        /// The speed grows by this much of itself per second held (up to 20 ×).
        #[serde(default, skip_serializing_if = "is_default")]
        acceleration: f32,
    },
    /// Wait inside a sequence.
    Pause { pause: u32 },
    /// Vibrate the physical controller.
    Rumble {
        rumble: u32,
        #[serde(default = "default_rumble")]
        low: f32,
        #[serde(default = "default_rumble")]
        high: f32,
    },
    /// Shake the virtual controller's accelerometer along one axis (`[virtual.motion]`
    /// sets how hard and how long).
    Shake { shake: ShakeAxis },
    /// Turn in the game by this many degrees (positive = right) with mouse motion, over
    /// `[virtual.mouse] turn_ms`.
    Turn { turn: f32 },
    /// Move the cursor to a point of the primary screen (`[x, y]` in 0–1).
    MouseTo { mouse_to: [f32; 2] },
    /// Keep the cursor inside a rectangle of the primary screen (`[x1, y1, x2, y2]` in 0–1)
    /// while the mapping is held.
    MouseClip { mouse_clip: [f32; 4] },
    /// Play a gesture on the virtual controller's touchpad (DualShock 4), over `ms`
    /// (default 250, taps 60).
    Touch {
        touch: TouchGesture,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ms: Option<u32>,
    },
    /// Built-in command.
    Command { command: Command },
    /// Split the sequence into segments (`{ break = true }`): each activation plays the
    /// next segment, starting over after the last.
    Break { r#break: bool },
}

/// Accelerometer axis and the direction a shake starts in (`"+x"`, `"-x"`, ... `"-z"`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShakeAxis {
    #[serde(rename = "+x")]
    PlusX,
    #[serde(rename = "-x")]
    MinusX,
    #[serde(rename = "+y")]
    PlusY,
    #[serde(rename = "-y")]
    MinusY,
    #[serde(rename = "+z")]
    PlusZ,
    #[serde(rename = "-z")]
    MinusZ,
}

impl ShakeAxis {
    pub const ALL: [ShakeAxis; 6] = [
        ShakeAxis::PlusX,
        ShakeAxis::MinusX,
        ShakeAxis::PlusY,
        ShakeAxis::MinusY,
        ShakeAxis::PlusZ,
        ShakeAxis::MinusZ,
    ];

    /// Accelerometer index (x, y, z) and the sign of the first push.
    pub fn axis(self) -> (usize, f32) {
        match self {
            ShakeAxis::PlusX => (0, 1.0),
            ShakeAxis::MinusX => (0, -1.0),
            ShakeAxis::PlusY => (1, 1.0),
            ShakeAxis::MinusY => (1, -1.0),
            ShakeAxis::PlusZ => (2, 1.0),
            ShakeAxis::MinusZ => (2, -1.0),
        }
    }
}

impl Action {
    pub const BREAK: Action = Action::Break { r#break: true };

    pub fn is_break(&self) -> bool {
        matches!(self, Action::Break { .. })
    }
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}
fn one() -> f32 {
    1.0
}
fn is_one(v: &f32) -> bool {
    *v == 1.0
}
fn default_mouse_speed() -> f32 {
    800.0
}
fn default_rumble() -> f32 {
    0.8
}

/// Built-in commands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    /// Stop remapping (release everything, unload the profile).
    RemapOff,
    /// Enable/disable all gyro routes.
    ToggleGyro,
    /// Enable all gyro routes.
    GyroOn,
    /// Disable all gyro routes.
    GyroOff,
    /// Take the controller's current pose as the centre for tilt.
    ResetTilt,
    /// Swap the gyro steering axis between roll and yaw.
    SwitchGyroAxis,
    /// Shut the computer down (applications may still ask to save).
    Shutdown,
    Restart,
    Sleep,
    Hibernate,
    /// End the user's session.
    LogOff,
    /// Save a picture of the screen showing the focused window.
    Screenshot,
    /// Ask the focused window to close.
    CloseWindow,
    /// Show crosshair N (1-based), or hide it if it is the one showing.
    Crosshair(u8),
    /// Hide the crosshair that shows, or bring back the last one.
    ToggleCrosshair,
    /// Open radial menu: point at the next sector clockwise.
    RadialNext,
    RadialPrev,
    /// Point at the sector in that direction.
    RadialUp,
    RadialDown,
    RadialLeft,
    RadialRight,
    /// Run the pointed sector (or open its sub-menu).
    RadialApply,
    /// Back to the parent menu, or close the menu.
    RadialBack,
    /// Launch a program.
    Launch {
        path: String,
        #[serde(default)]
        args: String,
    },
    /// A command from an imported profile that Doppelpad does not implement.
    Unsupported(String),
}

/// How a mapping changes the active layer.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JumpMode {
    /// Layer is active while the trigger is held.
    #[default]
    Hold,
    /// Each activation toggles between the layer and the previous one.
    Toggle,
    /// Switch permanently (until another jump).
    Switch,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Jump {
    pub layer: u8,
    #[serde(default)]
    pub mode: JumpMode,
    /// Wait this long after activation before switching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u32>,
    /// Run the mapping's actions when the jump happens instead of at the press.
    #[serde(default, skip_serializing_if = "is_default")]
    pub postpone: bool,
}

/// One remapping rule: when `trigger` is activated (per `activator`) in `layer`,
/// perform `actions` according to `mode`, and optionally switch layers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    #[serde(default, skip_serializing_if = "is_default")]
    pub layer: u8,
    pub trigger: Combo,
    #[serde(default, skip_serializing_if = "is_default")]
    pub activator: Activator,
    #[serde(default, skip_serializing_if = "is_default")]
    pub mode: Mode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump: Option<Jump>,
    /// Turbo period override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turbo_ms: Option<u32>,
    /// Number of runs for `mode = "repeat"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<u32>,
    /// Pause between runs for `repeat` / `while_held`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat_pause_ms: Option<u32>,
    /// Extra random wait of up to this many ms before each turbo or repeat cycle, so
    /// repeats are less regular.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub random_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_press_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub double_tap_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triple_tap_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_delay_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl Mapping {
    pub fn new(trigger: impl Into<Combo>, actions: Vec<Action>) -> Self {
        Mapping {
            layer: 0,
            trigger: trigger.into(),
            activator: Activator::Single,
            mode: Mode::Hold,
            actions,
            jump: None,
            turbo_ms: None,
            repeat: None,
            repeat_pause_ms: None,
            random_ms: None,
            long_press_ms: None,
            double_tap_ms: None,
            triple_tap_ms: None,
            key_delay_ms: None,
            description: String::new(),
        }
    }
}

impl From<Control> for Combo {
    fn from(c: Control) -> Self {
        Combo::single(c)
    }
}

/// Continuous analog sources.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalogSource {
    LeftStick,
    RightStick,
    /// Physical mouse motion.
    Mouse,
    /// Controller gyroscope (yaw/pitch rate).
    Gyro,
    /// Finger motion on the touchpad.
    Touchpad,
}

/// Continuous analog targets.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalogTarget {
    Mouse,
    LeftStick,
    RightStick,
    /// Flick stick: turn to face where the stick points, then turn with it (horizontal
    /// mouse motion, see `[virtual.mouse]`).
    Flick,
}

/// A continuous route from an analog source to an analog target.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnalogMapping {
    pub layer: u8,
    pub source: AnalogSource,
    pub target: AnalogTarget,
    /// Gain. Units depend on the route: stick→mouse = pixels/s at full deflection ÷ 1000;
    /// mouse→stick = full deflection per 1000 counts/s; gyro→mouse = pixels per degree;
    /// gyro→stick = full deflection per 100 °/s; touchpad→mouse = pixels per pad width ÷ 1000.
    pub sensitivity: f32,
    /// Separate vertical gain multiplier.
    pub vertical_ratio: f32,
    /// Exponent applied to the normalized magnitude (1 = linear).
    pub curve: f32,
    pub invert_x: bool,
    pub invert_y: bool,
    /// Source deadzone for stick sources (fraction of full deflection).
    pub deadzone: f32,
    /// Minimum output deflection for stick targets (counteracts in-game deadzones).
    pub anti_deadzone: f32,
    /// Exponential smoothing time constant.
    pub smoothing_ms: u32,
    /// Mouse→stick: time without motion before the stick recenters.
    pub decay_ms: u32,
    /// Touchpad→mouse: friction of the glide after lift-off (0 = longest glide, 1 = no glide).
    pub trackball_friction: f32,
    /// Route is active only while all of these controls are held (e.g. gyro aim on `LT`).
    pub while_held: Vec<Control>,
    /// Consume the source (e.g. stop the stick from also reaching the virtual pad).
    pub consume: bool,
    /// Mouse → stick: the stick returns to the centre when the mouse stops (spring); off, it
    /// keeps its position like a joystick the mouse pushes around.
    pub spring: bool,
    /// Mouse → stick: motion under this many counts per tick is ignored.
    pub noise_filter: f32,
    /// Stick targets: deflections under this read as 0.
    pub snap: f32,
    /// Stick → mouse: at 90 % deflection or more the speed grows by this much of itself per
    /// second, up to 4 ×.
    pub acceleration: f32,
}

impl Default for AnalogMapping {
    fn default() -> Self {
        AnalogMapping {
            layer: 0,
            source: AnalogSource::RightStick,
            target: AnalogTarget::Mouse,
            sensitivity: 1.0,
            vertical_ratio: 1.0,
            curve: 1.0,
            invert_x: false,
            invert_y: false,
            deadzone: 0.1,
            anti_deadzone: 0.0,
            smoothing_ms: 0,
            decay_ms: 40,
            trackball_friction: 0.0,
            while_held: Vec::new(),
            consume: true,
            spring: true,
            noise_filter: 0.0,
            snap: 0.0,
            acceleration: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_roundtrip() {
        let src = r#"
name = "Test"
processes = ["game.exe"]

[output]
virtual_pad = "dual_shock4"

[[layers]]
id = 1
name = "Shift"
not_inherited = ["A", ["LB", "RB"]]

[[mappings]]
trigger = "A"
actions = [{ key = "Space" }]

[[mappings]]
trigger = ["LB", "RB"]
activator = "long"
mode = "once"
actions = [{ key = "Escape", action = "tap" }, { pause = 50 }, { pad = "Start" }]

[[mappings]]
trigger = "Back"
jump = { layer = 1 }

[[mappings]]
layer = 1
trigger = "RT"
mode = "turbo"
turbo_ms = 80
actions = [{ mouse = "left" }, { stick = "left", x = 0.0, y = 1.0 }, { wheel = "up" }]

[[analog]]
source = "mouse"
target = "right_stick"
sensitivity = 2.0
"#;
        let p: Profile = toml::from_str(src).unwrap();
        assert_eq!(p.output.virtual_pad, VirtualPadType::DualShock4);
        assert_eq!(p.mappings.len(), 4);
        assert_eq!(p.mappings[1].trigger.0.len(), 2);
        assert_eq!(
            p.mappings[1].actions[0],
            Action::Key {
                key: Key::Escape,
                action: Press::Tap
            }
        );
        assert_eq!(
            p.mappings[3].actions[2],
            Action::Wheel {
                wheel: WheelDir::Up,
                clicks: 1.0
            }
        );
        assert_eq!(p.layers[0].not_inherited.len(), 2);
        assert_eq!(p.analog[0].source, AnalogSource::Mouse);
        assert!(p.output.hide_physical, "defaults fill in");

        let again: Profile = toml::from_str(&toml::to_string(&p).unwrap()).unwrap();
        assert_eq!(again, p);
    }

    #[test]
    fn zones_in_toml() {
        let src = r#"
[sticks.left]
zones = { low = 0.2, med = [0.4, 0.6] }

[triggers.right]
zones = { low = 0.1, high = 0.8 }
full_pull = 0.95

[[mappings]]
trigger = ["LB", "LeftStickUpHigh"]
actions = [{ key = "LeftShift" }]
"#;
        let p: Profile = toml::from_str(src).unwrap();
        let left = &p.sticks.left;
        assert_eq!(left.zones.low, ZoneRadius::round(0.2));
        assert_eq!(left.zones.med, Some(ZoneRadius { x: 0.4, y: 0.6 }));
        let [_, _, high] = left.zone_thresholds();
        assert!((high.x - (0.2 + 0.8 * 2.0 / 3.0)).abs() < 1e-6, "derived");
        let [low, med, high] = p.triggers.right.zone_thresholds();
        assert_eq!((low, high), (0.1, 0.8));
        assert!(
            (med - (0.1 + 0.85 / 3.0)).abs() < 1e-6,
            "derived from full_pull"
        );
        assert_eq!(p.triggers.left.zones, TriggerZones::default());

        let text = toml::to_string(&p).unwrap();
        assert!(text.contains("med = [0.4, 0.6]"), "{text}");
        assert_eq!(
            text.matches("\nhigh = ").count(),
            1,
            "unset zones stay unset: {text}"
        );
        let again: Profile = toml::from_str(&text).unwrap();
        assert_eq!(again, p);
    }

    #[test]
    fn unknown_field_rejected() {
        let src = "[[mappings]]\ntrigger = \"A\"\nactoins = []\n";
        assert!(toml::from_str::<Profile>(src).is_err());
    }
}
