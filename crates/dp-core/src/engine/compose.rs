//! The tick's output: digital events, host requests and the virtual pad, built in stages.

use super::analog::{
    apply_curve, orient, secs_since, shape_stick, shape_trigger, take_toward_zero,
};
use super::compile::{AnalogOut, Out, PULSE_PAD_MS};
use super::routes::{RouteInput, RouteOutput, RouteState};
use super::{Engine, Output, gyro, routes, touch};
use crate::control::{Control, Half, TrackpadPart};
use crate::pad::{Motion, PadButton, PadState, Stick, Trigger};
use crate::profile::{AnalogSource, GyroMode, VirtualPadType};

/// Button-driven mouse motion speeds up to at most this many times its speed.
const MOUSE_ACCEL_MAX: f32 = 20.0;
/// A shake flips direction this often.
const SHAKE_FLIP_MS: u64 = 25;

/// What the running actions do to one axis of a virtual stick: the largest push each way
/// (opposite pushes cancel) and whether an absolute push sets the axis, so that neither the
/// physical stick nor routes reach it.
#[derive(Clone, Copy, Debug, Default)]
struct StickPush {
    pos: f32,
    neg: f32,
    absolute: bool,
}

impl StickPush {
    /// The axis after the push, from its value `c` so far.
    fn apply(self, c: f32) -> f32 {
        let base = if self.absolute { 0.0 } else { c };
        let push = if self.pos > 0.0 && self.neg > 0.0 {
            0.0
        } else {
            self.pos - self.neg
        };
        (base + push).clamp(-1.0, 1.0)
    }
}

/// Where a virtual stick's jitter offset is and when it moves on.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct JitterState {
    active: bool,
    point: usize,
    offset: (f32, f32),
    next_at: u64,
}

impl Engine {
    /// The tick's output: digital events and host requests, then the virtual pad built in
    /// stages.
    pub(super) fn compose(&mut self, dt: f32, now: u64) -> Output {
        let mut out = Output {
            layer: self.layer,
            layer_changed: self.layer_changed,
            ..Default::default()
        };
        // A pad button pressed and released within this tick (a key tap that arrived in one
        // batch) is shown for a pulse, or the game would never see it.
        for &(o, down) in &self.outs.events {
            if let Out::Pad(b) = o
                && down
                && !self.outs.counts.contains_key(&o)
            {
                self.pad_pulses.push((b, now + PULSE_PAD_MS));
            }
        }
        self.pad_pulses.retain(|&(_, until)| until > now);
        self.drain_digital(&mut out);
        self.repeat_held_wheels(now);
        out.wheel = std::mem::take(&mut self.outs.wheel);
        out.mouse_to = self.outs.mouse_to.take();
        out.commands = std::mem::take(&mut self.outs.commands);
        out.rumble = self.outs.rumble.filter(|r| r.until > now).map(|r| r.rumble);

        let radial_stick = self.radial_stick();
        let mut pad = self.passthrough(radial_stick);
        // Mapping outputs.
        for o in self.outs.counts.keys() {
            if let Out::Pad(b) = o {
                pad.set(*b, true);
            }
        }
        for &(b, _) in &self.pad_pulses {
            pad.set(b, true);
        }
        let absolute = self.mix_analog_actions(&mut pad, &mut out, dt, now);
        self.spread_turns(&mut out, dt);
        self.run_routes(&mut pad, &mut out, absolute, radial_stick, dt, now);
        self.finish_virtual_sticks(&mut pad, now);
        self.play_touch(&mut pad, now);
        self.play_shakes(&mut pad, now);

        if self.profile.output.virtual_pad != VirtualPadType::None {
            out.pad = Some(pad);
        }
        out
    }

    /// Held wheel actions repeat at the virtual mouse's interval.
    fn repeat_held_wheels(&mut self, now: u64) {
        let every = u64::from(self.profile.virtual_devices.mouse.wheel_repeat_ms);
        for act in &mut self.acts {
            if let Some(w) = &mut act.wheel_held
                && let Some(due) = now.saturating_sub(w.since).checked_div(every)
            {
                while w.sent < due {
                    self.outs.wheel.push((w.dir, w.notches));
                    w.sent += 1;
                }
            }
        }
    }

