//! Controller hotkeys that work independently of the active profile: select a slot, turn
//! remapping on or off, stop macros, emergency stop, show the controller and mappings
//! overlays, drive the app with the controller.
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

/// Writes [`HotkeyAction`], its [`Hotkeys`] entries and their defaults from one table, so
/// a new hotkey cannot miss one of them.
macro_rules! hotkey_table {
    ($(
        $(#[doc = $doc:literal])*
        $variant:ident => $field:ident = [$($button:ident),*];
    )+) => {
        /// What a hotkey does.
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
        pub enum HotkeyAction {
            $( $(#[doc = $doc])* $variant ),+
        }

        impl HotkeyAction {
            /// Every action, in the order of their entries in [`Hotkeys`].
            pub const ALL: [HotkeyAction; [$(stringify!($variant)),+].len()] =
                [$(HotkeyAction::$variant),+];

            /// The name of the action's entry in the hotkey settings (`slot1`, `remap`, …).
            pub const fn setting(self) -> &'static str {
                match self {
                    $( HotkeyAction::$variant => stringify!($field) ),+
                }
            }
        }

        /// The hotkey settings. An empty button list disables that hotkey.
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct Hotkeys {
            /// How long all buttons of a hotkey must be held.
            pub hold_ms: u32,
            $( $(#[doc = $doc])* pub $field: Vec<PadButton>, )+
            /// Confirm each hotkey with a short rumble pattern on the controller.
            pub rumble: bool,
        }

        impl Default for Hotkeys {
            fn default() -> Self {
                #[allow(unused_imports)]
                use PadButton::*;
                Hotkeys {
                    hold_ms: 500,
                    $( $field: vec![$($button),*], )+
                    rumble: false,
                }
            }
        }

        impl Hotkeys {
            /// The buttons of `action`'s hotkey (none when it is off).
            pub fn buttons(&self, action: HotkeyAction) -> &[PadButton] {
                match action {
                    $( HotkeyAction::$variant => &self.$field ),+
                }
            }

            /// Every hotkey off; the hold time and the rumble setting as by default.
            pub fn none() -> Self {
                Hotkeys {
                    $( $field: Vec::new(), )+
                    ..Self::default()
                }
            }
        }
    };
}

hotkey_table! {
    /// Select slot 1 and turn remapping on.
    Slot1 => slot1 = [LB, RB, X];
    /// Select slot 2 and turn remapping on.
    Slot2 => slot2 = [LB, RB, Y];
    /// Select slot 3 and turn remapping on.
    Slot3 => slot3 = [LB, RB, B];
    /// Select slot 4 and turn remapping on.
    Slot4 => slot4 = [LB, RB, A];
    /// Turn remapping off, or on again.
    ToggleRemap => remap = [];
    /// Stop running macros, turbo and toggle latches; remapping continues.
    StopMacros => stop_macros = [];
    /// Stop everything like `stop_macros`, then turn remapping off.
    EmergencyStop => emergency_stop = [Back, Start, LB, RB];
    /// Show or hide the live controller overlay.
    PadOverlay => pad_overlay = [];
    /// Show or hide the overlay listing the current layer's mappings.
    MappingsOverlay => mappings_overlay = [];
    /// Drive the app with the controller (focus, activate, back), or stop.
    AppNavigation => app_navigation = [];
}

impl HotkeyAction {
    /// The slot (0-based) a slot hotkey selects.
    pub const fn slot(self) -> Option<u8> {
        match self {
            HotkeyAction::Slot1 => Some(0),
            HotkeyAction::Slot2 => Some(1),
            HotkeyAction::Slot3 => Some(2),
            HotkeyAction::Slot4 => Some(3),
            _ => None,
        }
    }
}

impl Hotkeys {
    /// Every configured hotkey with its button mask.
    pub fn entries(&self) -> impl Iterator<Item = (HotkeyAction, u32)> + '_ {
        HotkeyAction::ALL
            .into_iter()
            .map(|a| (a, self.buttons(a)))
            .filter(|(_, b)| !b.is_empty())
            .map(|(a, b)| (a, PadButton::mask(b)))
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
        let entries: Vec<_> = self.entries().collect();
        for (i, (a, mask)) in entries.iter().enumerate() {
            let name = a.setting();
            if mask.count_ones() as usize > MAX_BUTTONS {
                out.push(format!("{name}: at most {MAX_BUTTONS} buttons"));
            }
            if entries[..i].iter().any(|(_, m)| m == mask) {
                out.push(format!(
                    "{name}: the same buttons are used by another hotkey"
                ));
            }
        }
        out
    }
}

/// A complete hotkey being held.
#[derive(Clone, Debug)]
struct Held {
    mask: u32,
    since: u64,
    fired: bool,
}

/// Watches the buttons of the controller in use for hotkeys.
#[derive(Clone, Debug, Default)]
pub struct HotkeyDetector {
    held: Vec<Held>,
    withheld: u32,
}

impl HotkeyDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed the current buttons; returns the hotkey that fires now, if any.
    pub fn update(&mut self, hotkeys: &Hotkeys, buttons: u32, now: u64) -> Option<HotkeyAction> {
        let complete = || hotkeys.entries().filter(move |&(_, m)| buttons & m == m);
        // A hotkey inside a larger complete one does not count.
        let active =
            || complete().filter(move |&(_, m)| !complete().any(|(_, o)| o != m && o & m == m));
        self.held.retain(|h| active().any(|(_, m)| m == h.mask));
        let mut fire = None;
        for (action, mask) in active() {
            self.withheld |= mask;
            let i = match self.held.iter().position(|h| h.mask == mask) {
                Some(i) => i,
                None => {
                    self.held.push(Held {
                        mask,
                        since: now,
                        fired: false,
                    });
                    self.held.len() - 1
                }
            };
            let h = &mut self.held[i];
            // One per call: another one due now fires on the next tick.
            if fire.is_none() && !h.fired && now >= h.since + u64::from(hotkeys.hold_ms) {
                h.fired = true;
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
        assert_eq!(d.update(&hk, combo, 510), Some(HotkeyAction::Slot1));
        assert_eq!(d.update(&hk, combo, 2000), None, "only once per hold");
        // Releasing X keeps LB and RB withheld until they are released too.
        assert_eq!(d.update(&hk, mask(&[LB, RB]), 2001), None);
        assert_eq!(d.withheld(), mask(&[LB, RB]));
        assert_eq!(d.update(&hk, 0, 2002), None);
        assert_eq!(d.withheld(), 0);
    }

    #[test]
    fn hotkeys_due_together_fire_one_after_the_other() {
        let hk = Hotkeys {
            slot1: vec![LB, X],
            slot2: vec![RB, Y],
            ..Hotkeys::none()
        };
        let mut d = HotkeyDetector::new();
        let both = mask(&[LB, X, RB, Y]);
        assert_eq!(d.update(&hk, both, 0), None);
        assert_eq!(d.update(&hk, both, 500), Some(HotkeyAction::Slot1));
        assert_eq!(d.update(&hk, both, 501), Some(HotkeyAction::Slot2));
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
        assert_eq!(d.update(&hk, combo, 900), Some(HotkeyAction::Slot2));
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
            Some(HotkeyAction::Slot1)
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
        assert_eq!(
            bad.problems(),
            [
                "hotkey hold time must be 50–2500 ms",
                "remap: the same buttons are used by another hotkey",
                "stop_macros: at most 4 buttons",
            ]
        );
        let off = Hotkeys {
            slot1: Vec::new(),
            ..Default::default()
        };
        assert!(!off.entries().any(|(a, _)| a == HotkeyAction::Slot1));
        assert_eq!(hk.entries().count(), 5, "four slots and the emergency stop");
        assert_eq!(Hotkeys::none().entries().count(), 0);
    }

    #[test]
    fn every_action_names_its_settings_entry() {
        let full = Hotkeys {
            remap: vec![A],
            stop_macros: vec![B],
            pad_overlay: vec![X],
            mappings_overlay: vec![Y],
            app_navigation: vec![Start],
            ..Default::default()
        };
        let table = toml::Table::try_from(&full).unwrap();
        for a in HotkeyAction::ALL {
            let entry = table.get(a.setting()).unwrap_or_else(|| panic!("{a:?}"));
            let names: Vec<&str> = full.buttons(a).iter().map(|b| b.name()).collect();
            assert_eq!(entry, &toml::Value::try_from(names).unwrap(), "{a:?}");
        }
        assert_eq!(
            table.len(),
            HotkeyAction::ALL.len() + 2,
            "one entry per action, plus hold_ms and rumble"
        );
        let slots: Vec<u8> = HotkeyAction::ALL.iter().filter_map(|a| a.slot()).collect();
        assert_eq!(slots, [0, 1, 2, 3]);
    }
}
