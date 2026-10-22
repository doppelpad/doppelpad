//! Continuous analog routes: stick → mouse, mouse → stick, gyro → mouse/stick,
//! touchpad → mouse/stick.

use super::analog::{MIN_SPAN, ema_alpha, mix, orient, secs_since, shape_radial, take_toward_zero};
use crate::pad::{PadState, Stick};
use crate::profile::{
    AnalogMapping, AnalogSource, AnalogTarget, StickConfig, Sticks, VirtualMouse,
};

/// Stick → mouse acceleration starts at this deflection ...
const ACCEL_FROM: f32 = 0.9;
/// ... and makes the cursor at most this many times faster.
const ACCEL_MAX: f32 = 4.0;
/// A flick stick stays out until it falls below this part of its threshold.
const FLICK_RELEASE_RATIO: f32 = 0.9;
/// The touchpad's glide speed keeps this much of the last speed each tick and takes the
/// rest from the newest motion.
const TOUCH_VEL_KEEP: f32 = 0.7;
const TOUCH_VEL_NEW: f32 = 0.3;

/// Calibration constants: what `sensitivity = 1.0` means for each route.
pub(super) mod scale {
    /// Stick → mouse: cursor speed at full deflection, pixels per second.
    pub(crate) const STICK_MOUSE_PX_PER_S: f32 = 1000.0;
    /// Mouse → stick: mouse speed (counts per second) that gives full deflection.
    pub(crate) const MOUSE_STICK_COUNTS_PER_S: f32 = 2000.0;
    /// Mouse → stick without spring: mouse counts from the centre to full deflection.
    pub(crate) const MOUSE_STICK_COUNTS_FULL: f32 = 500.0;
    /// Gyro → mouse: pixels per degree of rotation.
    pub(crate) const GYRO_MOUSE_PX_PER_DEG: f32 = 10.0;
    /// Gyro → stick: rotation rate (degrees per second) that gives full deflection.
    pub(crate) const GYRO_STICK_DPS: f32 = 100.0;
    /// Touchpad → mouse: pixels for a swipe across the full pad width.
    pub(crate) const TOUCH_MOUSE_PX_PER_PAD: f32 = 1000.0;
    /// Gyro rates below this are treated as noise (degrees per second).
    pub(crate) const GYRO_NOISE_DPS: f32 = 0.5;
    /// Longest trackball glide (friction 0), milliseconds.
    pub(crate) const TRACKBALL_MAX_MS: f32 = 5200.0;
}

/// The touchpad's trackball glide after the finger lifted: speed (pixels per second), start
/// and length in milliseconds.
#[derive(Clone, Copy, Debug)]
struct Glide {
    vel: (f32, f32),
    since: u64,
    ms: f32,
}

#[derive(Clone, Debug, Default)]
pub(super) struct RouteState {
    ema: (f32, f32),
    last_motion: u64,
    last_touch: Option<(f32, f32)>,
    touch_vel: (f32, f32),
    glide: Option<Glide>,
    /// Flick stick: out past the threshold, the last stick angle, and the flick turn still
    /// to send (counts) with its speed (counts per second).
    flick_out: bool,
    flick_angle: f32,
    flick_left: f32,
    flick_rate: f32,
    /// Stick → mouse: held at the edge since then (acceleration).
    accel_since: Option<u64>,
    /// Mouse → stick without spring: where the mouse has pushed the stick.
    pos: (f32, f32),
}

pub(super) struct RouteInput<'a> {
    pub(super) pad: Option<&'a PadState>,
    pub(super) sticks: &'a Sticks,
    /// Raw mouse counts since the previous tick (y down).
    pub(super) mouse: (f32, f32),
    /// Seconds since the previous tick.
    pub(super) dt: f32,
    pub(super) now: u64,
    pub(super) gyro_enabled: bool,
    /// Turn calibration for flick routes.
    pub(super) turn: &'a VirtualMouse,
    /// Gyro yaw and pitch rates for routes (already in the profile's gyro space).
    pub(super) gyro_rates: Option<(f32, f32)>,
    /// In tilt mode: steering and pitch tilt as deflections (left and up positive).
    pub(super) tilt: Option<(f32, f32)>,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(super) enum RouteOutput {
    None,
    /// Pixels to move this tick (y down).
    Mouse(f32, f32),
    Stick(Stick, f32, f32),
}

