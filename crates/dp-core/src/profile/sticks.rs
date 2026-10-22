//! Stick and trigger settings: deadzones, curves, zones, effects and their layer patches.

use std::ops::RangeInclusive;

use serde::{Deserialize, Deserializer, Serialize};

use super::changed;
use crate::pad::{Stick, Trigger};

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
    /// Degrees.
    pub const DIRECTION_WIDTH: RangeInclusive<f32> = 1.0..=44.0;

    /// Tangent of [`direction_width`](Self::direction_width), the ratio the direction
    /// sectors are tested with.
    pub fn direction_ratio(&self) -> f32 {
        let [min, max] = [Self::DIRECTION_WIDTH.start(), Self::DIRECTION_WIDTH.end()];
        self.direction_width.clamp(*min, *max).to_radians().tan()
    }

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

layer_patch! {
    /// A layer's own settings for one stick; every field left out is the base layer's.
    pub struct StickPatch for StickConfig {
        deadzone: f32,
        outer: f32,
        anti_deadzone: f32,
        sensitivity: f32,
        curve: Vec<[f32; 2]>,
        invert_x: bool,
        invert_y: bool,
        rotation: f32,
        zones: StickZones,
        diagonals: bool,
        direction_width: f32,
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
    /// `off` turns the base layer's effect off on this layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<TriggerEffect>,
}

impl TriggerPatch {
    pub fn apply(&self, base: &TriggerConfig) -> TriggerConfig {
        TriggerConfig {
            deadzone: self.deadzone.unwrap_or(base.deadzone),
            zones: self.zones.clone().unwrap_or_else(|| base.zones.clone()),
            full_pull: self.full_pull.unwrap_or(base.full_pull),
            effect: self.effect.or(base.effect),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == TriggerPatch::default()
    }

    /// The patch that turns `base` into `target`.
    pub fn between(base: &TriggerConfig, target: &TriggerConfig) -> TriggerPatch {
        TriggerPatch {
            deadzone: changed(&base.deadzone, &target.deadzone),
            zones: changed(&base.zones, &target.zones),
            full_pull: changed(&base.full_pull, &target.full_pull),
            // An effect taken away is written as `off`, which applies over the base's.
            effect: (base.effect != target.effect)
                .then(|| target.effect.unwrap_or(TriggerEffect::Off)),
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

/// Zone thresholds of a stick or trigger. Each zone reaches from its threshold to the next one;
/// unset `med`/`high` thresholds split the range above `low` into thirds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    default,
    deny_unknown_fields,
    bound(deserialize = "T: Deserialize<'de>, Zones<T>: Default")
)]
pub struct Zones<T> {
    pub low: T,
    /// Start of the medium zone (default: a third of the way from `low` to the end).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub med: Option<T>,
    /// Start of the high zone (default: two thirds of the way from `low` to the end).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<T>,
}

/// Zone thresholds of a stick, up to full deflection; `low` is also where the stick leaves
/// its deadzone for digital mappings.
pub type StickZones = Zones<ZoneRadius>;

impl Default for StickZones {
    fn default() -> Self {
        StickZones {
            low: ZoneRadius::round(0.24),
            med: None,
            high: None,
        }
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
    /// Resistance or vibration the trigger plays against the finger (DualSense).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<TriggerEffect>,
}

/// A DualSense trigger effect. Positions count the trigger's travel in tenths (0 = released).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TriggerEffect {
    /// No resistance.
    Off,
    /// Resistance from `start` (0–9) to the end of the travel, `strength` 1–8.
    Resistance { start: u8, strength: u8 },
    /// A gun trigger: resistance from `start` (2–7) that gives way at `end` (up to 8).
    Weapon { start: u8, end: u8, strength: u8 },
    /// Vibration from `start` (0–9) on, `amplitude` 1–8, `frequency` in Hz.
    Vibration {
        start: u8,
        amplitude: u8,
        frequency: u8,
    },
    /// The eleven bytes the controller takes for one trigger.
    Raw { bytes: [u8; 11] },
}

impl Default for TriggerConfig {
    fn default() -> Self {
        TriggerConfig {
            deadzone: 0.0,
            zones: TriggerZones::default(),
            full_pull: 0.99,
            effect: None,
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

/// Zone thresholds of a trigger as fractions of a pull, up to `full_pull`; `low` is also where
/// the digital `LT`/`RT` press.
pub type TriggerZones = Zones<f32>;

impl Default for TriggerZones {
    fn default() -> Self {
        TriggerZones {
            low: 0.03,
            med: None,
            high: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Profile;

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
}
