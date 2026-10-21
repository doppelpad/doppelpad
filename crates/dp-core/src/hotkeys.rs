//! Controller hotkeys that work independently of the active profile: select a slot, turn
//! remapping on or off, stop macros, emergency stop, show the controller and mappings
//! overlays.
//!
//! A hotkey is a set of pad buttons held together for a while. [`HotkeyDetector`] watches
//! the buttons of the controller in use, reports a hotkey once it has been held long
//! enough, and tells which buttons to withhold from the mapping engine so a hotkey does
//! not also reach the game.

use serde::{Deserialize, Serialize};

use crate::pad::PadButton;

/// Allowed hold times in milliseconds.
pub const HOLD_MS_RANGE: std::ops::RangeInclusive<u32> = 50..=2500;
/// Most buttons in one hotkey.
pub const MAX_BUTTONS: usize = 4;

/// What a hotkey does.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyAction {
    /// Select slot `n` (0-based) and turn remapping on.
    Slot(u8),
    /// Turn remapping off, or on again.
    ToggleRemap,
    /// Stop running macros, turbo and toggle latches; remapping continues.
    StopMacros,
    /// Stop everything like [`HotkeyAction::StopMacros`], then turn remapping off.
    EmergencyStop,
    /// Show or hide the live controller overlay.
    PadOverlay,
    /// Show or hide the overlay listing the current layer's mappings.
    MappingsOverlay,
}

/// The hotkey settings. An empty button list disables that hotkey.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Hotkeys {
    /// How long all buttons of a hotkey must be held.
    pub hold_ms: u32,
    pub slot1: Vec<PadButton>,
    pub slot2: Vec<PadButton>,
    pub slot3: Vec<PadButton>,
    pub slot4: Vec<PadButton>,
    pub remap: Vec<PadButton>,
    pub stop_macros: Vec<PadButton>,
    pub emergency_stop: Vec<PadButton>,
    pub pad_overlay: Vec<PadButton>,
    pub mappings_overlay: Vec<PadButton>,
}

impl Default for Hotkeys {
    fn default() -> Self {
        use PadButton::*;
        Hotkeys {
            hold_ms: 500,
            slot1: vec![LB, RB, X],
            slot2: vec![LB, RB, Y],
            slot3: vec![LB, RB, B],
            slot4: vec![LB, RB, A],
            remap: Vec::new(),
            stop_macros: Vec::new(),
            emergency_stop: vec![Back, Start, LB, RB],
            pad_overlay: Vec::new(),
            mappings_overlay: Vec::new(),
        }
    }
}

impl Hotkeys {
    /// Every configured hotkey with its button mask.
    pub fn entries(&self) -> Vec<(HotkeyAction, u32)> {
        let slots = [&self.slot1, &self.slot2, &self.slot3, &self.slot4];
        let mut out: Vec<(HotkeyAction, &Vec<PadButton>)> = slots
            .into_iter()
            .enumerate()
            .map(|(i, b)| (HotkeyAction::Slot(i as u8), b))
            .collect();
        out.push((HotkeyAction::ToggleRemap, &self.remap));
        out.push((HotkeyAction::StopMacros, &self.stop_macros));
        out.push((HotkeyAction::EmergencyStop, &self.emergency_stop));
        out.push((HotkeyAction::PadOverlay, &self.pad_overlay));
        out.push((HotkeyAction::MappingsOverlay, &self.mappings_overlay));
        out.into_iter()
            .filter(|(_, b)| !b.is_empty())
            .map(|(a, b)| (a, PadButton::mask(b)))
            .collect()
    }

    /// Problems with the settings, for the user (empty when they are fine).
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !HOLD_MS_RANGE.contains(&self.hold_ms) {
            out.push(format!(
                "hotkey hold time must be {}–{} ms",
                HOLD_MS_RANGE.start(),
                HOLD_MS_RANGE.end()
            ));
        }
        let entries = self.entries();
        for (i, (a, mask)) in entries.iter().enumerate() {
            if mask.count_ones() as usize > MAX_BUTTONS {
                out.push(format!("{a:?}: at most {MAX_BUTTONS} buttons"));
            }
            if entries[..i].iter().any(|(_, m)| m == mask) {
                out.push(format!(
                    "{a:?}: the same buttons are used by another hotkey"
                ));
            }
        }
        out
    }
}

/// Watches the buttons of the controller in use for hotkeys.
#[derive(Clone, Debug, Default)]
pub struct HotkeyDetector {
    /// Since when each complete hotkey has been held (by mask), and whether it fired.
    held: Vec<(u32, u64, bool)>,
    withheld: u32,
}