fn smooth(st: &mut RouteState, v: (f32, f32), smoothing_ms: u32, dt: f32) -> (f32, f32) {
    if smoothing_ms == 0 || dt <= 0.0 {
        st.ema = v;
        return v;
    }
    st.ema = mix(st.ema, v, ema_alpha(dt * 1000.0, smoothing_ms as f32));
    st.ema
}

/// Normalized magnitude after a source deadzone, the deflection that counts as full and a
/// power curve, keeping direction.
fn shape_vector((x, y): (f32, f32), deadzone: f32, outer: f32, curve: f32) -> (f32, f32) {
    let mag = (x * x + y * y).sqrt();
    if mag <= deadzone || mag == 0.0 {
        return (0.0, 0.0);
    }
    let t = ((mag - deadzone) / (outer - deadzone).max(MIN_SPAN)).clamp(0.0, 1.0);
    let t = if curve > 0.0 && curve != 1.0 {
        t.powf(curve)
    } else {
        t
    };
    (x / mag * t, y / mag * t)
}

/// `v`, or 0 below `min` (noise).
fn gate(v: f32, min: f32) -> f32 {
    if v.abs() < min { 0.0 } else { v }
}

/// A power curve over the magnitude; 1 leaves the vector as it is.
fn power_curve(v: (f32, f32), curve: f32) -> (f32, f32) {
    if curve != 1.0 {
        shape_vector(v, 0.0, 1.0, curve)
    } else {
        v
    }
}

fn to_stick(r: &AnalogMapping, (x, y): (f32, f32)) -> RouteOutput {
    let Some(stick) = r.target.stick() else {
        return RouteOutput::None;
    };
    let (mut x, mut y) = (x, y * r.vertical_ratio);
    let mag = (x * x + y * y).sqrt();
    if mag > 1.0 {
        x /= mag;
        y /= mag;
    }
    let mag = mag.min(1.0);
    if mag < r.snap {
        return RouteOutput::Stick(stick, 0.0, 0.0);
    }
    if mag > 0.0 && r.anti_deadzone > 0.0 {
        let lifted = r.anti_deadzone + (1.0 - r.anti_deadzone) * mag;
        x *= lifted / mag;
        y *= lifted / mag;
    }
    RouteOutput::Stick(stick, x, y)
}

fn stick_source(s: Stick, input: &RouteInput) -> Option<(f32, f32)> {
    let pad = input.pad?;
    let cfg: &StickConfig = input.sticks.get(s);
    Some(orient(pad.stick(s), cfg))
}

/// Flick stick: when the stick is pushed out past the threshold, turn to face where it
/// points (spread over the turn time); while it stays out, turn with its rotation.
fn flick(
    r: &AnalogMapping,
    st: &mut RouteState,
    (x, y): (f32, f32),
    input: &RouteInput,
    sx: f32,
) -> RouteOutput {
    let threshold = r.deadzone.clamp(0.05, 0.99);
    let mag = x.hypot(y);
    let per_deg = input.turn.counts_per_turn / 360.0 * r.sensitivity * sx;
    // Angle from "up", clockwise positive.
    let angle = x.atan2(y).to_degrees();
    let mut counts = 0.0;
    if !st.flick_out && mag >= threshold {
        st.flick_out = true;
        st.flick_angle = angle;
        st.flick_left = angle * per_deg;
        let secs = input.turn.turn_ms as f32 / 1000.0;
        st.flick_rate = if secs > 0.0 {
            st.flick_left.abs() / secs
        } else {
            f32::INFINITY
        };
    } else if st.flick_out && mag >= threshold * FLICK_RELEASE_RATIO {
        let mut d = angle - st.flick_angle;
        if d > 180.0 {
            d -= 360.0;
        } else if d < -180.0 {
            d += 360.0;
        }
        st.flick_angle = angle;
        counts += if input.dt > 0.0 {
            smooth(st, (d * per_deg / input.dt, 0.0), r.smoothing_ms, input.dt).0 * input.dt
        } else {
            d * per_deg
        };
    } else if st.flick_out {
        st.flick_out = false;
        st.ema = (0.0, 0.0);
    }
    if st.flick_left != 0.0 {
        let max_step = if input.dt > 0.0 {
            st.flick_rate * input.dt
        } else {
            0.0
        };
        counts += take_toward_zero(&mut st.flick_left, max_step);
    }
    RouteOutput::Mouse(counts, 0.0)
}