    /// The physical controls no mapping, route or open menu uses, mirrored.
    fn passthrough(&self, radial_stick: Option<Stick>) -> PadState {
        let mut pad = PadState::default();
        let Some(phys) = self
            .pad
            .as_ref()
            .filter(|_| self.profile.output.passthrough)
        else {
            return pad;
        };
        let table = self.table();
        let route_consumes = |src: AnalogSource| {
            table
                .routes
                .iter()
                .any(|&i| self.profile.analog[i].source == src && self.profile.analog[i].consume)
        };
        let hidden = |c: Control| {
            table.mapped.contains(&c)
                || self.consumed.contains(&c)
                || self.pending.iter().any(|(p, _)| *p == c)
        };
        // A split pad's clicks come from the touchpad button.
        let click_used = Half::ALL
            .iter()
            .any(|h| hidden(Control::Trackpad(*h, TrackpadPart::Click)));
        for b in PadButton::ALL {
            let used = hidden(Control::Button(b)) || (b == PadButton::Touchpad && click_used);
            if phys.pressed(b) && !used {
                pad.set(b, true);
            }
        }
        for s in Stick::ALL {
            if Control::of_stick(s).any(hidden)
                || route_consumes(AnalogSource::from(s))
                || radial_stick == Some(s)
            {
                continue;
            }
            let cfg = self.sticks().get(s);
            pad.set_stick(s, shape_stick(orient(phys.stick(s), cfg), cfg));
        }
        for t in Trigger::ALL {
            if Control::of_trigger(t).any(hidden) {
                continue;
            }
            pad.set_axis(
                t.axis(),
                shape_trigger(phys.axis(t.axis()), self.triggers().get(t)),
            );
        }
        // A remapped touchpad or gyro no longer reaches the game, like a remapped stick.
        let touch_used =
            Control::of_touchpad().any(hidden) || route_consumes(AnalogSource::Touchpad);
        if !touch_used {
            pad.touch = phys.touch;
        }
        let gyro_used = Control::of_gyro().any(hidden) || route_consumes(AnalogSource::Gyro);
        if !gyro_used {
            pad.motion = phys.motion;
        }
        pad
    }

    /// The analog outputs of the running actions: stick pushes, trigger pulls, mouse motion
    /// and the cursor clip. Returns the stick axes an absolute push sets.
    fn mix_analog_actions(
        &self,
        pad: &mut PadState,
        out: &mut Output,
        dt: f32,
        now: u64,
    ) -> [[bool; 2]; 2] {
        let mut mouse_v = (0.0f32, 0.0f32);
        // Per stick and axis.
        let mut pushes = [[StickPush::default(); 2]; 2];
        for act in &self.acts {
            for &(a, since) in &act.analog {
                match a {
                    AnalogOut::Stick {
                        stick: s,
                        x,
                        y,
                        absolute,
                    } => {
                        let [px, py] = &mut pushes[s as usize];
                        if absolute {
                            let centre = x == 0.0 && y == 0.0;
                            px.absolute |= centre || x != 0.0;
                            py.absolute |= centre || y != 0.0;
                        }
                        let vs = self.virtual_stick(s);
                        let held = secs_since(now, since);
                        for (p, v, m) in [(px, x, vs.multiplier_x), (py, y, vs.multiplier_y)] {
                            let reach = v.abs().min(vs.ramp(held, m));
                            let side = if v < 0.0 { &mut p.neg } else { &mut p.pos };
                            *side = side.max(reach);
                        }
                    }
                    AnalogOut::Trigger(t, v) => {
                        let cur = pad.axis(t.axis());
                        pad.set_axis(t.axis(), cur.max(v.clamp(0.0, 1.0)));
                    }
                    AnalogOut::MouseVelocity(x, y) => {
                        mouse_v.0 += x;
                        mouse_v.1 += y;
                    }
                    AnalogOut::Clip(x1, y1, x2, y2) => out.mouse_clip = Some([x1, y1, x2, y2]),
                    AnalogOut::MouseAccel(x, y, accel) => {
                        let held = secs_since(now, since);
                        let k = (1.0 + accel * held).min(MOUSE_ACCEL_MAX);
                        mouse_v.0 += x * k;
                        mouse_v.1 += y * k;
                    }
                }
            }
        }
        out.mouse_move = (mouse_v.0 * dt, mouse_v.1 * dt);
        for s in Stick::ALL {
            let [px, py] = pushes[s as usize];
            let (cx, cy) = pad.stick(s);
            pad.set_stick(s, (px.apply(cx), py.apply(cy)));
        }
        pushes.map(|axes| axes.map(|p| p.absolute))
    }

    /// Turn actions: horizontal mouse motion spread over the turn time.
    fn spread_turns(&mut self, out: &mut Output, dt: f32) {
        let turn = &self.profile.virtual_devices.mouse;
        let secs = turn.turn_ms as f32 / 1000.0;
        for t in &mut self.outs.turns {
            let max_step = if secs > 0.0 {
                t.total.abs() * dt / secs
            } else {
                f32::INFINITY
            };
            let step = take_toward_zero(&mut t.left, max_step);
            out.mouse_move.0 += step * turn.counts_per_turn / 360.0;
        }
        self.outs.turns.retain(|t| t.left.abs() > 1e-4);
    }

