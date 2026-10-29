//! The controls of a tick: buttons, stick and trigger zones, touchpad regions and gestures,
//! gyro, keys, mouse buttons, wheel notches and mouse motion.

use std::collections::HashSet;

use super::analog::{
    DIRECTION_SECTOR, directions, ema_alpha, mix, orient, sectors, stick_zone, trigger_zone,
    zone_at,
};
use super::{Engine, gyro, touch};
use crate::control::{Control, Dir, TrackpadPart};
use crate::event::KbmEvent;
use crate::keys::WheelDir;
use crate::pad::{PadButton, PadState, Stick, Trigger};

/// Time constant of the smoothed mouse speed that mouse-motion controls read, in seconds.
const MOUSE_VELOCITY_TAU_S: f32 = 0.03;
/// How long one wheel notch counts as "pressed" when used as a trigger.
const WHEEL_PULSE_MS: u64 = 50;

#[derive(Debug, Default)]
pub(super) struct WheelPulse {
    queued: u32,
    active_until: Option<u64>,
}

impl Engine {
    pub(super) fn apply_kbm(&mut self, events: &[KbmEvent]) {
        // Controls that went down during this tick; if they also go up again before the
        // tick ends, they become a press + release "tap" instead of being lost.
        let mut downs = std::mem::take(&mut self.scratch.downs);
        downs.clear();
        for e in events {
            match *e {
                KbmEvent::Key { key, down } => self.kbm_edge(Control::Key(key), down, &mut downs),
                KbmEvent::MouseButton { button, down } => {
                    self.kbm_edge(Control::Mouse(button), down, &mut downs);
                }
                KbmEvent::MouseMove { dx, dy } => {
                    self.mouse_counts.0 += dx as f32;
                    self.mouse_counts.1 += dy as f32;
                }
                KbmEvent::Wheel { dir, detents } => {
                    self.wheel[dir as usize].queued += detents.round().max(1.0) as u32;
                }
            }
        }
        self.scratch.downs = downs;
        self.taps.sort();
        self.taps.dedup();
    }

    /// A key or mouse button went down or up; `downs` went down during this tick.
    fn kbm_edge(&mut self, c: Control, down: bool, downs: &mut HashSet<Control>) {
        if down {
            self.kbm_down.insert(c);
            downs.insert(c);
        } else {
            self.kbm_down.remove(&c);
            if downs.remove(&c) && !self.prev_controls.contains(&c) {
                self.taps.push(c);
            }
        }
    }

    /// Stick and trigger controls under the current layer's settings.
    pub(super) fn analog_controls(&self, pad: &PadState, out: &mut impl Extend<Control>) {
        for s in Stick::ALL {
            let cfg = self.sticks().get(s);
            let v = orient(pad.stick(s), cfg);
            let Some(zone) = stick_zone(v, cfg.zone_thresholds()) else {
                continue;
            };
            out.extend([Control::StickZone(s, None, zone)]);
            let d = sectors(v, cfg.direction_ratio(), cfg.diagonals);
            for dir in Dir::ALL {
                if d[dir as usize] {
                    out.extend([
                        Control::StickDir(s, dir),
                        Control::StickZone(s, Some(dir), zone),
                    ]);
                }
            }
        }
        for t in Trigger::ALL {
            let cfg = self.triggers().get(t);
            let v = pad.axis(t.axis());
            if let Some(zone) = trigger_zone(v, cfg.zone_thresholds()) {
                out.extend([Control::TriggerPress(t), Control::TriggerZone(t, zone)]);
            }
            if v >= cfg.full_pull {
                out.extend([Control::TriggerFull(t)]);
            }
        }
    }

    /// This tick's controls, added to `set`.
    pub(super) fn derive_controls(&mut self, now: u64, dt: f32, set: &mut HashSet<Control>) {
        if let Some(pad) = &self.pad {
            for b in PadButton::ALL {
                if pad.pressed(b) {
                    set.insert(Control::Button(b));
                }
            }
            self.analog_controls(pad, set);
            let cfg = &self.profile.touchpad;
            let clicked = pad.pressed(PadButton::Touchpad);
            let regions_on = !cfg.click_required || clicked;
            let mut regions = std::mem::take(&mut self.scratch.regions);
            regions.clear();
            if cfg.split {
                for p in pad.touch.into_iter().flatten() {
                    let (half, at) = touch::half_of(p);
                    if regions_on {
                        regions.clear();
                        touch::regions(at, cfg, self.touch_diagonals, &mut regions);
                        set.extend(
                            regions
                                .iter()
                                .map(|r| Control::Trackpad(half, TrackpadPart::Region(*r))),
                        );
                    }
                    if clicked {
                        set.insert(Control::Trackpad(half, TrackpadPart::Click));
                    }
                }
            } else if let Some(p) = pad.touch[0]
                && regions_on
            {
                touch::regions((p.x, p.y), cfg, self.touch_diagonals, &mut regions);
                set.extend(regions.iter().copied().map(Control::Touch));
            }
            self.scratch.regions = regions;
        }
        // Gestures: short presses after the tracker recognises them.
        if let Some(pad) = self.pad {
            let split = self.profile.touchpad.split;
            for c in self.gestures.update(pad.touch, now, split) {
                self.gesture_until.retain(|(g, _)| *g != c);
                self.gesture_until.push((c, now + touch::GESTURE_PRESS_MS));
            }
        }
        self.gesture_until.retain(|(_, until)| now < *until);
        set.extend(self.gesture_until.iter().map(|(c, _)| *c));
        self.gyro_axes = Default::default();
        if let Some(m) = self.pad.and_then(|p| p.motion) {
            // The tilt centre is taken even while the gyro is off.
            let axes = self.gyro.axes(&self.profile.gyro, &m, dt);
            self.gyro_axes = axes;
            if self.gyro_enabled {
                gyro::controls(&self.profile.gyro, axes, set);
            }
        }
        set.extend(self.kbm_down.iter().copied());

        for dir in [
            WheelDir::Up,
            WheelDir::Down,
            WheelDir::Left,
            WheelDir::Right,
        ] {
            let w = &mut self.wheel[dir as usize];
            match w.active_until {
                Some(t) if now < t => {
                    set.insert(Control::Wheel(dir));
                }
                Some(_) => w.active_until = None, // one tick released between notches
                None if w.queued > 0 => {
                    w.queued -= 1;
                    w.active_until = Some(now + WHEEL_PULSE_MS);
                    set.insert(Control::Wheel(dir));
                }
                None => {}
            }
        }

        if dt > 0.0 {
            let v = (self.mouse_counts.0 / dt, self.mouse_counts.1 / dt);
            self.mouse_vel = mix(self.mouse_vel, v, ema_alpha(dt, MOUSE_VELOCITY_TAU_S));
        }
        let (vx, vy) = self.mouse_vel;
        let [low, med, high] = self.profile.mouse.zones;
        // By speed and sector, as for sticks and the touchpad (screen y grows downwards;
        // directions are y up).
        let d = directions((vx, -vy), low, DIRECTION_SECTOR, self.mouse_diagonals);
        let dirs = Dir::ALL.into_iter().filter(|dir| d[*dir as usize]);
        // Zones by speed, in any direction and in each direction it moves.
        let speed = vx.hypot(vy);
        if let Some(z) = zone_at(speed, [low, med, high]) {
            set.insert(Control::MouseZone(None, z));
            set.extend(dirs.clone().map(|d| Control::MouseZone(Some(d), z)));
        }
        set.extend(dirs.map(Control::MouseMove));
    }
}