pub(super) fn run(r: &AnalogMapping, st: &mut RouteState, input: &RouteInput) -> RouteOutput {
    let sx = if r.invert_x { -1.0 } else { 1.0 };
    let sy = if r.invert_y { -1.0 } else { 1.0 };
    match r.source {
        AnalogSource::LeftStick | AnalogSource::RightStick => stick_route(r, st, input, sx, sy),
        AnalogSource::Mouse => mouse_route(r, st, input, sx, sy),
        AnalogSource::Gyro => gyro_route(r, st, input, sx, sy),
        AnalogSource::Touchpad => touch_route(r, st, input, sx, sy),
    }
}

/// A physical stick to the mouse, a flick or a virtual stick.
fn stick_route(
    r: &AnalogMapping,
    st: &mut RouteState,
    input: &RouteInput,
    sx: f32,
    sy: f32,
) -> RouteOutput {
    let Some(raw) = r.source.stick().and_then(|s| stick_source(s, input)) else {
        return RouteOutput::None;
    };
    match r.target {
        AnalogTarget::Flick => flick(r, st, raw, input, sx),
        AnalogTarget::Mouse => {
            let (x, y) = shape_vector(raw, r.deadzone, r.outer, r.curve);
            // Held at the edge, the speed grows (acceleration).
            let boost = if r.acceleration > 0.0 && x.hypot(y) >= ACCEL_FROM {
                let since = *st.accel_since.get_or_insert(input.now);
                let held = secs_since(input.now, since);
                (1.0 + r.acceleration * held).min(ACCEL_MAX)
            } else {
                st.accel_since = None;
                1.0
            };
            let speed = scale::STICK_MOUSE_PX_PER_S * r.sensitivity * boost;
            // Stick y is up-positive; the screen is y-down.
            let v = smooth(
                st,
                (x * speed * sx, -y * speed * r.vertical_ratio * sy),
                r.smoothing_ms,
                input.dt,
            );
            RouteOutput::Mouse(v.0 * input.dt, v.1 * input.dt)
        }
        _ => {
            let (x, y) = shape_radial(raw, r.deadzone, r.outer, |t| t);
            to_stick(r, (x * r.sensitivity * sx, y * r.sensitivity * sy))
        }
    }
}

/// The physical mouse to a virtual stick.
fn mouse_route(
    r: &AnalogMapping,
    st: &mut RouteState,
    input: &RouteInput,
    sx: f32,
    sy: f32,
) -> RouteOutput {
    if matches!(r.target, AnalogTarget::Mouse | AnalogTarget::Flick) || input.dt <= 0.0 {
        return RouteOutput::None;
    }
    let (dx, dy) = (
        gate(input.mouse.0, r.noise_filter),
        gate(input.mouse.1, r.noise_filter),
    );
    if !r.spring {
        // The mouse pushes the stick around and it stays where it was left.
        let k = r.sensitivity / scale::MOUSE_STICK_COUNTS_FULL;
        st.pos = (
            (st.pos.0 + dx * k * sx).clamp(-1.0, 1.0),
            (st.pos.1 - dy * k * sy).clamp(-1.0, 1.0),
        );
        return to_stick(r, power_curve(st.pos, r.curve));
    }
    if dx != 0.0 || dy != 0.0 {
        st.last_motion = input.now;
    } else if input.now.saturating_sub(st.last_motion) >= u64::from(r.decay_ms) {
        st.ema = (0.0, 0.0);
        return to_stick(r, (0.0, 0.0));
    }
    // Counts/s → deflection; mouse y is down-positive, stick y is up-positive.
    let full = scale::MOUSE_STICK_COUNTS_PER_S / r.sensitivity.max(0.001);
    let raw = (dx / input.dt / full * sx, -dy / input.dt / full * sy);
    let v = if dx != 0.0 || dy != 0.0 {
        smooth(st, raw, r.smoothing_ms, input.dt)
    } else {
        st.ema
    };
    to_stick(r, power_curve(v, r.curve))
}

