//! Stick and trigger shaping, and analog → digital derivation.

use crate::control::Dir;
use crate::profile::{StickConfig, TriggerConfig};

/// `tan(22.5°)`: with this sector ratio a stick has eight 45° sectors.
pub const DIRECTION_SECTOR: f32 = 0.414_213_57;

/// Rotate a stick vector by `deg` degrees (counter-clockwise, y up).
pub fn rotate((x, y): (f32, f32), deg: f32) -> (f32, f32) {
    if deg == 0.0 {
        return (x, y);
    }
    let (s, c) = deg.to_radians().sin_cos();
    (
        (x * c - y * s).clamp(-1.0, 1.0),
        (x * s + y * c).clamp(-1.0, 1.0),
    )
}

/// Physical stick after inversion and rotation; this is what digital direction
/// controls are evaluated on (before deadzone and curve).
pub fn orient(raw: (f32, f32), cfg: &StickConfig) -> (f32, f32) {
    let x = if cfg.invert_x { -raw.0 } else { raw.0 };
    let y = if cfg.invert_y { -raw.1 } else { raw.1 };
    rotate((x, y), cfg.rotation)
}

/// Piecewise-linear response curve through `(0,0)`, the given points, and `(1,1)`.
pub fn apply_curve(points: &[[f32; 2]], t: f32) -> f32 {
    if points.is_empty() {
        return t;
    }
    let mut prev = [0.0f32, 0.0];
    for &p in points.iter().chain(std::iter::once(&[1.0, 1.0])) {
        let p = [p[0].clamp(0.0, 1.0), p[1].clamp(0.0, 1.0)];
        if t <= p[0] {
            let span = p[0] - prev[0];
            if span <= f32::EPSILON {
                return p[1];
            }
            return prev[1] + (p[1] - prev[1]) * (t - prev[0]) / span;
        }
        prev = p;
    }
    1.0
}

/// Shape an oriented stick vector for output: radial scaled deadzone, outer
/// saturation, curve, anti-deadzone and sensitivity.
pub fn shape_stick((x, y): (f32, f32), cfg: &StickConfig) -> (f32, f32) {
    let mag = (x * x + y * y).sqrt();
    if mag <= cfg.deadzone || mag == 0.0 {
        return (0.0, 0.0);
    }
    let outer = cfg.outer.max(cfg.deadzone + 0.01);
    let t = ((mag - cfg.deadzone) / (outer - cfg.deadzone)).clamp(0.0, 1.0);
    let mut out = apply_curve(&cfg.curve, t);
    if out > 0.0 && cfg.anti_deadzone > 0.0 {
        out = cfg.anti_deadzone + (1.0 - cfg.anti_deadzone) * out;
    }
    out = (out * cfg.sensitivity).min(1.0);
    let (ux, uy) = (x / mag, y / mag);
    (ux * out, uy * out)
}

/// Which of the four direction controls are active for an oriented stick vector.
///
/// Within `atan(k)` of an axis only that cardinal is active; in between, both
/// cardinals are (4-way behaviour).
pub fn directions((x, y): (f32, f32), threshold: f32, k: f32) -> [bool; 4] {
    let mut d = [false; 4];
    if (x * x + y * y).sqrt() < threshold.max(1e-4) {
        return d;
    }
    let pure_h = y.abs() <= k * x.abs();
    let pure_v = x.abs() <= k * y.abs();
    let h = if x > 0.0 { Dir::Right } else { Dir::Left };
    let v = if y > 0.0 { Dir::Up } else { Dir::Down };
    if pure_h {
        d[h as usize] = true;
    } else if pure_v {
        d[v as usize] = true;
    } else {
        d[h as usize] = true;
        d[v as usize] = true;
    }
    d
}

/// Trigger output value after deadzone and full-pull rescaling.
pub fn shape_trigger(v: f32, cfg: &TriggerConfig) -> f32 {
    if v <= cfg.deadzone {
        return 0.0;
    }
    let top = cfg.full_pull.max(cfg.deadzone + 0.01);
    ((v - cfg.deadzone) / (top - cfg.deadzone)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: f32 = DIRECTION_SECTOR;

    fn dirs(v: (f32, f32)) -> Vec<Dir> {
        let d = directions(v, 7849.0 / 32768.0, K);
        Dir::ALL.into_iter().filter(|x| d[*x as usize]).collect()
    }

    #[test]
    fn spec_direction_vectors() {
        let n = |v: f32| v / 32768.0;
        assert_eq!(dirs((n(30000.0), n(10000.0))), vec![Dir::Right]);
        let mut both = dirs((n(20000.0), n(20000.0)));
        both.sort();
        assert_eq!(both, vec![Dir::Up, Dir::Right]);
        assert!(dirs((n(5000.0), n(5000.0))).is_empty(), "inside deadzone");
        assert_eq!(dirs((0.0, -1.0)), vec![Dir::Down]);
    }

    #[test]
    fn curve_is_piecewise_linear() {
        let pts = [[0.5, 0.25]];
        assert_eq!(apply_curve(&pts, 0.0), 0.0);
        assert!((apply_curve(&pts, 0.25) - 0.125).abs() < 1e-6);
        assert!((apply_curve(&pts, 0.75) - 0.625).abs() < 1e-6);
        assert_eq!(apply_curve(&pts, 1.0), 1.0);
        assert_eq!(apply_curve(&[], 0.3), 0.3);
    }

    #[test]
    fn stick_deadzone_is_scaled() {
        let cfg = StickConfig {
            deadzone: 0.2,
            ..Default::default()
        };
        assert_eq!(shape_stick((0.1, 0.1), &cfg), (0.0, 0.0));
        let (x, _) = shape_stick((0.6, 0.0), &cfg);
        assert!(
            (x - 0.5).abs() < 1e-5,
            "0.6 maps halfway between 0.2 and 1.0"
        );
        let (x, _) = shape_stick((1.0, 0.0), &cfg);
        assert!((x - 1.0).abs() < 1e-5);
    }

    #[test]
    fn anti_deadzone_lifts_small_values() {
        let cfg = StickConfig {
            deadzone: 0.0,
            anti_deadzone: 0.25,
            ..Default::default()
        };
        let (x, _) = shape_stick((0.01, 0.0), &cfg);
        assert!(x > 0.25);
    }

    #[test]
    fn rotation_and_inversion() {
        let (x, y) = rotate((1.0, 0.0), 90.0);
        assert!(x.abs() < 1e-6 && (y - 1.0).abs() < 1e-6);
        let cfg = StickConfig {
            invert_y: true,
            ..Default::default()
        };
        assert_eq!(orient((0.0, 0.5), &cfg), (0.0, -0.5));
    }

    #[test]
    fn trigger_rescale() {
        let cfg = TriggerConfig {
            deadzone: 0.1,
            full_pull: 0.9,
            ..Default::default()
        };
        assert_eq!(shape_trigger(0.05, &cfg), 0.0);
        assert!((shape_trigger(0.5, &cfg) - 0.5).abs() < 1e-5);
        assert_eq!(shape_trigger(0.95, &cfg), 1.0);
    }
}
