//! Short rumble patterns that confirm a controller hotkey on the controller it was pressed on,
//! so the player knows what happened without looking (also in exclusive full-screen games,
//! where on-screen notifications cannot draw). [`Confirm::for_action`] says which pattern
//! each hotkey plays.

use std::collections::HashMap;

use dp_core::hotkeys::HotkeyAction;
use dp_core::pad::Rumble;
use dp_platform::{DeviceId, GamepadBackend};

/// A confirmation: `n` short or long pulses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirm {
    Short(u8),
    Long(u8),
}

impl Confirm {
    /// The pattern for `action`, with remapping on (`remap_on`) or off afterwards: as many
    /// short pulses as the slot number; one long pulse when remapping is (still) on, two
    /// when it went off; one short pulse for the overlay and navigation switches, which
    /// show for themselves.
    pub(crate) fn for_action(action: HotkeyAction, remap_on: bool) -> Confirm {
        let on_off = Confirm::Long(if remap_on { 1 } else { 2 });
        match action {
            HotkeyAction::Slot1
            | HotkeyAction::PadOverlay
            | HotkeyAction::MappingsOverlay
            | HotkeyAction::AppNavigation => Confirm::Short(1),
            HotkeyAction::Slot2 => Confirm::Short(2),
            HotkeyAction::Slot3 => Confirm::Short(3),
            HotkeyAction::Slot4 => Confirm::Short(4),
            HotkeyAction::ToggleRemap | HotkeyAction::StopMacros => on_off,
            HotkeyAction::EmergencyStop => Confirm::Long(2),
        }
    }
}

/// Pulse lengths and the pause between pulses, in ms.
const SHORT_MS: u64 = 70;
const LONG_MS: u64 = 260;
const GAP_MS: u64 = 110;
/// How strongly both motors run during a pulse.
const STRENGTH: f32 = 0.7;

impl Confirm {
    /// The pattern as on-intervals `[from, to)` in ms from its start.
    fn intervals(self) -> Vec<(u64, u64)> {
        let (n, len) = match self {
            Confirm::Short(n) => (n, SHORT_MS),
            Confirm::Long(n) => (n, LONG_MS),
        };
        (0..u64::from(n.max(1)))
            .map(|i| {
                let from = i * (len + GAP_MS);
                (from, from + len)
            })
            .collect()
    }
}

struct Pattern {
    start: u64,
    on: Vec<(u64, u64)>,
    /// What the controller was last told: motors on or off.
    sent: Option<bool>,
}

impl Pattern {
    fn on_at(&self, now: u64) -> bool {
        let t = now.saturating_sub(self.start);
        self.on.iter().any(|&(a, b)| (a..b).contains(&t))
    }

    fn end(&self) -> u64 {
        self.start + self.on.last().map_or(0, |&(_, b)| b)
    }
}

/// The patterns playing, per controller.
#[derive(Default)]
pub(super) struct Pulses {
    playing: HashMap<DeviceId, Pattern>,
}

impl Pulses {
    /// Play `c` on `id` from `now` (replacing a pattern still playing there).
    pub(super) fn start(&mut self, id: &DeviceId, c: Confirm, now: u64) {
        self.playing.insert(
            id.clone(),
            Pattern {
                start: now,
                on: c.intervals(),
                sent: None,
            },
        );
    }

    /// Whether a pattern plays on `id` (sessions then leave its rumble alone).
    pub(super) fn playing(&self, id: &DeviceId) -> bool {
        self.playing.contains_key(id)
    }

    /// Switch the motors at the patterns' edges. Returns the controllers whose pattern ended,
    /// whose sessions must send their own rumble again.
    pub(super) fn drive(&mut self, pads: &mut impl GamepadBackend, now: u64) -> Vec<DeviceId> {
        let mut ended = Vec::new();
        for (id, p) in &mut self.playing {
            let done = now >= p.end();
            let on = !done && p.on_at(now);
            if p.sent != Some(on) {
                let r = if on {
                    Rumble {
                        low: STRENGTH,
                        high: STRENGTH,
                    }
                } else {
                    Rumble::default()
                };
                pads.set_rumble(id, r);
                p.sent = Some(on);
            }
            if done {
                ended.push(id.clone());
            }
        }
        for id in &ended {
            self.playing.remove(id);
        }
        ended
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_count_their_pulses() {
        assert_eq!(
            Confirm::Short(3).intervals(),
            [(0, 70), (180, 250), (360, 430)]
        );
        assert_eq!(Confirm::Long(1).intervals(), [(0, 260)]);
        assert_eq!(Confirm::Long(0).intervals().len(), 1, "at least one pulse");
    }

    #[test]
    fn every_hotkey_has_its_pattern() {
        let on: Vec<Confirm> = HotkeyAction::ALL
            .into_iter()
            .map(|a| Confirm::for_action(a, true))
            .collect();
        use Confirm::*;
        assert_eq!(
            on,
            [
                Short(1),
                Short(2),
                Short(3),
                Short(4),
                Long(1),
                Long(1),
                Long(2),
                Short(1),
                Short(1),
                Short(1)
            ]
        );
        assert_eq!(
            Confirm::for_action(HotkeyAction::ToggleRemap, false),
            Long(2)
        );
    }

    #[test]
    fn a_pattern_switches_at_its_edges_and_ends() {
        let p = Pattern {
            start: 1000,
            on: Confirm::Short(2).intervals(),
            sent: None,
        };
        assert!(p.on_at(1000) && p.on_at(1069));
        assert!(!p.on_at(1070) && !p.on_at(1179));
        assert!(p.on_at(1180));
        assert_eq!(p.end(), 1250);
    }
}