/// The gyro to the mouse or a virtual stick.
fn gyro_route(
    r: &AnalogMapping,
    st: &mut RouteState,
    input: &RouteInput,
    sx: f32,
    sy: f32,
) -> RouteOutput {
    let Some((yaw, pitch)) = input.gyro_rates else {
        return RouteOutput::None;
    };
    if !input.gyro_enabled {
        st.ema = (0.0, 0.0);
        return match r.target {
            AnalogTarget::Mouse => RouteOutput::None,
            _ => to_stick(r, (0.0, 0.0)),
        };
    }
    // Turning left (yaw +) moves the aim left; tilting up (pitch +) moves it up.
    let (yaw, pitch) = (
        gate(yaw, scale::GYRO_NOISE_DPS),
        gate(pitch, scale::GYRO_NOISE_DPS),
    );
    // In tilt mode a stick follows how far the controller is tilted.
    if let Some((steer, up)) = input.tilt
        && r.target != AnalogTarget::Mouse
    {
        let v = shape_vector((-steer * sx, up * sy), r.deadzone, 1.0, r.curve);
        return to_stick(r, (v.0 * r.sensitivity, v.1 * r.sensitivity));
    }
    match r.target {
        AnalogTarget::Mouse => {
            let k = scale::GYRO_MOUSE_PX_PER_DEG * r.sensitivity;
            let v = smooth(
                st,
                (-yaw * k * sx, -pitch * k * r.vertical_ratio * sy),
                r.smoothing_ms,
                input.dt,
            );
            RouteOutput::Mouse(v.0 * input.dt, v.1 * input.dt)
        }
        _ => {
            let k = r.sensitivity / scale::GYRO_STICK_DPS;
            let v = smooth(
                st,
                (-yaw * k * sx, pitch * k * sy),
                r.smoothing_ms,
                input.dt,
            );
            to_stick(r, v)
        }
    }
}

