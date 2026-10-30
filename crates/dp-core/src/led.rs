//! Light-bar animations: the colour a [`Light`] shows at a point in time.

use std::f32::consts::TAU;

use crate::pad::Rgb;
use crate::profile::{Light, LightMode};

fn scale(c: Rgb, k: f32) -> Rgb {
    let f = |v: u8| (f32::from(v) * k.clamp(0.0, 1.0)).round() as u8;
    Rgb {
        r: f(c.r),
        g: f(c.g),
        b: f(c.b),
    }
}

/// Hue 0–1 at full saturation and brightness.
fn hue(h: f32) -> Rgb {
    let h = h.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    let f = |v: f32| (v * 255.0).round() as u8;
    Rgb {
        r: f(r),
        g: f(g),
        b: f(b),
    }
}

/// A short bump of brightness centred at `at` (fractions of a period), `width` wide.
fn pulse(phase: f32, at: f32, width: f32) -> f32 {
    let d = (phase - at).abs() / width;
    if d >= 1.0 {
        0.0
    } else {
        0.5 + 0.5 * (d * std::f32::consts::PI).cos()
    }
}

/// The colour `light` shows `t_ms` after it started (black when it is off; white for no
/// colour or one that is not `#rrggbb`, which the validator reports). Runs every tick, so it
/// reads only the colour it shows.
pub fn color_at(light: &Light, t_ms: u64) -> Rgb {
    let period = u64::from(light.period_ms.max(*Light::PERIOD_MS.start()));
    let cycle = (t_ms / period) as usize;
    let phase = (t_ms % period) as f32 / period as f32;
    let n = light.colors.len();
    let base = match n {
        0 => None,
        _ => Some(light.colors[if light.cycle { cycle % n } else { 0 }].as_str()),
    }
    .and_then(Rgb::from_hex)
    .unwrap_or(Rgb::WHITE);
    match light.mode {
        LightMode::Off => Rgb::BLACK,
        LightMode::Solid => base,
        LightMode::Breath => scale(base, 0.5 - 0.5 * (TAU * phase).cos()),
        LightMode::Rainbow => hue(phase),
        LightMode::Shimmer => {
            let wobble = (TAU * phase * 3.0).sin() * (TAU * phase * 7.0).sin();
            scale(base, 0.75 + 0.25 * wobble)
        }
        LightMode::Heartbeat => scale(
            base,
            0.15 + 0.85 * pulse(phase, 0.1, 0.08).max(0.7 * pulse(phase, 0.3, 0.08)),
        ),
        LightMode::Strobe => {
            if (phase * 8.0).fract() < 0.5 {
                base
            } else {
                Rgb::BLACK
            }
        }
    }
}

/// The time steps colours change in: lights refresh at most this often.
pub const STEP_MS: u64 = 20;

#[cfg(test)]
mod tests {
    use super::*;

    fn light(mode: LightMode, colors: &[&str]) -> Light {
        Light {
            mode,
            colors: colors.iter().map(|c| c.to_string()).collect(),
            cycle: false,
            period_ms: 1000,
        }
    }

    #[test]
    fn solid_breath_and_cycle() {
        let red = Rgb { r: 255, g: 0, b: 0 };
        assert_eq!(color_at(&light(LightMode::Solid, &["#ff0000"]), 0), red);
        let breath = light(LightMode::Breath, &["#ff0000"]);
        assert_eq!(color_at(&breath, 0), Rgb::BLACK);
        assert_eq!(color_at(&breath, 500), red, "full at half the period");
        let mut cycling = light(LightMode::Solid, &["#ff0000", "#0000ff"]);
        cycling.cycle = true;
        assert_eq!(color_at(&cycling, 1500), Rgb { r: 0, g: 0, b: 255 });
        assert_eq!(
            color_at(&light(LightMode::Off, &["#ff0000"]), 0),
            Rgb::BLACK
        );
    }

    #[test]
    fn rainbow_and_strobe() {
        let rainbow = light(LightMode::Rainbow, &[]);
        assert_eq!(color_at(&rainbow, 0), Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(color_at(&rainbow, 333).g, 255, "green a third of the way");
        let strobe = light(LightMode::Strobe, &["#ffffff"]);
        assert_eq!(color_at(&strobe, 10).r, 255);
        assert_eq!(color_at(&strobe, 100).r, 0);
    }

    #[test]
    fn hex_colours() {
        assert_eq!(
            Rgb::from_hex("#0a1B2c"),
            Some(Rgb {
                r: 10,
                g: 27,
                b: 44
            })
        );
        assert_eq!(Rgb::from_hex("0a1b2c"), None);
        assert_eq!(Rgb::from_hex("#0a1b2"), None);
        assert_eq!(Rgb::from_hex("#+12345"), None, "only hex digits");
    }
}
