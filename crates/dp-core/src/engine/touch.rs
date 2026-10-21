//! Touchpad controls: the region a finger rests in (radial sectors or a 3 × 3 grid, on the
//! whole pad or on each half of a split pad) and gestures recognised from the finger tracks.

use super::analog::{DIRECTION_SECTOR, directions};
use crate::control::{Control, Dir, Half, TouchGesture, TouchRegion, TrackpadPart};
use crate::pad::TouchPoint;
use crate::profile::{TouchZones, TouchpadSettings};

/// A tap: finger down and up within this long ...
const TAP_MS: u64 = 200;
/// ... moving less than this (fraction of the pad).
const TAP_TRAVEL: f32 = 0.05;
/// A swipe: this much travel (fraction of the pad) ...
const SWIPE_TRAVEL: f32 = 0.25;
/// ... within this long from the first touch.
const SWIPE_MS: u64 = 500;
/// Zoom: two fingers' distance changes by this factor.
const ZOOM_RATIO: f32 = 1.2;
/// How long a gesture control stays pressed.
pub(super) const GESTURE_PRESS_MS: u64 = 50;
/// How long a played tap and a played swipe or zoom take on the virtual touchpad by default.
const TAP_OUTPUT_MS: u32 = 60;
const GESTURE_OUTPUT_MS: u32 = 250;

/// Regions for a finger at `(x, y)` (0–1, origin top left).
pub(super) fn regions(
    (x, y): (f32, f32),
    cfg: &TouchpadSettings,
    diagonals: bool,
    out: &mut impl Extend<TouchRegion>,
) {
    match cfg.zones {
        TouchZones::Radial => {
            let v = ((x - 0.5) * 2.0, (0.5 - y) * 2.0);
            // Four 90° regions, or eight 45° ones when diagonals are mapped.
            let k = if diagonals { DIRECTION_SECTOR } else { 1.0 };
            let d = directions(v, cfg.centre, k, diagonals);
            let mut any = false;
            for dir in Dir::ALL.into_iter().filter(|x| d[*x as usize]) {
                out.extend([TouchRegion::of(dir)]);
                any = true;
            }
            if !any {
                out.extend([TouchRegion::Center]);
            }
        }
        TouchZones::Grid => {
            let third = |v: f32| {
                if v < 1.0 / 3.0 {
                    -1
                } else if v > 2.0 / 3.0 {
                    1
                } else {
                    0
                }
            };
            let (col, row) = (third(x), -third(y));
            let h = match col {
                -1 => Some(Dir::Left),
                1 => Some(Dir::Right),
                _ => None,
            };
            let v = match row {
                1 => Some(Dir::Up),
                -1 => Some(Dir::Down),
                _ => None,
            };
            let r = match (v, h) {
                (None, None) => TouchRegion::Center,
                (Some(d), None) | (None, Some(d)) => TouchRegion::of(d),
                (Some(v), Some(h)) if diagonals => TouchRegion::of(Dir::diagonal(v, h)),
                // A corner without diagonal mappings counts for the nearer edge.
                (Some(v), Some(h)) => {
                    if (y - 0.5).abs() >= (x - 0.5).abs() {
                        TouchRegion::of(v)
                    } else {
                        TouchRegion::of(h)
                    }
                }
            };
            out.extend([r]);
        }
    }
}

/// How long a touch action plays when it gives no length.
pub(super) fn output_ms(g: TouchGesture) -> u32 {
    match g {
        TouchGesture::Tap
        | TouchGesture::TapLeft
        | TouchGesture::TapCenter
        | TouchGesture::TapRight
        | TouchGesture::MultiTap => TAP_OUTPUT_MS,
        _ => GESTURE_OUTPUT_MS,
    }
}

/// Fingers on the virtual touchpad `t` (0–1) of the way through a played gesture.
pub(super) fn output_fingers(g: TouchGesture, t: f32) -> [Option<TouchPoint>; 2] {
    use TouchGesture::*;
    let t = t.clamp(0.0, 1.0);
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    let p = |x: f32, y: f32| Some(TouchPoint { x, y });
    // Swipes run from 20 % to 80 % of the pad; two fingers sit 0.2 apart across the path.
    let swipe = |dir: Dir, two: bool| {
        let (from, to) = match dir {
            Dir::Up | Dir::Left => (0.8, 0.2),
            _ => (0.2, 0.8),
        };
        let along = lerp(from, to);
        let vertical = matches!(dir, Dir::Up | Dir::Down);
        let at = |across: f32| {
            if vertical {
                p(across, along)
            } else {
                p(along, across)
            }
        };
        if two {
            [at(0.4), at(0.6)]
        } else {
            [at(0.5), None]
        }
    };
    match g {
        Tap | TapCenter => [p(0.5, 0.5), None],
        TapLeft => [p(1.0 / 6.0, 0.5), None],
        TapRight => [p(5.0 / 6.0, 0.5), None],
        MultiTap => [p(0.4, 0.5), p(0.6, 0.5)],
        SwipeUp => swipe(Dir::Up, false),
        SwipeDown => swipe(Dir::Down, false),
        SwipeLeft => swipe(Dir::Left, false),
        SwipeRight => swipe(Dir::Right, false),
        Swipe2Up => swipe(Dir::Up, true),
        Swipe2Down => swipe(Dir::Down, true),
        Swipe2Left => swipe(Dir::Left, true),
        Swipe2Right => swipe(Dir::Right, true),
        // Pinching moves the fingers from the edges toward the middle; zooming in the reverse.
        ZoomOut => [p(lerp(0.1, 0.4), 0.5), p(lerp(0.9, 0.6), 0.5)],
        ZoomIn => [p(lerp(0.4, 0.1), 0.5), p(lerp(0.6, 0.9), 0.5)],
    }
}

