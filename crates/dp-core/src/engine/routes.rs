//! Continuous analog routes: stick → mouse, mouse → stick, gyro → mouse/stick,
//! touchpad → mouse/stick.

use super::analog::{orient, shape_stick};
use crate::pad::{PadState, Stick};
use crate::profile::{AnalogMapping, AnalogSource, AnalogTarget, StickConfig, Sticks};

/// Calibration constants: what `sensitivity = 1.0` means for each route.
pub mod scale {
    /// Stick → mouse: cursor speed at full deflection, pixels per second.
    pub const STICK_MOUSE_PX_PER_S: f32 = 1000.0;
    /// Mouse → stick: mouse speed (counts per second) that gives full deflection.
    pub const MOUSE_STICK_COUNTS_PER_S: f32 = 2000.0;
    /// Gyro → mouse: pixels per degree of rotation.
    pub const GYRO_MOUSE_PX_PER_DEG: f32 = 10.0;
    /// Gyro → stick: rotation rate (degrees per second) that gives full deflection.
    pub const GYRO_STICK_DPS: f32 = 100.0;
    /// Touchpad → mouse: pixels for a swipe across the full pad width.
    pub const TOUCH_MOUSE_PX_PER_PAD: f32 = 1000.0;
    /// Gyro rates below this are treated as noise (degrees per second).
    pub const GYRO_NOISE_DPS: f32 = 0.5;
    /// Longest trackball glide (friction 0), milliseconds.
    pub const TRACKBALL_MAX_MS: f32 = 5200.0;
}

#[derive(Clone, Debug, Default)]
pub struct RouteState {
    ema: (f32, f32),
    last_motion: u64,
    last_touch: Option<(f32, f32)>,
    touch_vel: (f32, f32),
    glide: Option<((f32, f32), u64, f32)>,
}

pub struct RouteInput<'a> {
    pub pad: Option<&'a PadState>,
    pub sticks: &'a Sticks,
    /// Raw mouse counts since the previous tick (y down).
    pub mouse: (f32, f32),
    /// Seconds since the previous tick.
    pub dt: f32,
    pub now: u64,
    pub gyro_enabled: bool,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RouteOutput {
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
    let a = 1.0 - (-dt * 1000.0 / smoothing_ms as f32).exp();
    st.ema = (
        st.ema.0 + (v.0 - st.ema.0) * a,
        st.ema.1 + (v.1 - st.ema.1) * a,
    );
    st.ema
}

/// Normalized magnitude after a source deadzone and power curve, keeping direction.
fn shape_vector((x, y): (f32, f32), deadzone: f32, curve: f32) -> (f32, f32) {
    let mag = (x * x + y * y).sqrt();
    if mag <= deadzone || mag == 0.0 {
        return (0.0, 0.0);
    }
    let t = ((mag - deadzone) / (1.0 - deadzone).max(0.01)).clamp(0.0, 1.0);
    let t = if curve > 0.0 && curve != 1.0 {
        t.powf(curve)
    } else {
        t
    };
    (x / mag * t, y / mag * t)
}

fn to_stick(r: &AnalogMapping, (x, y): (f32, f32)) -> RouteOutput {
    let stick = match r.target {
        AnalogTarget::LeftStick => Stick::Left,
        AnalogTarget::RightStick => Stick::Right,
        AnalogTarget::Mouse => return RouteOutput::None,
    };
    let (mut x, mut y) = (x, y * r.vertical_ratio);
    let mag = (x * x + y * y).sqrt();
    if mag > 1.0 {
        x /= mag;
        y /= mag;
    }
    let mag = mag.min(1.0);
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

pub fn run(r: &AnalogMapping, st: &mut RouteState, input: &RouteInput) -> RouteOutput {
    let sx = if r.invert_x { -1.0 } else { 1.0 };
    let sy = if r.invert_y { -1.0 } else { 1.0 };
    match r.source {
        AnalogSource::LeftStick | AnalogSource::RightStick => {
            let s = if r.source == AnalogSource::LeftStick {
                Stick::Left
            } else {
                Stick::Right
            };
            let Some(raw) = stick_source(s, input) else {
                return RouteOutput::None;
            };
            match r.target {
                AnalogTarget::Mouse => {
                    let (x, y) = shape_vector(raw, r.deadzone, r.curve);
                    let speed = scale::STICK_MOUSE_PX_PER_S * r.sensitivity;
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
                    let cfg = StickConfig {
                        deadzone: r.deadzone,
                        ..Default::default()
                    };
                    let (x, y) = shape_stick(raw, &cfg);
                    to_stick(r, (x * r.sensitivity * sx, y * r.sensitivity * sy))
                }
            }
        }
        AnalogSource::Mouse => {
            if r.target == AnalogTarget::Mouse || input.dt <= 0.0 {
                return RouteOutput::None;
            }
            let (dx, dy) = input.mouse;
            if dx != 0.0 || dy != 0.0 {
                st.last_motion = input.now;
            } else if input.now.saturating_sub(st.last_motion) >= r.decay_ms as u64 {
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
            let v = if r.curve != 1.0 {
                shape_vector(v, 0.0, r.curve)
            } else {
                v
            };
            to_stick(r, v)
        }
        AnalogSource::Gyro => {
            let Some(m) = input.pad.and_then(|p| p.motion) else {
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
            let noise = |v: f32| {
                if v.abs() < scale::GYRO_NOISE_DPS {
                    0.0
                } else {
                    v
                }
            };
            let (yaw, pitch) = (noise(m.gyro[1]), noise(m.gyro[0]));
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
        AnalogSource::Touchpad => {
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
                                    st.touch_vel.0 * 0.7 + v.0 * 0.3,
                                    st.touch_vel.1 * 0.7 + v.1 * 0.3,
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
                                let duration = scale::TRACKBALL_MAX_MS
                                    * (1.0 - r.trackball_friction.clamp(0.0, 1.0));
                                st.glide = Some((st.touch_vel, input.now, duration));
                            }
                            RouteOutput::None
                        }
                        (None, None) => match st.glide {
                            Some((v0, t0, duration)) => {
                                let t = input.now.saturating_sub(t0) as f32;
                                if t >= duration {
                                    st.glide = None;
                                    return RouteOutput::None;
                                }
                                let f = 1.0 - t / duration;
                                RouteOutput::Mouse(v0.0 * f * input.dt, v0.1 * f * input.dt)
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
                    let v = shape_vector(v, r.deadzone, r.curve);
                    to_stick(r, (v.0 * r.sensitivity * sx, v.1 * r.sensitivity * sy))
                }
            }
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
        }
    }

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
