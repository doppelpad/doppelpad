//! Scenario tests: a fake 1 ms clock drives the engine through button timelines.

mod combos;
mod layers;
mod modes;
mod patterns;
mod radial;
mod touch_gyro_mouse;
mod virtual_sticks;

use super::*;
use crate::control::{Dir, GyroDir, Half, TouchGesture, TouchRegion, TrackpadPart, Zone};
use crate::pad::{Axis, PadButton, Trigger};
use crate::profile::{
    Action, Activator, AnalogMapping, AnalogSource, AnalogTarget, Combo, Jump, JumpMode, Layer,
    Mapping, Mode, Press,
};

struct Harness {
    e: Engine,
    t: u64,
    pad: PadState,
    pending_kbm: Vec<KbmEvent>,
    keys: Vec<(u64, Key, bool)>,
    mouse: Vec<(u64, MouseButton, bool)>,
    mouse_moved: (f32, f32),
    last: Output,
}

impl Harness {
    fn new(p: Profile) -> Self {
        let mut h = Harness {
            e: Engine::new(p),
            t: 0,
            pad: PadState::default(),
            pending_kbm: Vec::new(),
            keys: Vec::new(),
            mouse: Vec::new(),
            mouse_moved: (0.0, 0.0),
            last: Output::default(),
        };
        h.tick();
        h
    }

    fn tick(&mut self) {
        let kbm = std::mem::take(&mut self.pending_kbm);
        let out = self.e.tick(Input {
            now_ms: self.t,
            pad: Some(&self.pad),
            kbm: &kbm,
        });
        for (k, d) in &out.keys {
            self.keys.push((self.t, *k, *d));
        }
        for (b, d) in &out.mouse_buttons {
            self.mouse.push((self.t, *b, *d));
        }
        self.mouse_moved.0 += out.mouse_move.0;
        self.mouse_moved.1 += out.mouse_move.1;
        self.last = out;
    }

    /// One millisecond later.
    fn step(&mut self) {
        self.t += 1;
        self.tick();
    }

    /// Advance the clock to `t`, ticking every millisecond.
    fn until(&mut self, t: u64) {
        while self.t < t {
            self.step();
        }
    }

    /// Keyboard and mouse events, delivered with the next tick.
    fn kbm(&mut self, events: impl IntoIterator<Item = KbmEvent>) {
        self.pending_kbm.extend(events);
        self.step();
    }

    fn btn(&mut self, b: PadButton, down: bool) {
        self.pad.set(b, down);
        self.step();
    }

    /// Press `b`, then release it on the next tick.
    fn tap(&mut self, b: PadButton) {
        self.btn(b, true);
        self.btn(b, false);
    }

    fn key(&mut self, k: Key, down: bool) {
        self.kbm([KbmEvent::Key { key: k, down }]);
    }

    fn virtual_pad(&self) -> PadState {
        self.last.pad.expect("virtual pad enabled")
    }

    fn key_events(&self, k: Key) -> Vec<(u64, bool)> {
        self.keys
            .iter()
            .filter(|(_, kk, _)| *kk == k)
            .map(|(t, _, d)| (*t, *d))
            .collect()
    }
}

fn key(k: Key) -> Action {
    Action::Key {
        key: k,
        action: Press::Auto,
    }
}

fn b(p: PadButton) -> Control {
    Control::Button(p)
}

fn map(trigger: impl Into<Combo>, actions: Vec<Action>) -> Mapping {
    Mapping::new(trigger, actions)
}

/// A jump to `layer` without a delay.
fn jump(layer: u8, mode: JumpMode) -> Jump {
    Jump {
        layer,
        mode,
        delay_ms: None,
        postpone: false,
    }
}

/// A mapping of `trigger` that only jumps.
fn shift(trigger: impl Into<Combo>, layer: u8, mode: JumpMode) -> Mapping {
    Mapping {
        jump: Some(jump(layer, mode)),
        ..map(trigger, Vec::new())
    }
}

/// Back shifts to layer 1 while held; A types Space in the base layer and F5 in layer 1.
fn shift_profile() -> Profile {
    let mut p = Profile::default();
    p.mappings
        .push(shift(b(PadButton::Back), 1, JumpMode::Hold));
    p.mappings.push(map(b(PadButton::A), vec![key(Key::Space)]));
    let mut f5 = map(b(PadButton::A), vec![key(Key::F5)]);
    f5.layer = 1;
    p.mappings.push(f5);
    p
}

fn tzone(t: Trigger, z: Zone) -> Control {
    Control::TriggerZone(t, z)
}

fn szone(s: Stick, d: Option<Dir>, z: Zone) -> Control {
    Control::StickZone(s, d, z)
}
