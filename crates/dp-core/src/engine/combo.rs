//! Shortcuts: controls pressed together within the combo window fire their shared mask.

use super::Engine;
use super::compile::MaskId;
use crate::control::Control;

impl Engine {
    fn combo_window(&self) -> u64 {
        u64::from(self.profile.timing.combo_window_ms)
    }

    pub(super) fn on_press(&mut self, c: Control, t: u64) {
        if !self.consumed.contains(&c) && self.table().member_of.contains_key(&c) {
            self.pending.push((c, t));
            let window = self.combo_window();
            self.combo_deadline.get_or_insert(t + window);
            self.try_fire(t, false);
        } else {
            self.plain_press(c, t);
        }
    }

    pub(super) fn on_release(&mut self, c: Control, t: u64) {
        if let Some(i) = self.pending.iter().position(|(p, _)| *p == c) {
            let (_, t0) = self.pending.remove(i);
            if self.pending.is_empty() {
                self.combo_deadline = None;
            }
            self.plain_press(c, t0);
            self.plain_release(c, t);
        } else if self.consumed.remove(&c) {
            let masks = &self.c.masks;
            let released: Vec<MaskId> = self
                .fired
                .iter()
                .copied()
                .filter(|s| masks[*s].contains(&c))
                .collect();
            self.fired.retain(|s| !released.contains(s));
            for s in released {
                self.mask_release(s, t);
            }
        } else {
            self.plain_release(c, t);
        }
    }

    fn try_fire(&mut self, now: u64, window_closed: bool) {
        loop {
            let table = self.c.layer(self.layer);
            let masks = &self.c.masks;
            let ready = |s: MaskId| {
                masks[s]
                    .iter()
                    .all(|c| self.cur_controls.contains(c) && !self.consumed.contains(c))
                    && masks[s]
                        .iter()
                        .any(|c| self.pending.iter().any(|(p, _)| p == c))
            };
            let best = table
                .shortcuts
                .iter()
                .copied()
                .filter(|&s| ready(s))
                .max_by(|&a, &b| masks[a].len().cmp(&masks[b].len()).then(b.cmp(&a)));
            let Some(s) = best else { break };
            if !window_closed && self.combo_deadline.is_some_and(|d| now <= d) {
                let members = &masks[s];
                let superset_possible = table.shortcuts.iter().any(|&o| {
                    masks[o].len() > members.len()
                        && members.iter().all(|c| masks[o].contains(c))
                        && masks[o].iter().any(|c| !self.cur_controls.contains(c))
                });
                if superset_possible {
                    break;
                }
            }
            let members = masks[s].clone();
            for c in members {
                if let Some(i) = self.pending.iter().position(|(p, _)| *p == c) {
                    self.pending.remove(i);
                } else {
                    // Already delivered as a plain press: end that first.
                    self.plain_release(c, now);
                }
                self.consumed.insert(c);
            }
            self.fired.push(s);
            self.mask_press(s, now);
        }
        if self.pending.is_empty() {
            self.combo_deadline = None;
        }
    }

    pub(super) fn expire_combo(&mut self, now: u64) {
        let Some(deadline) = self.combo_deadline else {
            return;
        };
        if now <= deadline {
            return;
        }
        self.try_fire(deadline, true);
        for (c, t0) in std::mem::take(&mut self.pending) {
            self.plain_press(c, t0);
        }
        self.combo_deadline = None;
    }
}
