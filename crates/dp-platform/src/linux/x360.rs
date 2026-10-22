//! The virtual Xbox 360 pad's side of uinput: its layout as xpad registers it, the events
//! that report a [`PadState`], and the force-feedback mixer behind its rumble. No I/O, so it
//! is unit-tested on every OS.

use dp_core::pad::{Axis, PadButton, PadState, Rumble, axis_to_i16, unit_to_u8_round};

use super::codes::*;
use crate::MICROSOFT_VID;

/// Name, ids and layout of a wired Xbox 360 pad as xpad registers it.
pub const X360_NAME: &str = "Microsoft X-Box 360 pad";
pub const X360_VID: u16 = MICROSOFT_VID;
pub const X360_PID: u16 = 0x028E;
pub const X360_VERSION: u16 = 0x0114;

/// Buttons of the virtual Xbox 360 pad. xpad reports by label, so the left "X" button is
/// `BTN_X` (= `BTN_NORTH`) and the top "Y" button `BTN_Y` (= `BTN_WEST`).
pub const X360_KEYS: [(PadButton, u16); 11] = [
    (PadButton::A, BTN_SOUTH),
    (PadButton::B, BTN_EAST),
    (PadButton::X, BTN_NORTH),
    (PadButton::Y, BTN_WEST),
    (PadButton::LB, BTN_TL),
    (PadButton::RB, BTN_TR),
    (PadButton::Back, BTN_SELECT),
    (PadButton::Start, BTN_START),
    (PadButton::Guide, BTN_MODE),
    (PadButton::LS, BTN_THUMBL),
    (PadButton::RS, BTN_THUMBR),
];

/// Absolute axes of the virtual Xbox 360 pad: `(code, min, max, fuzz, flat)` as in
/// xpad's `xpad_set_up_abs`.
pub const X360_AXES: [(u16, i32, i32, i32, i32); 8] = [
    (ABS_X, -32768, 32767, 16, 128),
    (ABS_Y, -32768, 32767, 16, 128),
    (ABS_RX, -32768, 32767, 16, 128),
    (ABS_RY, -32768, 32767, 16, 128),
    (ABS_Z, 0, 255, 0, 0),
    (ABS_RZ, 0, 255, 0, 0),
    (ABS_HAT0X, -1, 1, 0, 0),
    (ABS_HAT0Y, -1, 1, 0, 0),
];

pub const X360_EVENTS: usize = X360_KEYS.len() + X360_AXES.len();

/// The `(type, code, value)` of every key and axis a real Xbox 360 pad would report in `s`.
pub fn x360_events(s: &PadState) -> [(u16, u16, i32); X360_EVENTS] {
    let trigger = |a| i32::from(unit_to_u8_round(s.axis(a)));
    let dpad = |neg, pos| i32::from(s.pressed(pos)) - i32::from(s.pressed(neg));
    let values = [
        i32::from(axis_to_i16(s.axis(Axis::LeftX))),
        i32::from(axis_to_i16(-s.axis(Axis::LeftY))), // evdev: up is negative
        i32::from(axis_to_i16(s.axis(Axis::RightX))),
        i32::from(axis_to_i16(-s.axis(Axis::RightY))),
        trigger(Axis::LeftTrigger),
        trigger(Axis::RightTrigger),
        dpad(PadButton::DpadLeft, PadButton::DpadRight),
        dpad(PadButton::DpadUp, PadButton::DpadDown),
    ];
    let mut out = [(0, 0, 0); X360_EVENTS];
    for (o, &(b, code)) in out.iter_mut().zip(&X360_KEYS) {
        *o = (EV_KEY, code, i32::from(s.pressed(b)));
    }
    for (o, (&(code, ..), v)) in out[X360_KEYS.len()..]
        .iter_mut()
        .zip(X360_AXES.iter().zip(values))
    {
        *o = (EV_ABS, code, v);
    }
    out
}

#[derive(Copy, Clone, Debug)]
struct Effect {
    id: i16,
    strong: u16,
    weak: u16,
    length_ms: u16,
    delay_ms: u16,
    /// Start time and repeat count while playing.
    playing: Option<(u64, i32)>,
}

/// Rumble effects games upload to the virtual pad (`FF_RUMBLE` through uinput), and which
/// of them are playing. Times are milliseconds on any monotonic clock.
#[derive(Clone, Debug)]
pub struct FfMixer {
    effects: Vec<Effect>,
    gain: u16,
}

