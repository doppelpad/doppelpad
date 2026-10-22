//! How the game's rumble and the light bar reach the controller.

use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

/// How the game's rumble reaches the controller. Rumble that mappings play is not changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Vibration {
    /// Strength of the left (heavy) motor, 0–1; 0 turns it off.
    pub left: f32,
    /// Strength of the right (light) motor, 0–1.
    pub right: f32,
    /// Give each motor the other one's rumble.
    pub swap: bool,
    /// Shortest gap between two rumble commands, 10–200 ms (the latest value wins).
    pub min_delay_ms: u32,
}

impl Default for Vibration {
    fn default() -> Self {
        Vibration {
            left: 1.0,
            right: 1.0,
            swap: false,
            min_delay_ms: 20,
        }
    }
}

impl Vibration {
    pub const MIN_DELAY_MS: RangeInclusive<u32> = 10..=200;

    /// The game's rumble as the controller should play it.
    pub fn apply(&self, game: crate::pad::Rumble) -> crate::pad::Rumble {
        let (low, high) = if self.swap {
            (game.high, game.low)
        } else {
            (game.low, game.high)
        };
        crate::pad::Rumble {
            low: low * self.left.clamp(0.0, 1.0),
            high: high * self.right.clamp(0.0, 1.0),
        }
    }
}

/// A light-bar setting: a mode, its colours and how fast it animates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Light {
    pub mode: LightMode,
    /// `#rrggbb`, 1–10 colours.
    pub colors: Vec<String>,
    /// Use the next colour every period.
    pub cycle: bool,
    /// One animation cycle, 100–10000 ms.
    pub period_ms: u32,
}

impl Light {
    pub const PERIOD_MS: RangeInclusive<u32> = 100..=10_000;
    pub const MAX_COLORS: usize = 10;
}

impl Default for Light {
    fn default() -> Self {
        Light {
            mode: LightMode::Solid,
            colors: vec!["#14b8a6".into()],
            cycle: false,
            period_ms: 2000,
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightMode {
    /// Dark.
    Off,
    #[default]
    Solid,
    Breath,
    Rainbow,
    Shimmer,
    Heartbeat,
    Strobe,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vibration_scales_and_swaps_the_game_rumble() {
        use crate::pad::Rumble;
        let game = Rumble {
            low: 1.0,
            high: 0.5,
        };
        let v = Vibration {
            left: 0.5,
            right: 0.0,
            swap: true,
            ..Default::default()
        };
        assert_eq!(
            v.apply(game),
            Rumble {
                low: 0.25,
                high: 0.0
            }
        );
        assert_eq!(Vibration::default().apply(game), game);
    }
}
