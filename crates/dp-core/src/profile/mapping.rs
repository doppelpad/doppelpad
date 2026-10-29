//! Mappings: trigger combos, activators, modes, actions, commands and jumps.

use serde::{Deserialize, Deserializer, Serialize};

use super::is_default;
use crate::control::{Control, Dir, TouchGesture};
use crate::keys::{Key, MouseButton, WheelDir};
use crate::pad::{PadButton, Stick, Trigger};

/// One or more controls that must be held together. Written as a single string
/// (`"A"`) or a list (`["LB", "RB"]`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

// A visitor rather than an untagged enum, so a misspelt control is reported by name
// instead of as "data did not match any variant".
impl<'de> Deserialize<'de> for Combo {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct ComboVisitor;

        impl<'de> serde::de::Visitor<'de> for ComboVisitor {
            type Value = Combo;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a control name or a list of control names")
            }

            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Combo, E> {
                s.parse().map(Combo::single).map_err(E::custom)
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Combo, A::Error> {
                let mut controls = Vec::new();
                while let Some(c) = seq.next_element()? {
                    controls.push(c);
                }
                if controls.is_empty() {
                    return Err(serde::de::Error::custom("empty trigger combo"));
                }
                Ok(Combo(controls))
            }
        }

        d.deserialize_any(ComboVisitor)
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
///
/// Written untagged, but read by hand (`ActionFields`): that names the field or value at
/// fault where an untagged derive only says that no variant matched.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Action {
    Key {
        key: Key,
        #[serde(skip_serializing_if = "is_default")]
        action: Press,
    },
    Pad {
        pad: PadButton,
        #[serde(skip_serializing_if = "is_default")]
        action: Press,
    },
    Mouse {
        mouse: MouseButton,
        #[serde(skip_serializing_if = "is_default")]
        action: Press,
    },
    Wheel {
        wheel: WheelDir,
        #[serde(skip_serializing_if = "is_one")]
        clicks: f32,
    },
    /// Deflect a virtual stick (`x`, `y` in `[-1, 1]`, up positive). Added to the physical
    /// stick, or with `absolute` replacing the axes it names (its non-zero ones; both for a
    /// centred action).
    Stick {
        stick: Stick,
        x: f32,
        y: f32,
        #[serde(skip_serializing_if = "is_default")]
        absolute: bool,
        #[serde(skip_serializing_if = "is_default")]
        action: Press,
    },
    /// Pull a virtual trigger.
    Trigger {
        trigger: Trigger,
        #[serde(skip_serializing_if = "is_one")]
        value: f32,
        #[serde(skip_serializing_if = "is_default")]
        action: Press,
    },
    /// Move the mouse continuously in a direction while active (pixels per second).
    MouseMove {
        mouse_move: Dir,
        speed: f32,
        /// The speed grows by this much of itself per second held (up to 20 ×).
        #[serde(skip_serializing_if = "is_default")]
        acceleration: f32,
    },
    /// Wait inside a sequence.
    Pause { pause: u32 },
    /// Vibrate the physical controller.
    Rumble { rumble: u32, low: f32, high: f32 },
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
        #[serde(skip_serializing_if = "Option::is_none")]
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

/// The field that says what an action is, with the other fields that action takes.
const ACTION_KINDS: &[(&str, &[&str])] = &[
    ("key", &["action"]),
    ("pad", &["action"]),
    ("mouse", &["action"]),
    ("wheel", &["clicks"]),
    ("stick", &["x", "y", "absolute", "action"]),
    ("trigger", &["value", "action"]),
    ("mouse_move", &["speed", "acceleration"]),
    ("pause", &[]),
    ("rumble", &["low", "high"]),
    ("shake", &[]),
    ("turn", &[]),
    ("mouse_to", &[]),
    ("mouse_clip", &[]),
    ("touch", &["ms"]),
    ("command", &[]),
    ("break", &[]),
];

/// `names` as inline code, separated by commas.
fn code_list(names: &[&str]) -> String {
    let names: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
    names.join(", ")
}

/// Declares [`ActionFields`]: every field an action can have, by its name in profiles.
macro_rules! action_fields {
    ($( $field:ident: $ty:ty = $name:literal ),+ $(,)?) => {
        /// The fields of one action as read, before they are checked to make one.
        #[derive(Default)]
        struct ActionFields {
            $( $field: Option<$ty> ),+
        }

        impl ActionFields {
            /// Read the value of the field `name`; false if no action has such a field.
            fn read<'de, A: serde::de::MapAccess<'de>>(
                &mut self,
                name: &str,
                map: &mut A,
            ) -> Result<bool, A::Error> {
                match name {
                    $( $name => self.$field = Some(map.next_value()?), )+
                    _ => return Ok(false),
                }
                Ok(true)
            }

            /// The names of the fields read.
            fn present(&self) -> Vec<&'static str> {
                let mut names = Vec::new();
                $( if self.$field.is_some() { names.push($name); } )+
                names
            }
        }
    };
}

