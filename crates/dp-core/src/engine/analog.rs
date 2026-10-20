//! Stick and trigger shaping, and analog → digital derivation.

use crate::control::{Dir, Zone};
use crate::profile::{StickConfig, TriggerConfig, ZoneRadius};

/// `tan(22.5°)`: with this sector ratio a stick has eight 45° sectors.
pub const DIRECTION_SECTOR: f32 = 0.414_213_57;

/// Smallest usable zone threshold; a zero threshold would count the resting
/// control as deflected.
const MIN_THRESHOLD: f32 = 1e-3;

/// The zone whose test passes highest: `passes[i]` says whether the value reaches
/// the low (0), medium (1) and high (2) threshold.
fn band(passes: [bool; 3]) -> Option<Zone> {
    match passes {
        [_, _, true] => Some(Zone::High),
        [_, true, _] => Some(Zone::Med),
        [true, ..] => Some(Zone::Low),
        _ => None,
    }
}

/// Zone of a trigger pull for its low, medium and high thresholds; `None` below the
/// low threshold. A pull exactly on a threshold is in the higher zone.
pub fn trigger_zone(v: f32, t: [f32; 3]) -> Option<Zone> {
    band(t.map(|t| v >= t.max(MIN_THRESHOLD)))
}

/// Zone of an oriented stick vector. Each threshold is an ellipse (a circle when
/// both radii are equal); the stick reaches it when `(x/rx)² + (y/ry)² ≥ 1`.
pub fn stick_zone((x, y): (f32, f32), t: [ZoneRadius; 3]) -> Option<Zone> {
    band(t.map(|r| {
        let (mx, my) = (x / r.x.max(MIN_THRESHOLD), y / r.y.max(MIN_THRESHOLD));
        mx * mx + my * my >= 1.0
    }))
}

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

/// Which direction controls are active for a vector (indexed by [`Dir`]), or none while
/// it is shorter than `threshold`.
pub fn directions((x, y): (f32, f32), threshold: f32, k: f32, diagonals: bool) -> [bool; 8] {
    if (x * x + y * y).sqrt() < threshold.max(1e-4) {
        return [false; 8];
    }
    sectors((x, y), k, diagonals)
}

/// Which directions a deflected stick points to (indexed by [`Dir`]).
///
/// Within `atan(k)` of an axis only that cardinal is active. In between, both cardinals
/// are (4-way), or with `diagonals` only the diagonal (8-way).
pub fn sectors((x, y): (f32, f32), k: f32, diagonals: bool) -> [bool; 8] {
    let mut d = [false; 8];
    if x == 0.0 && y == 0.0 {
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
    } else if diagonals {
        d[Dir::diagonal(v, h) as usize] = true;
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
        dirs_with(v, false)
    }

    fn dirs_with(v: (f32, f32), diagonals: bool) -> Vec<Dir> {
        let d = directions(v, 7849.0 / 32768.0, K, diagonals);
        Dir::ALL8.into_iter().filter(|x| d[*x as usize]).collect()
    }

    #[test]
    fn spec_eight_way_vectors() {
        let n = |v: f32| v / 32768.0;
        assert_eq!(dirs_with((n(30000.0), n(10000.0)), true), vec![Dir::Right]);
        assert_eq!(
            dirs_with((n(20000.0), n(20000.0)), true),
            vec![Dir::UpRight]
        );
        assert_eq!(dirs_with((-0.7, -0.6), true), vec![Dir::DownLeft]);
        assert_eq!(dirs_with((0.0, -1.0), true), vec![Dir::Down]);
        // A narrower cardinal sector (10° each side) leaves more room for diagonals.
        let k10 = 10f32.to_radians().tan();
        let d = sectors((1.0, 0.25), k10, true);
        assert!(
            d[Dir::UpRight as usize],
            "14° off the axis is diagonal at 10°"
        );
        assert!(sectors((1.0, 0.25), K, true)[Dir::Right as usize]);
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
    fn derived_zone_thresholds_match_the_defaults() {
        let n = |v: f32| v / 32768.0;
        let trigger = TriggerConfig {
            zones: crate::profile::TriggerZones {
                low: n(960.0),
                ..Default::default()
            },
            full_pull: 1.0,
            ..Default::default()
        };
        // The integer defaults round each third down, so they sit up to two steps lower.
        let [low, med, high] = trigger.zone_thresholds();
        assert_eq!(low, n(960.0));
        assert!((med - n(11562.0)).abs() <= n(2.0), "{med}");
        assert!((high - n(22164.0)).abs() <= n(2.0), "{high}");

        let stick = StickConfig::default();
        let [low, med, high] = stick.zone_thresholds();
        let m = |v: f32| v / 32767.0;
        assert!((low.x - m(7849.0)).abs() < 0.001);
        assert!((med.x - m(16155.0)).abs() < 0.001, "{med:?}");
        assert!((high.y - m(24461.0)).abs() < 0.001, "{high:?}");
    }

    #[test]
    fn trigger_bands_are_exclusive_and_thresholds_belong_to_the_higher_band() {
        let t = [0.1, 0.4, 0.7];
        assert_eq!(trigger_zone(0.0, t), None);
        assert_eq!(trigger_zone(0.099, t), None);
        assert_eq!(trigger_zone(0.1, t), Some(Zone::Low));
        assert_eq!(trigger_zone(0.399, t), Some(Zone::Low));
        assert_eq!(trigger_zone(0.4, t), Some(Zone::Med));
        assert_eq!(trigger_zone(0.7, t), Some(Zone::High));
        assert_eq!(trigger_zone(1.0, t), Some(Zone::High));
        assert_eq!(
            trigger_zone(0.0, [0.0, 0.4, 0.7]),
            None,
            "a zero threshold does not count a released trigger"
        );
    }

    #[test]
    fn stick_bands_radial_and_elliptic() {
        let round = StickConfig::default().zone_thresholds();
        let n = |v: f32| v / 32768.0;
        assert_eq!(stick_zone((n(5000.0), n(5000.0)), round), None, "m = 0.90");
        assert_eq!(stick_zone((0.0, round[0].y), round), Some(Zone::Low));
        assert_eq!(stick_zone((-round[1].x, 0.0), round), Some(Zone::Med));
        assert_eq!(stick_zone((0.6, -0.6), round), Some(Zone::High));

        let r = |x, y| ZoneRadius { x, y };
        let ellipse = [r(0.2, 0.5), r(0.4, 0.7), r(0.6, 0.9)];
        assert_eq!(stick_zone((0.3, 0.0), ellipse), Some(Zone::Low));
        assert_eq!(
            stick_zone((0.0, 0.3), ellipse),
            None,
            "0.3 < 0.5 vertically"
        );
        assert_eq!(stick_zone((0.0, 0.8), ellipse), Some(Zone::Med));
        assert_eq!(stick_zone((0.45, 0.45), ellipse), Some(Zone::Med));
        assert_eq!(stick_zone((0.5, 0.5), ellipse), Some(Zone::High));
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