    /// The layer's analog routes, added to the mouse motion and the virtual sticks (except
    /// the axes an absolute push sets).
    fn run_routes(
        &mut self,
        pad: &mut PadState,
        out: &mut Output,
        absolute: [[bool; 2]; 2],
        radial_stick: Option<Stick>,
        dt: f32,
        now: u64,
    ) {
        let input = RouteInput {
            pad: self.pad.as_ref(),
            sticks: self.layer_settings.sticks(&self.profile, self.layer),
            mouse: self.mouse_counts,
            dt,
            now,
            gyro_enabled: self.gyro_enabled,
            turn: &self.profile.virtual_devices.mouse,
            gyro_rates: self
                .pad
                .and_then(|p| p.motion)
                .map(|m| gyro::route_rates(&self.profile.gyro, &m)),
            tilt: (self.profile.gyro.mode == GyroMode::Tilt)
                .then(|| self.gyro_axes.tilt_deflection()),
        };
        for &i in &self.c.layer(self.layer).routes {
            let r = &self.profile.analog[i];
            let pointing = radial_stick.is_some() && r.source.stick() == radial_stick;
            if pointing {
                self.route_state[i] = RouteState::default();
                continue;
            }
            if !r.while_held.iter().all(|c| self.cur_controls.contains(c)) {
                self.route_state[i] = RouteState::default();
                continue;
            }
            match routes::run(r, &mut self.route_state[i], &input) {
                RouteOutput::None => {}
                RouteOutput::Mouse(x, y) => {
                    out.mouse_move.0 += x;
                    out.mouse_move.1 += y;
                }
                RouteOutput::Stick(s, x, y) => {
                    let (cx, cy) = pad.stick(s);
                    let [ax, ay] = absolute[s as usize];
                    let add =
                        |c: f32, v: f32, abs: bool| if abs { c } else { (c + v).clamp(-1.0, 1.0) };
                    pad.set_stick(s, (add(cx, x, ax), add(cy, y, ay)));
                }
            }
        }
    }

    /// The virtual sticks' own output curve over every source, then their jitter.
    fn finish_virtual_sticks(&mut self, pad: &mut PadState, now: u64) {
        for s in Stick::ALL {
            let curve = &self.virtual_stick(s).curve;
            let (x, y) = pad.stick(s);
            let mag = x.hypot(y).min(1.0);
            if !curve.is_empty() && mag > 0.0 {
                let k = apply_curve(curve, mag) / mag;
                pad.set_stick(s, ((x * k).clamp(-1.0, 1.0), (y * k).clamp(-1.0, 1.0)));
            }
        }
        // Jitter: an offset that steps through its pattern after random delays.
        for s in Stick::ALL {
            let j = self.virtual_stick(s).jitter.clone();
            let st = &mut self.jitter[s as usize];
            if !j.enabled {
                *st = JitterState::default();
                continue;
            }
            if !st.active || now >= st.next_at {
                let points = j.pattern.points();
                st.point = if st.active {
                    (st.point + 1) % points.len()
                } else {
                    0
                };
                let size = j.min + (j.max - j.min).max(0.0) * self.outs.rng.unit();
                let (px, py) = points[st.point];
                st.offset = (px * size, py * size);
                let spread = u64::from(j.delay_max_ms.saturating_sub(j.delay_min_ms));
                st.next_at = now + u64::from(j.delay_min_ms.max(1)) + self.outs.rng.upto(spread);
                st.active = true;
            }
            let (x, y) = pad.stick(s);
            pad.set_stick(
                s,
                (
                    (x + st.offset.0).clamp(-1.0, 1.0),
                    (y + st.offset.1).clamp(-1.0, 1.0),
                ),
            );
        }
    }

    /// A touch action plays its fingers on the virtual touchpad.
    fn play_touch(&mut self, pad: &mut PadState, now: u64) {
        if let Some(p) = self.outs.touch {
            if now < p.start + p.ms {
                let t = now.saturating_sub(p.start) as f32 / p.ms.max(1) as f32;
                pad.touch = touch::output_fingers(p.gesture, t);
            } else {
                self.outs.touch = None;
            }
        }
    }

    /// Shakes: a square wave on one accelerometer axis, starting in its direction.
    fn play_shakes(&mut self, pad: &mut PadState, now: u64) {
        let motion = &self.profile.virtual_devices.motion;
        let shake_ms = u64::from(motion.shake_ms);
        self.outs.shakes.retain(|(_, t0)| now < t0 + shake_ms);
        if !self.outs.shakes.is_empty() {
            let mut m = pad.motion.unwrap_or(Motion {
                gyro: [0.0; 3],
                accel: [0.0, 1.0, 0.0],
            });
            for &(axis, t0) in &self.outs.shakes {
                let (i, sign) = axis.axis();
                let flipped = (now - t0) / SHAKE_FLIP_MS % 2 == 1;
                m.accel[i] += if flipped { -sign } else { sign } * motion.shake_g;
            }
            pad.motion = Some(m);
        }
    }
}