action_fields! {
    key: Key = "key",
    pad: PadName = "pad",
    mouse: MouseButton = "mouse",
    wheel: WheelDir = "wheel",
    clicks: f32 = "clicks",
    stick: Stick = "stick",
    x: f32 = "x",
    y: f32 = "y",
    absolute: bool = "absolute",
    trigger: Trigger = "trigger",
    value: f32 = "value",
    mouse_move: Dir = "mouse_move",
    speed: f32 = "speed",
    acceleration: f32 = "acceleration",
    pause: u32 = "pause",
    rumble: u32 = "rumble",
    low: f32 = "low",
    high: f32 = "high",
    shake: ShakeAxis = "shake",
    turn: f32 = "turn",
    mouse_to: [f32; 2] = "mouse_to",
    mouse_clip: [f32; 4] = "mouse_clip",
    touch: TouchGesture = "touch",
    ms: Option<u32> = "ms",
    command: Command = "command",
    r#break: bool = "break",
    action: Press = "action",
}

impl ActionFields {
    /// The action these fields make: exactly one field of [`ACTION_KINDS`] and only the
    /// fields that kind takes.
    fn build(self) -> Result<Action, String> {
        let present = self.present();
        let kinds: Vec<&str> = present
            .iter()
            .copied()
            .filter(|n| ACTION_KINDS.iter().any(|(k, _)| k == n))
            .collect();
        let all_kinds = || code_list(&ACTION_KINDS.iter().map(|&(k, _)| k).collect::<Vec<_>>());
        let kind = match kinds[..] {
            [kind] => kind,
            [] if present.is_empty() => {
                return Err(format!("an empty action; it needs one of {}", all_kinds()));
            }
            [] => {
                return Err(format!(
                    "an action needs one of {}; this one has only {}",
                    all_kinds(),
                    code_list(&present)
                ));
            }
            [a, b, ..] => {
                return Err(format!(
                    "`{a}` and `{b}` are two actions; put each in its own {{ }}"
                ));
            }
        };
        let takes = ACTION_KINDS
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(&[][..], |&(_, t)| t);
        if let Some(extra) = present.iter().find(|&&n| n != kind && !takes.contains(&n)) {
            return Err(if takes.is_empty() {
                format!("`{extra}` does not go with `{kind}`, which takes nothing else")
            } else {
                format!(
                    "`{extra}` does not go with `{kind}`, which takes {}",
                    code_list(takes)
                )
            });
        }
        let action = self.action.unwrap_or_default();
        Ok(if let Some(key) = self.key {
            Action::Key { key, action }
        } else if let Some(PadName(pad)) = self.pad {
            Action::Pad { pad, action }
        } else if let Some(mouse) = self.mouse {
            Action::Mouse { mouse, action }
        } else if let Some(wheel) = self.wheel {
            Action::Wheel {
                wheel,
                clicks: self.clicks.unwrap_or_else(one),
            }
        } else if let Some(stick) = self.stick {
            let (Some(x), Some(y)) = (self.x, self.y) else {
                return Err(
                    "a stick action needs `x` and `y` (up is +y), such as { stick = \"left\", x = 0.0, y = 1.0 }"
                        .into(),
                );
            };
            Action::Stick {
                stick,
                x,
                y,
                absolute: self.absolute.unwrap_or_default(),
                action,
            }
        } else if let Some(trigger) = self.trigger {
            Action::Trigger {
                trigger,
                value: self.value.unwrap_or_else(one),
                action,
            }
        } else if let Some(mouse_move) = self.mouse_move {
            Action::MouseMove {
                mouse_move,
                speed: self.speed.unwrap_or_else(default_mouse_speed),
                acceleration: self.acceleration.unwrap_or_default(),
            }
        } else if let Some(pause) = self.pause {
            Action::Pause { pause }
        } else if let Some(rumble) = self.rumble {
            Action::Rumble {
                rumble,
                low: self.low.unwrap_or_else(default_rumble),
                high: self.high.unwrap_or_else(default_rumble),
            }
        } else if let Some(shake) = self.shake {
            Action::Shake { shake }
        } else if let Some(turn) = self.turn {
            Action::Turn { turn }
        } else if let Some(mouse_to) = self.mouse_to {
            Action::MouseTo { mouse_to }
        } else if let Some(mouse_clip) = self.mouse_clip {
            Action::MouseClip { mouse_clip }
        } else if let Some(touch) = self.touch {
            Action::Touch {
                touch,
                ms: self.ms.flatten(),
            }
        } else if let Some(command) = self.command {
            Action::Command { command }
        } else {
            // `break`, the one kind left.
            Action::Break {
                r#break: self.r#break.unwrap_or_default(),
            }
        })
    }
}

