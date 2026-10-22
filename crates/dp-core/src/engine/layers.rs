//! Layers: their own stick, trigger and virtual stick settings, and switching between them.

use std::collections::{HashMap, HashSet};

use super::Engine;
use super::compile::MaskId;
use super::vm::ActId;
use crate::control::Control;
use crate::pad::Stick;
use crate::profile::{Profile, Sticks, Triggers, VirtualStick, VirtualSticks};

/// The stick, trigger and virtual stick settings of the layers that change them.
pub(super) struct LayerSettings {
    analog: HashMap<u8, (Sticks, Triggers)>,
    virtual_sticks: HashMap<u8, VirtualSticks>,
}

impl LayerSettings {
    pub(super) fn new(p: &Profile) -> LayerSettings {
        let analog = p
            .layers
            .iter()
            .filter(|l| l.sticks != Default::default() || l.triggers != Default::default())
            .map(|l| {
                let sticks = l.sticks.apply(&p.sticks);
                (l.id, (sticks, l.triggers.apply(&p.triggers)))
            })
            .collect();
        let virtual_sticks = p
            .layers
            .iter()
            .filter(|l| l.virtual_devices.sticks != Default::default())
            .map(|l| {
                (
                    l.id,
                    l.virtual_devices.sticks.apply(&p.virtual_devices.sticks),
                )
            })
            .collect();
        LayerSettings {
            analog,
            virtual_sticks,
        }
    }

    /// Whether `layer` has stick or trigger settings of its own.
    fn changes_analog(&self, layer: u8) -> bool {
        self.analog.contains_key(&layer)
    }

    /// Stick settings of `layer`.
    pub(super) fn sticks<'a>(&'a self, base: &'a Profile, layer: u8) -> &'a Sticks {
        self.analog.get(&layer).map_or(&base.sticks, |(s, _)| s)
    }

    /// Trigger settings of `layer`.
    pub(super) fn triggers<'a>(&'a self, base: &'a Profile, layer: u8) -> &'a Triggers {
        self.analog.get(&layer).map_or(&base.triggers, |(_, t)| t)
    }

    /// Virtual stick settings of `layer`.
    pub(super) fn virtual_stick<'a>(
        &'a self,
        base: &'a Profile,
        layer: u8,
        s: Stick,
    ) -> &'a VirtualStick {
        self.virtual_sticks
            .get(&layer)
            .unwrap_or(&base.virtual_devices.sticks)
            .get(s)
    }
}

impl Engine {
    pub(super) fn switch_layer(&mut self, target: u8, owner: Option<MaskId>, t: u64) {
        if target == self.layer {
            return;
        }
        tracing::debug!(from = self.layer, to = target, "layer switch");
        // Segmented sequences start again from their first segment.
        self.segment_cursor.clear();
        // Toggle latches belong to the old layer; they end in the order they began.
        let mut latched: Vec<ActId> = self.toggles.drain().map(|(_, id)| id).collect();
        latched.sort_unstable();
        for id in latched {
            self.end_toggle(id, t);
        }
        let analog_differs = self.layer_settings.changes_analog(self.layer)
            || self.layer_settings.changes_analog(target);
        self.layer = target;
        self.layer_changed = true;
        // Stick and trigger controls follow the new layer's thresholds at once: what the
        // new layer does not report is released before anything is re-pressed (what it
        // newly reports is pressed on the next tick).
        if analog_differs && let Some(pad) = self.pad {
            let mut now_on = HashSet::new();
            self.analog_controls(&pad, &mut now_on);
            let mut off: Vec<Control> = self
                .cur_controls
                .iter()
                .copied()
                .filter(|c| {
                    matches!(
                        c,
                        Control::StickDir(..)
                            | Control::StickZone(..)
                            | Control::TriggerPress(_)
                            | Control::TriggerZone(..)
                            | Control::TriggerFull(_)
                    ) && !now_on.contains(c)
                })
                .collect();
            off.sort();
            for c in off {
                self.cur_controls.remove(&c);
                self.on_release(c, t);
            }
        }

        if let Some(m) = owner
            && self.mask_down.contains(&m)
        {
            self.jump_owned.insert(m);
            self.reset_rec(m);
        }
        // Everything else that is held is re-pressed under the new layer.
        let mut held: Vec<MaskId> = self
            .mask_down
            .iter()
            .copied()
            .filter(|m| Some(*m) != owner && !self.jump_owned.contains(m))
            .collect();
        held.sort();
        let mut busy: Vec<MaskId> = self
            .recs
            .keys()
            .copied()
            .filter(|m| Some(*m) != owner && !self.jump_owned.contains(m))
            .collect();
        busy.sort_unstable();
        let repress = self.profile.layer_change.repress;
        for m in busy {
            if !repress && held.contains(&m) {
                continue;
            }
            self.drop_mask(m, t);
        }
        if !repress {
            self.stale.extend(held);
            return;
        }
        for m in held {
            if self.table().rec.contains_key(&m) {
                self.mask_down.remove(&m);
                self.mask_press(m, t);
            }
        }
    }
}
