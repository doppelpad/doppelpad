//! The Doppelpad profile model: what the user configures, independent of how it is stored.
//!
//! Profiles are written in TOML (see `profiles/` for examples). JSON profiles
//! (schema 3.0) are imported into this model by `dp-config`.

use serde::{Deserialize, Deserializer, Serialize};

use crate::control::{Control, Dir};
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Layer>,
    pub mappings: Vec<Mapping>,
    /// Continuous analog routes (stick → mouse, mouse → stick, gyro → mouse, ...).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub analog: Vec<AnalogMapping>,
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
}

impl Default for Layer {
    fn default() -> Self {
        Layer {
            id: 1,
            name: String::new(),
            inherit: true,
            not_inherited: Vec::new(),
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
    /// Deflect a virtual stick (`x`, `y` in `[-1, 1]`, up positive).
    Stick {
        stick: Stick,
        x: f32,
        y: f32,
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
    /// Built-in command.
    Command { command: Command },
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