impl<'de> Deserialize<'de> for Action {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct ActionVisitor;

        impl<'de> serde::de::Visitor<'de> for ActionVisitor {
            type Value = Action;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an action such as { key = \"Space\" } or { pad = \"A\" }")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Action, A::Error> {
                let mut fields = ActionFields::default();
                while let Some(name) = map.next_key::<String>()? {
                    if !fields.read(&name, &mut map)? {
                        return Err(serde::de::Error::custom(format!(
                            "unknown field `{name}` in an action; an action starts with one of {}, \
                             and settings such as `mode` belong to the mapping, beside `actions`",
                            code_list(&ACTION_KINDS.iter().map(|&(k, _)| k).collect::<Vec<_>>())
                        )));
                    }
                }
                fields.build().map_err(serde::de::Error::custom)
            }
        }

        d.deserialize_map(ActionVisitor)
    }
}

/// The button of a `pad` action, which takes the button names exactly as written.
struct PadName(PadButton);

impl<'de> Deserialize<'de> for PadName {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if let Some(b) = PadButton::ALL.into_iter().find(|b| b.name() == s) {
            return Ok(PadName(b));
        }
        let message = match s.parse::<Control>() {
            Ok(Control::Button(b)) => {
                format!(
                    "`pad` takes the button's own name: write `{}` for `{s}`",
                    b.name()
                )
            }
            Ok(Control::TriggerPress(t) | Control::TriggerFull(t) | Control::TriggerZone(t, _)) => {
                format!(
                    "`{s}` is a trigger, not a button: use {{ trigger = \"{}\" }}",
                    t.name().to_ascii_lowercase()
                )
            }
            _ => format!(
                "unknown button `{s}`; `pad` takes {}",
                code_list(&PadButton::ALL.map(PadButton::name))
            ),
        };
        Err(serde::de::Error::custom(message))
    }
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
    /// Show or hide the live controller overlay.
    PadOverlay,
    /// Show or hide the overlay listing the current layer's mappings.
    MappingsOverlay,
    /// Turn the controller off (wireless controllers only).
    PowerOff,
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
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
            trigger: trigger.into(),
            actions,
            ..Default::default()
        }
    }

    /// The mode the engine plays. A sequence with break steps plays one segment per
    /// activation, so it cannot pulse, latch or repeat: turbo and toggle are held instead,
    /// repeat and while-held play once.
    pub fn effective_mode(&self) -> Mode {
        if !self.actions.iter().any(Action::is_break) {
            return self.mode;
        }
        match self.mode {
            Mode::Turbo | Mode::Toggle => Mode::Hold,
            Mode::Repeat | Mode::WhileHeld => Mode::Once,
            other => other,
        }
    }
}

