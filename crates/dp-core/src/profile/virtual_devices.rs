//! The virtual devices Doppelpad drives: sticks, mouse, motion, keyboard and the pad type.

use std::ops::RangeInclusive;

use serde::{Deserialize, Deserializer, Serialize};

use super::is_default;
use crate::pad::Stick;

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

impl VirtualMotion {
    pub const SHAKE_MS: RangeInclusive<u32> = 10..=2000;
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

layer_patch! {
    /// A layer's own settings for one virtual stick; every field left out is the base layer's.
    pub struct VirtualStickPatch for VirtualStick {
        initial: f32,
        max: f32,
        speed: f32,
        acceleration: f32,
        multiplier_x: f32,
        multiplier_y: f32,
        curve: Vec<[f32; 2]>,
        jitter: Jitter,
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
    pub const MS_RANGE: RangeInclusive<u32> = 1..=1000;
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
    DualShock4,
    /// No virtual pad: keyboard/mouse output only.
    None,
}
