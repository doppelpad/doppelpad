//! Continuous analog routes (stick → mouse, mouse → stick, gyro, touchpad).

use serde::{Deserialize, Serialize};

use crate::control::Control;
use crate::pad::Stick;

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

impl From<Stick> for AnalogSource {
    fn from(s: Stick) -> Self {
        match s {
            Stick::Left => AnalogSource::LeftStick,
            Stick::Right => AnalogSource::RightStick,
        }
    }
}

impl AnalogSource {
    /// The stick this source reads, if it is one.
    pub fn stick(self) -> Option<Stick> {
        match self {
            AnalogSource::LeftStick => Some(Stick::Left),
            AnalogSource::RightStick => Some(Stick::Right),
            AnalogSource::Mouse | AnalogSource::Gyro | AnalogSource::Touchpad => None,
        }
    }
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

impl AnalogTarget {
    /// The virtual stick this target moves, if it is one.
    pub fn stick(self) -> Option<Stick> {
        match self {
            AnalogTarget::LeftStick => Some(Stick::Left),
            AnalogTarget::RightStick => Some(Stick::Right),
            AnalogTarget::Mouse | AnalogTarget::Flick => None,
        }
    }
}

/// A continuous route from an analog source to an analog target.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnalogMapping {
    pub layer: u8,
    pub source: AnalogSource,
    pub target: AnalogTarget,
    /// Gain; `1.0` is the calibration in the engine's `routes::scale`: stick→mouse 1000 px/s
    /// at full deflection; mouse→stick full deflection at 2000 counts/s (without spring, 500
    /// counts from the centre); gyro→mouse 10 px per degree; gyro→stick full deflection at
    /// 100 °/s; touchpad→mouse 1000 px per pad width. Higher values are proportionally more
    /// sensitive.
    pub sensitivity: f32,
    /// Separate vertical gain multiplier.
    pub vertical_ratio: f32,
    /// Exponent applied to the normalized magnitude (1 = linear).
    pub curve: f32,
    pub invert_x: bool,
    pub invert_y: bool,
    /// Source deadzone for stick sources (fraction of full deflection).
    pub deadzone: f32,
    /// Stick sources: deflection treated as fully pushed, `(deadzone, 1]`.
    pub outer: f32,
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
            outer: 1.0,
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