impl From<Control> for Combo {
    fn from(c: Control) -> Self {
        Combo::single(c)
    }
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

#[cfg(test)]
mod tests {
    use crate::profile::Profile;

    /// Every `Action` variant in its written form. Actions are written untagged and read by
    /// hand, so the text must survive a round trip unchanged.
    #[test]
    fn action_variants_roundtrip() {
        let src = r#"[[mappings]]
trigger = "A"
actions = [
    { key = "Space" },
    { key = "LeftShift", action = "down" },
    { pad = "A" },
    { pad = "Start", action = "tap" },
    { mouse = "left" },
    { mouse = "x2", action = "up" },
    { wheel = "up" },
    { wheel = "down", clicks = 2.5 },
    { stick = "left", x = 0.0, y = 1.0 },
    { stick = "right", x = -0.5, y = 0.0, absolute = true, action = "tap" },
    { trigger = "left" },
    { trigger = "right", value = 0.5, action = "down" },
    { mouse_move = "up", speed = 800.0 },
    { mouse_move = "down_left", speed = 400.0, acceleration = 2.0 },
    { pause = 50 },
    { rumble = 200, low = 0.8, high = 0.8 },
    { rumble = 100, low = 1.0, high = 0.0 },
    { shake = "-z" },
    { turn = 90.0 },
    { mouse_to = [0.5, 0.25] },
    { mouse_clip = [0.0, 0.0, 1.0, 1.0] },
    { touch = "tap" },
    { touch = "swipe_up", ms = 120 },
    { command = "remap_off" },
    { command = { crosshair = 2 } },
    { command = { launch = { path = 'C:\Games\a.exe', args = "-x" } } },
    { command = { unsupported = "macro" } },
    { break = true },
]
"#;
        // How Doppelpad writes them (`dp_config::to_toml` adds only a header).
        let written = r#"name = ""

[[mappings]]
trigger = "A"

[[mappings.actions]]
key = "Space"

[[mappings.actions]]
key = "LeftShift"
action = "down"

[[mappings.actions]]
pad = "A"

[[mappings.actions]]
pad = "Start"
action = "tap"

[[mappings.actions]]
mouse = "left"

[[mappings.actions]]
mouse = "x2"
action = "up"

[[mappings.actions]]
wheel = "up"

[[mappings.actions]]
wheel = "down"
clicks = 2.5

[[mappings.actions]]
stick = "left"
x = 0.0
y = 1.0

[[mappings.actions]]
stick = "right"
x = -0.5
y = 0.0
absolute = true
action = "tap"

[[mappings.actions]]
trigger = "left"

[[mappings.actions]]
trigger = "right"
value = 0.5
action = "down"

[[mappings.actions]]
mouse_move = "up"
speed = 800.0

[[mappings.actions]]
mouse_move = "down_left"
speed = 400.0
acceleration = 2.0

[[mappings.actions]]
pause = 50

[[mappings.actions]]
rumble = 200
low = 0.8
high = 0.8

[[mappings.actions]]
rumble = 100
low = 1.0
high = 0.0

[[mappings.actions]]
shake = "-z"

[[mappings.actions]]
turn = 90.0

[[mappings.actions]]
mouse_to = [0.5, 0.25]

[[mappings.actions]]
mouse_clip = [0.0, 0.0, 1.0, 1.0]

[[mappings.actions]]
touch = "tap"

[[mappings.actions]]
touch = "swipe_up"
ms = 120

[[mappings.actions]]
command = "remap_off"

[[mappings.actions]]

[mappings.actions.command]
crosshair = 2

[[mappings.actions]]

[mappings.actions.command.launch]
path = 'C:\Games\a.exe'
args = "-x"

[[mappings.actions]]

[mappings.actions.command]
unsupported = "macro"

[[mappings.actions]]
break = true
"#;
        let parsed: Profile = toml::from_str(src).unwrap();
        assert_eq!(parsed.mappings[0].actions.len(), 28);
        let text = toml::to_string(&parsed).unwrap();
        assert_eq!(text, written);
        assert_eq!(toml::from_str::<Profile>(&text).unwrap(), parsed);
    }