/// Which half of the pad a finger is on, and where within it (0–1 again).
pub(super) fn half_of(p: TouchPoint) -> (Half, (f32, f32)) {
    if p.x < 0.5 {
        (Half::Left, (p.x * 2.0, p.y))
    } else {
        (Half::Right, ((p.x - 0.5) * 2.0, p.y))
    }
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    start: (f32, f32),
    pos: (f32, f32),
    travel: f32,
}

#[derive(Clone, Copy, Debug)]
struct Touch {
    start_ms: u64,
    start: (f32, f32),
    fingers: usize,
    /// Distance between the two fingers when the second one landed.
    spread: Option<f32>,
    /// A gesture was recognised during this touch.
    fired: bool,
    /// Every finger so far stayed within the tap travel.
    short: bool,
}

/// Follows the fingers from the first touch to the last lift and recognises gestures.
#[derive(Clone, Debug, Default)]
pub(super) struct GestureTracker {
    fingers: [Option<Finger>; 2],
    touch: Option<Touch>,
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

impl GestureTracker {
    /// Feed this tick's fingers; returns the gestures recognised now. With `split`, taps also
    /// report the trackpad half they were on.
    pub(super) fn update(
        &mut self,
        touch: [Option<TouchPoint>; 2],
        now: u64,
        split: bool,
    ) -> Vec<Control> {
        let mut out = Vec::new();
        for (slot, p) in self.fingers.iter_mut().zip(touch) {
            match (slot.as_mut(), p) {
                (None, Some(p)) => {
                    *slot = Some(Finger {
                        start: (p.x, p.y),
                        pos: (p.x, p.y),
                        travel: 0.0,
                    });
                }
                (Some(f), Some(p)) => {
                    f.pos = (p.x, p.y);
                    f.travel = f.travel.max(dist(f.start, f.pos));
                }
                (Some(f), None) => {
                    if let Some(t) = &mut self.touch {
                        t.short &= f.travel < TAP_TRAVEL;
                    }
                    *slot = None;
                }
                (None, None) => {}
            }
        }
        let down: Vec<Finger> = self.fingers.iter().flatten().copied().collect();
        if down.is_empty() {
            if let Some(t) = self.touch.take()
                && !t.fired
                && t.short
                && now.saturating_sub(t.start_ms) <= TAP_MS
            {
                if t.fingers >= 2 {
                    out.push(Control::Gesture(TouchGesture::MultiTap));
                } else {
                    let third = if t.start.0 < 1.0 / 3.0 {
                        TouchGesture::TapLeft
                    } else if t.start.0 > 2.0 / 3.0 {
                        TouchGesture::TapRight
                    } else {
                        TouchGesture::TapCenter
                    };
                    out.push(Control::Gesture(TouchGesture::Tap));
                    out.push(Control::Gesture(third));
                    if split {
                        let (half, _) = half_of(TouchPoint {
                            x: t.start.0,
                            y: t.start.1,
                        });
                        out.push(Control::Trackpad(half, TrackpadPart::Tap));
                    }
                }
            }
            return out;
        }
        let t = self.touch.get_or_insert(Touch {
            start_ms: now,
            start: down[0].start,
            fingers: 0,
            spread: None,
            fired: false,
            short: true,
        });
        t.fingers = t.fingers.max(down.len());
        t.short &= down.iter().all(|f| f.travel < TAP_TRAVEL);
        if down.len() == 2 && t.spread.is_none() {
            t.spread = Some(dist(down[0].pos, down[1].pos));
        }
        if t.fired {
            return out;
        }
        // Zoom: the two fingers spread or pinch.
        if let (2, Some(s)) = (down.len(), t.spread)
            && s > 0.01
        {
            let ratio = dist(down[0].pos, down[1].pos) / s;
            if ratio >= ZOOM_RATIO || ratio <= 1.0 / ZOOM_RATIO {
                let g = if ratio > 1.0 {
                    TouchGesture::ZoomIn
                } else {
                    TouchGesture::ZoomOut
                };
                out.push(Control::Gesture(g));
                t.fired = true;
                return out;
            }
        }
        // Swipe: the fingers travel far enough soon enough (y grows downwards on the pad).
        if now.saturating_sub(t.start_ms) <= SWIPE_MS {
            let n = down.len() as f32;
            let (dx, dy) = down.iter().fold((0.0, 0.0), |(x, y), f| {
                (x + (f.pos.0 - f.start.0) / n, y + (f.pos.1 - f.start.1) / n)
            });
            if dx.abs().max(dy.abs()) >= SWIPE_TRAVEL {
                let dir = if dx.abs() >= dy.abs() {
                    if dx > 0.0 { Dir::Right } else { Dir::Left }
                } else if dy > 0.0 {
                    Dir::Down
                } else {
                    Dir::Up
                };
                out.push(Control::Gesture(TouchGesture::swipe(dir, t.fingers >= 2)));
                t.fired = true;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32, y: f32) -> Option<TouchPoint> {
        Some(TouchPoint { x, y })
    }

    /// Play a recorded touch sequence (one entry per 10 ms) and collect the gestures.
    fn play(seq: &[[Option<TouchPoint>; 2]], split: bool) -> Vec<(u64, Control)> {
        let mut g = GestureTracker::default();
        let mut out = Vec::new();
        for (i, f) in seq.iter().enumerate() {
            let t = i as u64 * 10;
            out.extend(g.update(*f, t, split).into_iter().map(|c| (t, c)));
        }
        out
    }

    const UP: [Option<TouchPoint>; 2] = [None, None];

    #[test]
    fn taps_by_third_and_with_two_fingers() {
        let left = play(&[[at(0.1, 0.5), None], [at(0.11, 0.5), None], UP], false);
        assert_eq!(
            left,
            [
                (20, Control::Gesture(TouchGesture::Tap)),
                (20, Control::Gesture(TouchGesture::TapLeft))
            ]
        );
        let two = play(
            &[
                [at(0.3, 0.5), at(0.7, 0.5)],
                [at(0.3, 0.5), at(0.7, 0.5)],
                UP,
            ],
            false,
        );
        assert_eq!(two, [(20, Control::Gesture(TouchGesture::MultiTap))]);
        // Too long for a tap.
        let mut long = vec![[at(0.5, 0.5), None]; 30];
        long.push(UP);
        assert!(play(&long, false).is_empty());
        // Split: the tap also counts for its half.
        let right = play(&[[at(0.8, 0.5), None], UP], true);
        assert!(right.contains(&(10, Control::Trackpad(Half::Right, TrackpadPart::Tap))));
    }

    #[test]
    fn swipes_and_zoom() {
        let swipe: Vec<_> = (0..6)
            .map(|i| [at(0.5, 0.8 - i as f32 * 0.06), None])
            .chain([UP])
            .collect();
        assert_eq!(
            play(&swipe, false),
            [(50, Control::Gesture(TouchGesture::SwipeUp))]
        );
        let two: Vec<_> = (0..6)
            .map(|i| {
                let x = 0.3 + i as f32 * 0.06;
                [at(x, 0.4), at(x, 0.6)]
            })
            .chain([UP])
            .collect();
        assert_eq!(
            play(&two, false),
            [(50, Control::Gesture(TouchGesture::Swipe2Right))]
        );
        let pinch: Vec<_> = (0..5)
            .map(|i| {
                let d = 0.3 - i as f32 * 0.03;
                [at(0.5 - d, 0.5), at(0.5 + d, 0.5)]
            })
            .chain([UP])
            .collect();
        assert_eq!(
            play(&pinch, false),
            [(20, Control::Gesture(TouchGesture::ZoomOut))]
        );
    }

    #[test]
    fn grid_and_radial_regions() {
        let grid = TouchpadSettings {
            zones: TouchZones::Grid,
            ..Default::default()
        };
        let r = |p, cfg: &TouchpadSettings, diag| {
            let mut v = Vec::new();
            regions(p, cfg, diag, &mut v);
            v
        };
        assert_eq!(r((0.5, 0.5), &grid, false), [TouchRegion::Center]);
        assert_eq!(r((0.5, 0.1), &grid, false), [TouchRegion::Up]);
        assert_eq!(r((0.1, 0.1), &grid, true), [TouchRegion::UpLeft]);
        assert_eq!(
            r((0.05, 0.25), &grid, false),
            [TouchRegion::Left],
            "nearer edge"
        );
        let radial = TouchpadSettings::default();
        assert_eq!(r((0.5, 0.45), &radial, false), [TouchRegion::Center]);
        assert_eq!(r((0.9, 0.5), &radial, false), [TouchRegion::Right]);
        assert_eq!(
            half_of(TouchPoint { x: 0.75, y: 0.2 }),
            (Half::Right, (0.5, 0.2))
        );
    }
}