/// The touchpad to the mouse (with trackball glide) or a virtual stick.
fn touch_route(
    r: &AnalogMapping,
    st: &mut RouteState,
    input: &RouteInput,
    sx: f32,
    sy: f32,
) -> RouteOutput {
    let finger = input.pad.and_then(|p| p.touch[0]).map(|t| (t.x, t.y));
    match r.target {
        AnalogTarget::Mouse => {
            let k = scale::TOUCH_MOUSE_PX_PER_PAD * r.sensitivity;
            match (finger, st.last_touch) {
                (Some(f), Some(prev)) => {
                    st.glide = None;
                    let d = (
                        (f.0 - prev.0) * k * sx,
                        (f.1 - prev.1) * k * r.vertical_ratio * sy,
                    );
                    st.last_touch = Some(f);
                    if input.dt > 0.0 {
                        let v = (d.0 / input.dt, d.1 / input.dt);
                        st.touch_vel = (
                            st.touch_vel.0 * TOUCH_VEL_KEEP + v.0 * TOUCH_VEL_NEW,
                            st.touch_vel.1 * TOUCH_VEL_KEEP + v.1 * TOUCH_VEL_NEW,
                        );
                    }
                    RouteOutput::Mouse(d.0, d.1)
                }
                (Some(f), None) => {
                    st.glide = None;
                    st.last_touch = Some(f);
                    st.touch_vel = (0.0, 0.0);
                    RouteOutput::None
                }
                (None, Some(_)) => {
                    st.last_touch = None;
                    if r.trackball_friction < 1.0 {
                        let duration =
                            scale::TRACKBALL_MAX_MS * (1.0 - r.trackball_friction.clamp(0.0, 1.0));
                        st.glide = Some(Glide {
                            vel: st.touch_vel,
                            since: input.now,
                            ms: duration,
                        });
                    }
                    RouteOutput::None
                }
                (None, None) => match st.glide {
                    Some(g) => {
                        let t = input.now.saturating_sub(g.since) as f32;
                        if t >= g.ms {
                            st.glide = None;
                            return RouteOutput::None;
                        }
                        let f = 1.0 - t / g.ms;
                        RouteOutput::Mouse(g.vel.0 * f * input.dt, g.vel.1 * f * input.dt)
                    }
                    None => RouteOutput::None,
                },
            }
        }
        _ => {
            // Spring mode: finger position relative to the pad centre is the stick.
            let v = finger
                .map(|(x, y)| ((x - 0.5) * 2.0, (0.5 - y) * 2.0))
                .unwrap_or((0.0, 0.0));
            let v = shape_vector(v, r.deadzone, 1.0, r.curve);
            to_stick(r, (v.0 * r.sensitivity * sx, v.1 * r.sensitivity * sy))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::{Axis, Motion, TouchPoint};

    fn input<'a>(
        pad: Option<&'a PadState>,
        sticks: &'a Sticks,
        mouse: (f32, f32),
        now: u64,
    ) -> RouteInput<'a> {
        RouteInput {
            pad,
            sticks,
            mouse,
            dt: 0.01,
            now,
            gyro_enabled: true,
            turn: &TURN,
            gyro_rates: pad.and_then(|p| p.motion).map(|m| (m.gyro[1], m.gyro[0])),
            tilt: None,
        }
    }

    const TURN: VirtualMouse = VirtualMouse {
        counts_per_turn: 3600.0,
        turn_ms: 100,
        wheel_repeat_ms: 0,
    };

    #[test]
    fn stick_to_mouse_full_deflection_speed() {
        let sticks = Sticks::default();
        let mut pad = PadState::default();
        pad.set_axis(Axis::RightX, 1.0);
        let r = AnalogMapping {
            deadzone: 0.0,
            ..Default::default()
        };
        let out = run(
            &r,
            &mut RouteState::default(),
            &input(Some(&pad), &sticks, (0.0, 0.0), 0),
        );
        assert_eq!(out, RouteOutput::Mouse(10.0, 0.0), "1000 px/s for 10 ms");
    }

    #[test]
    fn stick_to_mouse_is_at_full_speed_from_outer() {
        let sticks = Sticks::default();
        let mut pad = PadState::default();
        let r = AnalogMapping {
            deadzone: 0.0,
            outer: 0.9,
            ..Default::default()
        };
        let mut speed = |x: f32| {
            pad.set_axis(Axis::RightX, x);
            match run(
                &r,
                &mut RouteState::default(),
                &input(Some(&pad), &sticks, (0.0, 0.0), 0),
            ) {
                RouteOutput::Mouse(dx, _) => dx,
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(speed(0.95), 10.0, "past outer is full speed");
        assert!(
            (speed(0.45) - 5.0).abs() < 1e-4,
            "half of outer is half speed"
        );
    }

    #[test]
    fn flick_turns_to_the_stick_then_with_it() {
        let sticks = Sticks::default();
        let r = AnalogMapping {
            target: AnalogTarget::Flick,
            deadzone: 0.6,
            ..Default::default()
        };
        let mut st = RouteState::default();
        let mut pad = PadState::default();
        let turn = |pad: &PadState, ticks: u64, st: &mut RouteState| {
            (0..ticks)
                .map(
                    |t| match run(&r, st, &input(Some(pad), &sticks, (0.0, 0.0), t)) {
                        RouteOutput::Mouse(x, y) => {
                            assert_eq!(y, 0.0);
                            x
                        }
                        other => panic!("{other:?}"),
                    },
                )
                .sum::<f32>()
        };
        // 10 counts per degree; a flick takes 100 ms (10 ticks of 10 ms).
        pad.set_stick(Stick::Right, (1.0, 0.0));
        let first = turn(&pad, 1, &mut st);
        assert!(
            (first - 90.0).abs() < 0.01,
            "a tenth of the flick per tick: {first}"
        );
        assert!(
            (first + turn(&pad, 20, &mut st) - 900.0).abs() < 0.01,
            "flick right: 90°"
        );
        pad.set_stick(Stick::Right, (0.0, -1.0));
        assert!(
            (turn(&pad, 1, &mut st) - 900.0).abs() < 0.01,
            "rotate a quarter more"
        );
        pad.set_stick(Stick::Right, (0.0, 0.0));
        assert_eq!(turn(&pad, 1, &mut st), 0.0, "released: re-armed");
        pad.set_stick(Stick::Right, (-0.7, 0.0));
        assert!((turn(&pad, 20, &mut st) + 900.0).abs() < 0.01, "flick left");
    }

    #[test]
    fn mouse_stick_without_spring_noise_and_snap() {
        let sticks = Sticks::default();
        let r = AnalogMapping {
            source: AnalogSource::Mouse,
            target: AnalogTarget::RightStick,
            spring: false,
            noise_filter: 2.0,
            snap: 0.1,
            ..Default::default()
        };
        let mut st = RouteState::default();
        let mut step = |dx: f32| run(&r, &mut st, &input(None, &sticks, (dx, 0.0), 0));
        // Under the noise filter: nothing moves.
        assert_eq!(step(1.0), RouteOutput::Stick(Stick::Right, 0.0, 0.0));
        // 25 counts = 5 % of full: below the snap.
        assert_eq!(step(25.0), RouteOutput::Stick(Stick::Right, 0.0, 0.0));
        // Another 225 counts: half way, and it stays there without motion.
        assert_eq!(step(225.0), RouteOutput::Stick(Stick::Right, 0.5, 0.0));
        assert_eq!(step(0.0), RouteOutput::Stick(Stick::Right, 0.5, 0.0));
    }

    #[test]
    fn stick_mouse_accelerates_at_the_edge() {
        let sticks = Sticks::default();
        let mut pad = PadState::default();
        pad.set_axis(Axis::RightX, 1.0);
        let r = AnalogMapping {
            deadzone: 0.0,
            acceleration: 1.0,
            ..Default::default()
        };
        let mut st = RouteState::default();
        let at = |st: &mut RouteState, now| match run(
            &r,
            st,
            &input(Some(&pad), &sticks, (0.0, 0.0), now),
        ) {
            RouteOutput::Mouse(x, _) => x,
            other => panic!("{other:?}"),
        };
        assert_eq!(at(&mut st, 0), 10.0);
        assert_eq!(at(&mut st, 1000), 20.0, "twice as fast after a second");
        assert_eq!(at(&mut st, 5000), 40.0, "at most 4 ×");
    }

    #[test]
    fn stick_up_moves_cursor_up() {
        let sticks = Sticks::default();
        let mut pad = PadState::default();
        pad.set_axis(Axis::RightY, 1.0);
        let r = AnalogMapping {
            deadzone: 0.0,
            ..Default::default()
        };
        let RouteOutput::Mouse(_, dy) = run(
            &r,
            &mut RouteState::default(),
            &input(Some(&pad), &sticks, (0.0, 0.0), 0),
        ) else {
            panic!()
        };
        assert!(dy < 0.0);
    }

    #[test]
    fn mouse_to_stick_and_decay() {
        let sticks = Sticks::default();
        let r = AnalogMapping {
            source: AnalogSource::Mouse,
            target: AnalogTarget::RightStick,
            decay_ms: 40,
            ..Default::default()
        };
        let mut st = RouteState::default();
        // 10 counts right in 10 ms = 1000 counts/s = half deflection.
        let out = run(&r, &mut st, &input(None, &sticks, (10.0, -10.0), 100));
        let RouteOutput::Stick(Stick::Right, x, y) = out else {
            panic!("{out:?}")
        };
        assert!(
            (x - 0.5).abs() < 1e-4 && (y - 0.5).abs() < 1e-4,
            "mouse up = stick up"
        );
        // Holds while within the decay window, then recentres.
        assert!(
            matches!(run(&r, &mut st, &input(None, &sticks, (0.0, 0.0), 120)), RouteOutput::Stick(_, x, _) if x > 0.4)
        );
        assert_eq!(
            run(&r, &mut st, &input(None, &sticks, (0.0, 0.0), 141)),
            RouteOutput::Stick(Stick::Right, 0.0, 0.0)
        );
    }

    #[test]
    fn gyro_turn_left_moves_left() {
        let sticks = Sticks::default();
        let pad = PadState {
            motion: Some(Motion {
                gyro: [0.0, 90.0, 0.0],
                accel: [0.0; 3],
            }),
            ..Default::default()
        };
        let r = AnalogMapping {
            source: AnalogSource::Gyro,
            target: AnalogTarget::Mouse,
            ..Default::default()
        };
        let RouteOutput::Mouse(dx, _) = run(
            &r,
            &mut RouteState::default(),
            &input(Some(&pad), &sticks, (0.0, 0.0), 0),
        ) else {
            panic!()
        };
        assert!(
            (dx + 9.0).abs() < 1e-3,
            "90°/s for 10 ms = 0.9° = 9 px left, got {dx}"
        );
    }

    #[test]
    fn touchpad_trackball_glides_after_lift() {
        let sticks = Sticks::default();
        let r = AnalogMapping {
            source: AnalogSource::Touchpad,
            target: AnalogTarget::Mouse,
            trackball_friction: 0.5,
            ..Default::default()
        };
        let mut st = RouteState::default();
        let mut pad = PadState::default();
        for i in 0..10 {
            pad.touch[0] = Some(TouchPoint {
                x: 0.1 + i as f32 * 0.01,
                y: 0.5,
            });
            run(&r, &mut st, &input(Some(&pad), &sticks, (0.0, 0.0), i * 10));
        }
        pad.touch[0] = None;
        run(&r, &mut st, &input(Some(&pad), &sticks, (0.0, 0.0), 100));
        let RouteOutput::Mouse(dx, _) =
            run(&r, &mut st, &input(Some(&pad), &sticks, (0.0, 0.0), 110))
        else {
            panic!()
        };
        assert!(dx > 0.0, "keeps gliding right");
    }
}