impl HotkeyDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed the current buttons; returns the hotkey that fires now, if any.
    pub fn update(&mut self, hotkeys: &Hotkeys, buttons: u32, now: u64) -> Option<HotkeyAction> {
        let entries = hotkeys.entries();
        let complete: Vec<(HotkeyAction, u32)> = entries
            .iter()
            .copied()
            .filter(|(_, m)| buttons & m == *m)
            .collect();
        // A hotkey inside a larger complete one does not count.
        let active: Vec<(HotkeyAction, u32)> = complete
            .iter()
            .copied()
            .filter(|(_, m)| !complete.iter().any(|(_, o)| o != m && o & m == *m))
            .collect();
        self.held
            .retain(|(m, _, _)| active.iter().any(|(_, a)| a == m));
        let mut fire = None;
        for (action, mask) in active {
            self.withheld |= mask;
            let i = match self.held.iter().position(|(m, _, _)| *m == mask) {
                Some(i) => i,
                None => {
                    self.held.push((mask, now, false));
                    self.held.len() - 1
                }
            };
            let (_, since, fired) = &mut self.held[i];
            // One per call: another one due now fires on the next tick.
            if fire.is_none() && !*fired && now >= *since + u64::from(hotkeys.hold_ms) {
                *fired = true;
                fire = Some(action);
            }
        }
        self.withheld &= buttons;
        fire
    }

    /// Buttons the mapping engine must not see (bitset over [`PadButton::bit`]).
    pub fn withheld(&self) -> u32 {
        self.withheld
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PadButton::*;

    fn mask(b: &[PadButton]) -> u32 {
        PadButton::mask(b)
    }

    #[test]
    fn fires_once_after_the_hold_time() {
        let hk = Hotkeys::default();
        let mut d = HotkeyDetector::new();
        let combo = mask(&[LB, RB, X]);
        assert_eq!(d.update(&hk, mask(&[LB]), 0), None);
        assert_eq!(d.withheld(), 0, "a partial press passes through");
        assert_eq!(d.update(&hk, combo, 10), None);
        assert_eq!(d.withheld(), combo, "complete: withheld at once");
        assert_eq!(d.update(&hk, combo, 509), None);
        assert_eq!(d.update(&hk, combo, 510), Some(HotkeyAction::Slot(0)));
        assert_eq!(d.update(&hk, combo, 2000), None, "only once per hold");
        // Releasing X keeps LB and RB withheld until they are released too.
        assert_eq!(d.update(&hk, mask(&[LB, RB]), 2001), None);
        assert_eq!(d.withheld(), mask(&[LB, RB]));
        assert_eq!(d.update(&hk, 0, 2002), None);
        assert_eq!(d.withheld(), 0);
    }

    #[test]
    fn hotkeys_due_together_fire_one_after_the_other() {
        let off = Hotkeys {
            slot1: vec![],
            slot2: vec![],
            slot3: vec![],
            slot4: vec![],
            remap: vec![],
            stop_macros: vec![],
            emergency_stop: vec![],
            pad_overlay: vec![],
            mappings_overlay: vec![],
            ..Default::default()
        };
        let hk = Hotkeys {
            slot1: vec![LB, X],
            slot2: vec![RB, Y],
            ..off
        };
        let mut d = HotkeyDetector::new();
        let both = mask(&[LB, X, RB, Y]);
        assert_eq!(d.update(&hk, both, 0), None);
        assert_eq!(d.update(&hk, both, 500), Some(HotkeyAction::Slot(0)));
        assert_eq!(d.update(&hk, both, 501), Some(HotkeyAction::Slot(1)));
        assert_eq!(d.update(&hk, both, 502), None, "each fires once");
    }

    #[test]
    fn early_release_cancels() {
        let hk = Hotkeys::default();
        let mut d = HotkeyDetector::new();
        let combo = mask(&[LB, RB, Y]);
        d.update(&hk, combo, 0);
        d.update(&hk, mask(&[LB, RB]), 300);
        assert_eq!(d.update(&hk, combo, 400), None, "timer restarted");
        assert_eq!(d.update(&hk, combo, 899), None);
        assert_eq!(d.update(&hk, combo, 900), Some(HotkeyAction::Slot(1)));
    }

    #[test]
    fn larger_combo_wins() {
        let hk = Hotkeys {
            remap: vec![LB, RB],
            ..Default::default()
        };
        let mut d = HotkeyDetector::new();
        d.update(&hk, mask(&[LB, RB]), 0);
        // X joins before the remap hotkey's hold time ran out: slot 1 takes over.
        d.update(&hk, mask(&[LB, RB, X]), 200);
        assert_eq!(
            d.update(&hk, mask(&[LB, RB, X]), 600),
            None,
            "remap does not fire"
        );
        assert_eq!(
            d.update(&hk, mask(&[LB, RB, X]), 700),
            Some(HotkeyAction::Slot(0))
        );
        // On its own, the smaller one works.
        let mut d = HotkeyDetector::new();
        d.update(&hk, mask(&[LB, RB]), 0);
        assert_eq!(
            d.update(&hk, mask(&[LB, RB]), 500),
            Some(HotkeyAction::ToggleRemap)
        );
    }

    #[test]
    fn emergency_stop_default_and_overlap() {
        let hk = Hotkeys::default();
        let mut d = HotkeyDetector::new();
        let stop = mask(&[Back, Start, LB, RB]);
        d.update(&hk, stop, 0);
        assert_eq!(d.update(&hk, stop, 500), Some(HotkeyAction::EmergencyStop));
        // Buttons outside any hotkey are never withheld.
        d.update(&hk, stop | DpadUp.bit(), 600);
        assert_eq!(d.withheld() & DpadUp.bit(), 0);
    }

    #[test]
    fn settings_round_trip_and_problems() {
        let hk = Hotkeys::default();
        let text = toml::to_string(&hk).unwrap();
        assert!(text.contains(r#"slot1 = ["LB", "RB", "X"]"#), "{text}");
        assert_eq!(toml::from_str::<Hotkeys>(&text).unwrap(), hk);
        assert_eq!(
            toml::from_str::<Hotkeys>("hold_ms = 800").unwrap().slot2,
            hk.slot2,
            "missing entries keep their defaults"
        );
        assert!(hk.problems().is_empty());
        let bad = Hotkeys {
            hold_ms: 10,
            remap: vec![LB, RB, X],
            stop_macros: vec![A, B, X, Y, LB],
            ..Default::default()
        };
        let p = bad.problems();
        assert_eq!(p.len(), 3, "{p:?}");
        let off = Hotkeys {
            slot1: Vec::new(),
            ..Default::default()
        };
        assert!(
            !off.entries()
                .iter()
                .any(|(a, _)| *a == HotkeyAction::Slot(0))
        );
    }
}
