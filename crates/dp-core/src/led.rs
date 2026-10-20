//! Light-bar animations: the colour a [`Light`] shows at a point in time.

use std::f32::consts::TAU;

use crate::pad::Rgb;
use crate::profile::{Light, LightMode};

/// `#rrggbb` as a colour.
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Rgb {
        r: (v >> 16) as u8,
        g: (v >> 8) as u8,
        b: v as u8,
    })
}

fn scale(c: Rgb, k: f32) -> Rgb {
    let f = |v: u8| (v as f32 * k.clamp(0.0, 1.0)).round() as u8;
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

/// The colour `light` shows `t_ms` after it started; `None` when it shows nothing of its own.
pub fn color_at(light: &Light, t_ms: u64) -> Option<Rgb> {
    let colors: Vec<Rgb> = light.colors.iter().filter_map(|c| parse_hex(c)).collect();
    let period = light.period_ms.max(100) as u64;
    let cycle = (t_ms / period) as usize;
    let phase = (t_ms % period) as f32 / period as f32;
    let base = match colors.len() {
        0 => Rgb {
            r: 255,
            g: 255,
            b: 255,
        },
        n => colors[if light.cycle { cycle % n } else { 0 }],
    };
    Some(match light.mode {
        LightMode::Off => return None,
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
                Rgb { r: 0, g: 0, b: 0 }
            }
        }
    })
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
        assert_eq!(
            color_at(&light(LightMode::Solid, &["#ff0000"]), 0),
            Some(red)
        );
        let breath = light(LightMode::Breath, &["#ff0000"]);
        assert_eq!(color_at(&breath, 0), Some(Rgb { r: 0, g: 0, b: 0 }));
        assert_eq!(color_at(&breath, 500), Some(red), "full at half the period");
        let mut cycling = light(LightMode::Solid, &["#ff0000", "#0000ff"]);
        cycling.cycle = true;
        assert_eq!(color_at(&cycling, 1500), Some(Rgb { r: 0, g: 0, b: 255 }));
        assert_eq!(color_at(&light(LightMode::Off, &["#ff0000"]), 0), None);
    }

    #[test]
    fn rainbow_and_strobe() {
        let rainbow = light(LightMode::Rainbow, &[]);
        assert_eq!(color_at(&rainbow, 0), Some(Rgb { r: 255, g: 0, b: 0 }));
        assert_eq!(
            color_at(&rainbow, 333).unwrap().g,
            255,
            "green a third of the way"
        );
        let strobe = light(LightMode::Strobe, &["#ffffff"]);
        assert_eq!(color_at(&strobe, 10).unwrap().r, 255);
        assert_eq!(color_at(&strobe, 100).unwrap().r, 0);
    }

    #[test]
    fn hex_colours() {
        assert_eq!(
            parse_hex("#0a1B2c"),
            Some(Rgb {
                r: 10,
                g: 27,
                b: 44
            })
        );
        assert_eq!(parse_hex("0a1b2c"), None);
        assert_eq!(parse_hex("#0a1b2"), None);
    }
}