    /// A mistake in an action is reported by what it is, so that whoever wrote it, a person or
    /// an assistant, can fix it from the message alone.
    #[test]
    fn action_mistakes_are_named() {
        let err = |action: &str| {
            let src = format!("[[mappings]]\ntrigger = \"A\"\nactions = [{action}]\n");
            toml::from_str::<Profile>(&src).unwrap_err().to_string()
        };
        for (action, says) in [
            (
                r#"{ key = "W", mode = "down" }"#,
                "unknown field `mode` in an action",
            ),
            (r#"{ pad = "Cross" }"#, "write `A` for `Cross`"),
            (r#"{ pad = "dpadup" }"#, "write `DpadUp` for `dpadup`"),
            (
                r#"{ pad = "RT" }"#,
                r#"`RT` is a trigger, not a button: use { trigger = "right" }"#,
            ),
            (
                r#"{ pad = "Bogus" }"#,
                "unknown button `Bogus`; `pad` takes `A`",
            ),
            (
                r#"{ mouse = "Left" }"#,
                "unknown variant `Left`, expected one of `left`",
            ),
            (
                r#"{ trigger = "RT" }"#,
                "unknown variant `RT`, expected `left` or `right`",
            ),
            (
                r#"{ key = "Ctrl+C" }"#,
                "unknown key `Ctrl+C` (a key name names one key",
            ),
            (
                r#"{ stick = "right", y = -0.15 }"#,
                "a stick action needs `x` and `y`",
            ),
            (
                r#"{ key = "A", pad = "B" }"#,
                "`key` and `pad` are two actions",
            ),
            (
                r#"{ key = "A", clicks = 2 }"#,
                "`clicks` does not go with `key`, which takes `action`",
            ),
            (
                r#"{ command = "remap_off", action = "tap" }"#,
                "`action` does not go with `command`, which takes nothing else",
            ),
            (r#"{ x = 1.0 }"#, "this one has only `x`"),
            ("{}", "an empty action"),
            (
                r#""Space""#,
                r#"expected an action such as { key = "Space" }"#,
            ),
        ] {
            let message = err(action);
            assert!(message.contains(says), "{action}:\n{message}");
        }
    }

    #[test]
    fn actions_read_from_json_too() {
        let json = r#"{ "mappings": [{ "trigger": "A", "actions": [
            { "touch": "tap", "ms": null },
            { "stick": "left", "x": 0, "y": 1 }
        ] }] }"#;
        let p: Profile = serde_json::from_str(json).unwrap();
        assert_eq!(
            p.mappings[0].actions,
            [
                super::Action::Touch {
                    touch: crate::control::TouchGesture::Tap,
                    ms: None
                },
                super::Action::Stick {
                    stick: crate::pad::Stick::Left,
                    x: 0.0,
                    y: 1.0,
                    absolute: false,
                    action: super::Press::Auto
                },
            ]
        );
    }

    #[test]
    fn misspelt_controls_are_named() {
        let err = |src: &str| toml::from_str::<Profile>(src).unwrap_err().to_string();
        let one = err("[[mappings]]\ntrigger = \"Bogus\"\n");
        assert!(one.contains("unknown control `Bogus`"), "{one}");
        let many = err("[[mappings]]\ntrigger = [\"LB\", \"Bogsu\"]\n");
        assert!(many.contains("unknown control `Bogsu`"), "{many}");
        let empty = err("[[mappings]]\ntrigger = []\n");
        assert!(empty.contains("empty trigger combo"), "{empty}");
    }
}
