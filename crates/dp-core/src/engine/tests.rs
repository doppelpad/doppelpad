//! Scenario tests: a fake 1 ms clock drives the engine through button timelines.

use super::*;
use crate::profile::{
    Action, AnalogMapping, AnalogTarget, Combo, Jump, JumpMode, Layer, Mapping, Press,
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

    /// Advance the clock to `t`, ticking every millisecond.
    fn until(&mut self, t: u64) {
        while self.t < t {
            self.t += 1;
            self.tick();
        }
    }

    fn btn(&mut self, b: PadButton, down: bool) {
        self.pad.set(b, down);
        self.t += 1;
        self.tick();
    }

    fn key(&mut self, k: Key, down: bool) {
        self.pending_kbm.push(KbmEvent::Key { key: k, down });
        self.t += 1;
        self.tick();
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

#[test]
fn simple_remap_and_passthrough() {
    let mut p = Profile::default();
    p.mappings.push(map(b(PadButton::A), vec![key(Key::Space)]));
    let mut h = Harness::new(p);

    h.btn(PadButton::A, true);
    h.btn(PadButton::B, true);
    assert_eq!(h.key_events(Key::Space), vec![(1, true)]);
    let v = h.virtual_pad();
    assert!(!v.pressed(PadButton::A), "mapped button is not mirrored");
    assert!(v.pressed(PadButton::B), "unmapped button is mirrored");

    h.btn(PadButton::A, false);
    assert_eq!(h.key_events(Key::Space), vec![(1, true), (3, false)]);
}

#[test]
fn remap_to_other_pad_button() {
    let mut p = Profile::default();
    p.mappings.push(map(
        b(PadButton::A),
        vec![Action::Pad {
            pad: PadButton::B,
            action: Press::Auto,
        }],
    ));
    let mut h = Harness::new(p);
    h.btn(PadButton::A, true);
    assert!(h.virtual_pad().pressed(PadButton::B) && !h.virtual_pad().pressed(PadButton::A));
    h.btn(PadButton::A, false);
    assert!(!h.virtual_pad().pressed(PadButton::B));
}

#[test]
fn combo_wins_within_window() {
    // LB+A = X, A = Y, window 50 ms.
    let mut p = Profile::default();
    p.mappings.push(map(
        Combo(vec![b(PadButton::LB), b(PadButton::A)]),
        vec![key(Key::X)],
    ));
    p.mappings.push(map(b(PadButton::A), vec![key(Key::Y)]));
    let mut h = Harness::new(p);

    h.until(100);
    h.btn(PadButton::LB, true); // t=101
    h.until(130);
    h.btn(PadButton::A, true); // t=131
    assert_eq!(h.key_events(Key::X), vec![(131, true)]);
    assert!(
        !h.virtual_pad().pressed(PadButton::LB),
        "consumed members are not mirrored"
    );
    h.btn(PadButton::A, false);
    h.btn(PadButton::LB, false);
    h.until(400);
    assert_eq!(h.key_events(Key::X), vec![(131, true), (132, false)]);
    assert!(h.key_events(Key::Y).is_empty());
}

#[test]
fn combo_member_alone_is_replayed_after_window() {
    let mut p = Profile::default();
    p.mappings.push(map(
        Combo(vec![b(PadButton::LB), b(PadButton::A)]),
        vec![key(Key::X)],
    ));
    p.mappings.push(map(b(PadButton::A), vec![key(Key::Y)]));
    let mut h = Harness::new(p);
    h.until(100);
    h.btn(PadButton::A, true); // 101
    h.until(300);
    h.btn(PadButton::A, false); // 301
    // Fires when the 50 ms window closes, stamped with the original press time.
    assert_eq!(h.key_events(Key::Y), vec![(152, true), (301, false)]);
    assert!(h.key_events(Key::X).is_empty());
}

#[test]
fn combo_member_without_own_mapping_still_passes_through() {
    let mut p = Profile::default();
    p.mappings.push(map(
        Combo(vec![b(PadButton::LB), b(PadButton::RB)]),
        vec![key(Key::Escape)],
    ));
    let mut h = Harness::new(p);
    h.btn(PadButton::LB, true);
    assert!(
        !h.virtual_pad().pressed(PadButton::LB),
        "delayed while the combo window is open"
    );
    h.until(100);
    assert!(
        h.virtual_pad().pressed(PadButton::LB),
        "mirrored once the window expires"
    );
}

#[test]
fn long_and_single_on_same_button() {
    let mut p = Profile::default();
    p.mappings.push(map(b(PadButton::A), vec![key(Key::S)]));
    let mut long = map(b(PadButton::A), vec![key(Key::L)]);
    long.activator = Activator::Long;
    p.mappings.push(long);
    let mut h = Harness::new(p);

    h.until(10);
    h.btn(PadButton::A, true); // 11
    h.until(130);
    h.btn(PadButton::A, false); // 131 -> short press: single pulse of 10 ms
    h.until(200);
    assert_eq!(h.key_events(Key::S), vec![(131, true), (141, false)]);
    assert!(h.key_events(Key::L).is_empty());

    h.btn(PadButton::A, true); // 201
    h.until(700);
    h.btn(PadButton::A, false); // 701
    assert_eq!(h.key_events(Key::L), vec![(501, true), (701, false)]);
    assert_eq!(
        h.key_events(Key::S).len(),
        2,
        "no single for the long press"
    );
}

#[test]
fn double_tap() {
    let mut p = Profile::default();
    p.mappings.push(map(b(PadButton::A), vec![key(Key::S)]));
    let mut dbl = map(b(PadButton::A), vec![key(Key::D)]);
    dbl.activator = Activator::Double;
    p.mappings.push(dbl);
    let mut h = Harness::new(p);
    h.btn(PadButton::A, true); // 1
    h.until(50);
    h.btn(PadButton::A, false); // 51
    h.until(100);
    h.btn(PadButton::A, true); // 101
    h.until(150);
    h.btn(PadButton::A, false); // 151
    h.until(500);
    assert_eq!(h.key_events(Key::D), vec![(101, true), (151, false)]);
    assert!(h.key_events(Key::S).is_empty());
}

#[test]
fn hold_layer_switch() {
    let mut p = Profile::default();
    p.layers.push(Layer {
        id: 1,
        name: "Shift".into(),
        ..Default::default()
    });
    let mut shift = map(b(PadButton::Back), vec![]);
    shift.jump = Some(Jump {
        layer: 1,
        mode: JumpMode::Hold,
        delay_ms: None,
    });
    p.mappings.push(shift);
    p.mappings.push(map(b(PadButton::A), vec![key(Key::Space)]));
    let mut f5 = map(b(PadButton::A), vec![key(Key::F5)]);
    f5.layer = 1;
    p.mappings.push(f5);
    p.mappings
        .push(map(b(PadButton::B), vec![key(Key::Escape)])); // inherited into layer 1
    let mut h = Harness::new(p);

    h.btn(PadButton::Back, true);
    assert_eq!(h.e.layer(), 1);
    assert!(h.last.layer_changed);
    h.btn(PadButton::A, true);
    h.btn(PadButton::A, false);
    h.btn(PadButton::B, true);
    h.btn(PadButton::B, false);
    h.btn(PadButton::Back, false);
    h.until(50);
    assert_eq!(h.e.layer(), 0, "release returns to the base layer");
    h.btn(PadButton::A, true);
    h.btn(PadButton::A, false);

    assert_eq!(h.key_events(Key::F5).len(), 2);
    assert_eq!(h.key_events(Key::Escape).len(), 2, "base mapping inherited");
    assert_eq!(
        h.key_events(Key::Space).len(),
        2,
        "base mapping again after return"
    );
    assert!(!h.virtual_pad().pressed(PadButton::Back));
}

#[test]
fn toggle_layer_switch() {
    let mut p = Profile::default();
    let mut shift = map(b(PadButton::Back), vec![]);
    shift.jump = Some(Jump {
        layer: 2,
        mode: JumpMode::Toggle,
        delay_ms: None,
    });
    p.mappings.push(shift);
    let mut h = Harness::new(p);
    h.btn(PadButton::Back, true);
    h.btn(PadButton::Back, false);
    assert_eq!(h.e.layer(), 2);
    h.until(20);
    h.btn(PadButton::Back, true);
    h.btn(PadButton::Back, false);
    assert_eq!(h.e.layer(), 0);
}

#[test]
fn held_button_is_repressed_in_new_layer() {
    let mut p = Profile::default();
    let mut shift = map(b(PadButton::Back), vec![]);
    shift.jump = Some(Jump {
        layer: 1,
        mode: JumpMode::Hold,
        delay_ms: None,
    });
    p.mappings.push(shift);
    p.mappings.push(map(b(PadButton::A), vec![key(Key::Space)]));
    let mut f5 = map(b(PadButton::A), vec![key(Key::F5)]);
    f5.layer = 1;
    p.mappings.push(f5);
    let mut h = Harness::new(p);
    h.btn(PadButton::A, true); // Space down
    h.btn(PadButton::Back, true); // switch: Space up, F5 down
    assert_eq!(h.key_events(Key::Space), vec![(1, true), (2, false)]);
    assert_eq!(h.key_events(Key::F5), vec![(2, true)]);
}

#[test]
fn turbo_pulses_while_held() {
    let mut p = Profile::default();
    let mut m = map(
        Control::TriggerPress(Trigger::Right),
        vec![Action::Mouse {
            mouse: MouseButton::Left,
            action: Press::Auto,
        }],
    );
    m.mode = Mode::Turbo;
    m.turbo_ms = Some(100);
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.pad.set_axis(Axis::RightTrigger, 1.0);
    h.until(1); // press at 1
    h.until(260);
    h.pad.set_axis(Axis::RightTrigger, 0.0);
    h.until(300);
    let ev: Vec<(u64, bool)> = h.mouse.iter().map(|(t, _, d)| (*t, *d)).collect();
    assert_eq!(
        ev,
        vec![
            (1, true),
            (51, false),
            (101, true),
            (151, false),
            (201, true),
            (251, false)
        ]
    );
    assert_eq!(
        h.virtual_pad().axis(Axis::RightTrigger),
        0.0,
        "mapped trigger is not mirrored"
    );
}

#[test]
fn once_macro_runs_to_completion() {
    let mut p = Profile::default();
    let mut m = map(
        b(PadButton::Y),
        vec![
            Action::Key {
                key: Key::LeftCtrl,
                action: Press::Down,
            },
            key(Key::C),
            Action::Pause { pause: 100 },
            Action::Key {
                key: Key::LeftCtrl,
                action: Press::Up,
            },
        ],
    );
    m.mode = Mode::Once;
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.btn(PadButton::Y, true); // 1
    h.btn(PadButton::Y, false); // 2: release does not cancel
    h.until(300);
    // Ctrl down @1, key delay 10, C tap 10 ms, pause 100, key delay is not added next to pauses.
    assert_eq!(h.key_events(Key::LeftCtrl), vec![(1, true), (121, false)]);
    assert_eq!(h.key_events(Key::C), vec![(11, true), (21, false)]);
}

#[test]
fn toggle_mode_latches() {
    let mut p = Profile::default();
    let mut m = map(b(PadButton::X), vec![key(Key::LeftShift)]);
    m.mode = Mode::Toggle;
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.btn(PadButton::X, true);
    h.btn(PadButton::X, false);
    h.until(50);
    assert!(h.e.is_output_down(Key::LeftShift));
    h.btn(PadButton::X, true);
    h.btn(PadButton::X, false);
    assert!(!h.e.is_output_down(Key::LeftShift));
    assert_eq!(h.key_events(Key::LeftShift), vec![(1, true), (51, false)]);
}

#[test]
fn repeat_mode() {
    let mut p = Profile::default();
    let mut m = map(b(PadButton::A), vec![key(Key::E)]);
    m.mode = Mode::Repeat;
    m.repeat = Some(3);
    m.repeat_pause_ms = Some(40);
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.btn(PadButton::A, true);
    h.btn(PadButton::A, false);
    h.until(500);
    let downs: Vec<u64> = h
        .key_events(Key::E)
        .into_iter()
        .filter(|e| e.1)
        .map(|e| e.0)
        .collect();
    assert_eq!(downs, vec![1, 51, 101]);
}

#[test]
fn keyboard_to_stick_and_mute() {
    let mut p = Profile::default();
    p.mappings.push(map(
        Control::Key(Key::W),
        vec![Action::Stick {
            stick: Stick::Left,
            x: 0.0,
            y: 1.0,
            action: Press::Auto,
        }],
    ));
    p.mappings.push(map(
        Control::Key(Key::D),
        vec![Action::Stick {
            stick: Stick::Left,
            x: 1.0,
            y: 0.0,
            action: Press::Auto,
        }],
    ));
    let mut h = Harness::new(p);
    assert_eq!(
        h.e.mutes().keys,
        vec![Key::W, Key::D]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    );
    h.key(Key::W, true);
    h.key(Key::D, true);
    let (x, y) = h.virtual_pad().stick(Stick::Left);
    assert_eq!((x, y), (1.0, 1.0));
    h.key(Key::W, false);
    assert_eq!(h.virtual_pad().stick(Stick::Left), (1.0, 0.0));
}

#[test]
fn quick_key_tap_within_one_tick_is_not_lost() {
    let mut p = Profile::default();
    p.mappings.push(map(
        Control::Key(Key::Q),
        vec![Action::Pad {
            pad: PadButton::A,
            action: Press::Auto,
        }],
    ));
    let mut h = Harness::new(p);
    h.pending_kbm.push(KbmEvent::Key {
        key: Key::Q,
        down: true,
    });
    h.pending_kbm.push(KbmEvent::Key {
        key: Key::Q,
        down: false,
    });
    h.t += 1;
    h.tick();
    // The tap pressed and released A within the tick; outputs were emitted and cleared.
    assert!(!h.virtual_pad().pressed(PadButton::A));
}

#[test]
fn mouse_to_stick_route_mutes_cursor() {
    let mut p = Profile::default();
    p.analog.push(AnalogMapping {
        source: AnalogSource::Mouse,
        target: AnalogTarget::RightStick,
        ..Default::default()
    });
    let mut h = Harness::new(p);
    assert!(h.e.mutes().mouse_move);
    h.pending_kbm.push(KbmEvent::MouseMove { dx: 2, dy: 0 });
    h.t += 1;
    h.tick();
    let (x, _) = h.virtual_pad().stick(Stick::Right);
    assert!(
        x > 0.9,
        "2 counts in 1 ms = 2000 counts/s = full deflection, got {x}"
    );
}

#[test]
fn stop_combo_releases_everything() {
    let mut p = Profile::default();
    let mut m = map(b(PadButton::X), vec![key(Key::LeftShift)]);
    m.mode = Mode::Toggle;
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.btn(PadButton::X, true);
    h.btn(PadButton::X, false);
    assert!(h.e.is_output_down(Key::LeftShift));
    for btn in STOP_COMBO {
        h.pad.set(btn, true);
    }
    h.t += 1;
    h.tick();
    assert!(!h.e.is_output_down(Key::LeftShift));
}

#[test]
fn pre_held_buttons_do_not_fire() {
    let mut p = Profile::default();
    p.mappings.push(map(b(PadButton::A), vec![key(Key::Space)]));
    let mut e = Engine::new(p);
    let mut pad = PadState::default();
    pad.set(PadButton::A, true);
    let out = e.tick(Input {
        now_ms: 0,
        pad: Some(&pad),
        kbm: &[],
    });
    assert!(out.keys.is_empty());
    pad.set(PadButton::A, false);
    let out = e.tick(Input {
        now_ms: 1,
        pad: Some(&pad),
        kbm: &[],
    });
    assert!(
        out.keys.is_empty(),
        "release of a pre-held button fires nothing"
    );
}

#[test]
fn stick_passthrough_and_direction_mapping() {
    let mut p = Profile::default();
    p.mappings.push(map(
        Control::StickDir(Stick::Left, Dir::Up),
        vec![key(Key::W)],
    ));
    let mut h = Harness::new(p);
    h.pad.set_stick(Stick::Left, (0.0, 0.9));
    h.pad.set_stick(Stick::Right, (0.5, 0.0));
    h.until(2);
    assert_eq!(h.key_events(Key::W), vec![(1, true)]);
    let v = h.virtual_pad();
    assert_eq!(
        v.stick(Stick::Left),
        (0.0, 0.0),
        "stick with direction mappings is not mirrored"
    );
    assert_eq!(v.stick(Stick::Right), (0.5, 0.0));
}

#[test]
fn release_all_clears_outputs() {
    let mut p = Profile::default();
    p.mappings.push(map(b(PadButton::A), vec![key(Key::Space)]));
    let mut h = Harness::new(p);
    h.btn(PadButton::A, true);
    let out = h.e.release_all();
    assert_eq!(out.keys, vec![(Key::Space, false)]);
}

#[test]
fn wheel_notches_trigger_mappings() {
    let mut p = Profile::default();
    p.mappings.push(map(
        Control::Wheel(WheelDir::Up),
        vec![Action::Pad {
            pad: PadButton::Y,
            action: Press::Tap,
        }],
    ));
    let mut h = Harness::new(p);
    h.pending_kbm.push(KbmEvent::Wheel {
        dir: WheelDir::Up,
        detents: 1.0,
    });
    h.t += 1;
    h.tick();
    assert!(h.virtual_pad().pressed(PadButton::Y));
    h.until(100);
    assert!(!h.virtual_pad().pressed(PadButton::Y));
}