impl Default for FfMixer {
    fn default() -> Self {
        FfMixer {
            effects: Vec::new(),
            gain: u16::MAX,
        }
    }
}

impl FfMixer {
    /// A game uploaded (or updated) rumble effect `id`.
    pub fn upload(&mut self, id: i16, strong: u16, weak: u16, length_ms: u16, delay_ms: u16) {
        let new = Effect {
            id,
            strong,
            weak,
            length_ms,
            delay_ms,
            playing: None,
        };
        match self.effects.iter_mut().find(|e| e.id == id) {
            Some(e) => {
                *e = Effect {
                    playing: e.playing,
                    ..new
                }
            }
            None => self.effects.push(new),
        }
    }

    pub fn erase(&mut self, id: i16) {
        self.effects.retain(|e| e.id != id);
    }

    /// `EV_FF` with code `id`: play `count` times, or stop if `count` is 0.
    pub fn play(&mut self, id: i16, count: i32, now_ms: u64) {
        if let Some(e) = self.effects.iter_mut().find(|e| e.id == id) {
            e.playing = (count > 0).then_some((now_ms, count));
        }
    }

    /// `EV_FF` / `FF_GAIN`: overall strength, `0..=0xFFFF`.
    pub fn set_gain(&mut self, gain: u16) {
        self.gain = gain;
    }

    /// Sum of all effects playing at `now_ms` (an effect of length 0 plays until stopped).
    pub fn rumble(&self, now_ms: u64) -> Rumble {
        let (mut strong, mut weak) = (0u32, 0u32);
        for e in &self.effects {
            let Some((start, count)) = e.playing else {
                continue;
            };
            let Some(t) = now_ms.checked_sub(start + u64::from(e.delay_ms)) else {
                continue;
            };
            if e.length_ms != 0 && t >= u64::from(e.length_ms) * count as u64 {
                continue;
            }
            strong += u32::from(e.strong);
            weak += u32::from(e.weak);
        }
        let gain = f32::from(self.gain) / 65535.0;
        Rumble {
            low: strong.min(65535) as f32 / 65535.0 * gain,
            high: weak.min(65535) as f32 / 65535.0 * gain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x360_encoding() {
        let neutral = x360_events(&PadState::default());
        assert!(neutral.iter().all(|&(_, _, v)| v == 0));

        let mut s = PadState::default();
        s.set(PadButton::X, true);
        s.set(PadButton::DpadUp, true);
        s.set_axis(Axis::LeftY, 1.0);
        s.set_axis(Axis::RightTrigger, 1.0);
        let ev = x360_events(&s);
        let get = |ty, code| ev.iter().find(|e| e.0 == ty && e.1 == code).unwrap().2;
        assert_eq!(get(EV_KEY, BTN_NORTH), 1, "Xbox X is BTN_X");
        assert_eq!(get(EV_KEY, BTN_WEST), 0);
        assert_eq!(get(EV_ABS, ABS_Y), -32768);
        assert_eq!(get(EV_ABS, ABS_RZ), 255);
        assert_eq!(get(EV_ABS, ABS_HAT0Y), -1);
    }

    #[test]
    fn rumble_mixing() {
        let mut ff = FfMixer::default();
        ff.upload(0, 0xFFFF, 0, 100, 0);
        ff.upload(1, 0, 0x8000, 0, 50);
        assert_eq!(ff.rumble(0), Rumble::default());
        ff.play(0, 1, 1000);
        ff.play(1, 1, 1000);
        let r = ff.rumble(1010);
        assert_eq!((r.low, r.high), (1.0, 0.0), "effect 1 is still delayed");
        let r = ff.rumble(1060);
        assert!(r.low == 1.0 && (r.high - 0.5).abs() < 1e-3);
        let r = ff.rumble(1100);
        assert_eq!(r.low, 0.0, "effect 0 ran out");
        ff.set_gain(0x8000);
        assert!((ff.rumble(5000).high - 0.25).abs() < 1e-3);
        ff.play(1, 0, 5000);
        assert_eq!(ff.rumble(5001), Rumble::default());
        ff.erase(1);
        ff.play(1, 1, 6000);
        assert_eq!(ff.rumble(6001), Rumble::default());
    }
}
