//! Scenario tests: a fake 1 ms clock drives the engine through button timelines.

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
        postpone: false,
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
        postpone: false,
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

/// Toggled keys that a layer switch lets go come up in the order they went down, whatever
/// order the engine's hash maps keep (every fresh engine hashes differently).
#[test]
fn layer_switch_releases_toggles_in_order() {
    for _ in 0..20 {
        let mut p = Profile::default();
        for (button, k) in [(PadButton::X, Key::A), (PadButton::Y, Key::B)] {
            let mut m = map(b(button), vec![key(k)]);
            m.mode = Mode::Toggle;
            p.mappings.push(m);
        }
        let mut switch = map(b(PadButton::Back), vec![]);
        switch.jump = Some(Jump {
            layer: 1,
            mode: JumpMode::Switch,
            delay_ms: None,
            postpone: false,
        });
        p.mappings.push(switch);
        let mut h = Harness::new(p);
        for button in [PadButton::X, PadButton::Y] {
            h.btn(button, true);
            h.btn(button, false);
        }
        h.btn(PadButton::Back, true);
        let ups: Vec<Key> = h
            .keys
            .iter()
            .filter(|(_, _, down)| !down)
            .map(|(_, k, _)| *k)
            .collect();
        assert_eq!(ups, [Key::A, Key::B]);
    }
}

#[test]
fn held_button_is_repressed_in_new_layer() {
    let mut p = Profile::default();
    let mut shift = map(b(PadButton::Back), vec![]);
    shift.jump = Some(Jump {
        layer: 1,
        mode: JumpMode::Hold,
        delay_ms: None,
        postpone: false,
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
fn without_repress_held_buttons_keep_their_old_layer_action() {
    let mut p = Profile::default();
    p.layer_change.repress = false;
    let mut shift = map(b(PadButton::Back), vec![]);
    shift.jump = Some(Jump {
        layer: 1,
        mode: JumpMode::Hold,
        delay_ms: None,
        postpone: false,
    });
    p.mappings.push(shift);
    p.mappings.push(map(b(PadButton::A), vec![key(Key::Space)]));
    let mut f5 = map(b(PadButton::A), vec![key(Key::F5)]);
    f5.layer = 1;
    p.mappings.push(f5);
    let mut h = Harness::new(p);
    h.btn(PadButton::A, true); // Space down
    h.btn(PadButton::Back, true); // layer 1, Space stays down
    assert_eq!(h.e.layer(), 1);
    assert_eq!(h.key_events(Key::Space), vec![(1, true)]);
    h.btn(PadButton::A, false);
    assert_eq!(h.key_events(Key::Space), vec![(1, true), (3, false)]);
    h.until(20);
    h.btn(PadButton::A, true); // the next press uses layer 1
    assert_eq!(h.key_events(Key::F5), vec![(21, true)]);
    assert_eq!(h.key_events(Key::Space).len(), 2);
}

#[test]
fn postponed_jump_runs_its_actions_when_it_happens() {
    let mut p = Profile::default();
    let mut shift = map(b(PadButton::Back), vec![key(Key::F1)]);
    shift.jump = Some(Jump {
        layer: 1,
        mode: JumpMode::Hold,
        delay_ms: Some(300),
        postpone: true,
    });
    p.mappings.push(shift);
    let mut h = Harness::new(p);
    h.btn(PadButton::Back, true);
    h.until(250);
    assert_eq!(h.e.layer(), 0);
    assert!(h.key_events(Key::F1).is_empty(), "nothing before the jump");
    h.until(320);
    assert_eq!(h.e.layer(), 1);
    assert_eq!(h.key_events(Key::F1), vec![(301, true)]);
    h.btn(PadButton::Back, false);
    h.until(340);
    assert_eq!(h.e.layer(), 0);
    assert_eq!(h.key_events(Key::F1), vec![(301, true), (321, false)]);

    // Released before the delay: neither the jump nor the actions.
    h.btn(PadButton::Back, true);
    h.until(400);
    h.btn(PadButton::Back, false);
    h.until(800);
    assert_eq!(h.e.layer(), 0);
    assert_eq!(h.key_events(Key::F1).len(), 2);
}

#[test]
fn delayed_jump_without_postpone_runs_its_actions_at_the_press() {
    let mut p = Profile::default();
    let mut shift = map(b(PadButton::Back), vec![key(Key::F1)]);
    shift.jump = Some(Jump {
        layer: 1,
        mode: JumpMode::Hold,
        delay_ms: Some(300),
        postpone: false,
    });
    p.mappings.push(shift);
    let mut h = Harness::new(p);
    h.btn(PadButton::Back, true);
    assert_eq!(h.key_events(Key::F1), vec![(1, true)]);
    h.until(320);
    assert_eq!(h.e.layer(), 1);
}

#[test]
fn unlock_layer_gives_the_keyboard_back() {
    use crate::profile::Unlock;
    let mut p = Profile::default();
    p.layers.push(Layer {
        id: 1,
        unlock: Some(Unlock::Keyboard),
        ..Default::default()
    });
    let mut shift = map(b(PadButton::Back), vec![]);
    shift.jump = Some(Jump {
        layer: 1,
        mode: JumpMode::Hold,
        delay_ms: None,
        postpone: false,
    });
    p.mappings.push(shift);
    p.mappings
        .push(map(Control::Key(Key::W), vec![key(Key::Space)]));
    p.mappings
        .push(map(b(PadButton::B), vec![key(Key::Escape)]));
    let mut own = map(Control::Key(Key::E), vec![key(Key::F5)]);
    own.layer = 1;
    p.mappings.push(own);
    let mut h = Harness::new(p);
    assert_eq!(h.e.mutes().keys, vec![Key::W]);

    h.btn(PadButton::Back, true);
    assert_eq!(h.e.layer(), 1);
    assert!(h.e.mutes().keys.is_empty(), "nothing swallowed");
    h.key(Key::W, true);
    h.key(Key::W, false);
    assert!(
        h.key_events(Key::Space).is_empty(),
        "base keyboard mapping not inherited"
    );
    h.btn(PadButton::B, true);
    h.btn(PadButton::B, false);
    assert_eq!(
        h.key_events(Key::Escape).len(),
        2,
        "pad mappings are inherited"
    );
    h.key(Key::E, true);
    h.key(Key::E, false);
    assert_eq!(
        h.key_events(Key::F5).len(),
        2,
        "the layer's own mappings work"
    );
    h.btn(PadButton::Back, false);
    h.until(50);
    assert_eq!(h.e.layer(), 0);
    assert_eq!(h.e.mutes().keys, vec![Key::W]);
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
            absolute: false,
            action: Press::Auto,
        }],
    ));
    p.mappings.push(map(
        Control::Key(Key::D),
        vec![Action::Stick {
            stick: Stick::Left,
            x: 1.0,
            y: 0.0,
            absolute: false,
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

fn push(stick: Stick, x: f32, y: f32) -> Action {
    Action::Stick {
        stick,
        x,
        y,
        absolute: false,
        action: Press::Auto,
    }
}

fn near(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
}

#[test]
fn buttons_ramp_a_virtual_stick_up() {
    use crate::profile::VirtualStick;
    let mut p = Profile::default();
    p.virtual_devices.sticks.left = VirtualStick {
        initial: 0.25,
        max: 0.75,
        speed: 1.0,
        multiplier_x: 2.0,
        ..Default::default()
    };
    p.mappings
        .push(map(Control::Key(Key::W), vec![push(Stick::Left, 0.0, 1.0)]));
    p.mappings.push(map(
        Control::Key(Key::S),
        vec![push(Stick::Left, 0.0, -1.0)],
    ));
    p.mappings
        .push(map(Control::Key(Key::D), vec![push(Stick::Left, 0.4, 0.0)]));
    let mut h = Harness::new(p);

    h.key(Key::W, true); // 1
    assert!(
        near(h.virtual_pad().stick(Stick::Left), (0.0, 0.25)),
        "starts at `initial`"
    );
    h.until(251);
    assert!(near(h.virtual_pad().stick(Stick::Left), (0.0, 0.5)));
    h.until(700);
    assert!(
        near(h.virtual_pad().stick(Stick::Left), (0.0, 0.75)),
        "capped at `max`"
    );

    h.key(Key::S, true); // 701: opposite directions cancel
    assert_eq!(h.virtual_pad().stick(Stick::Left).1, 0.0);
    h.key(Key::S, false);
    assert!(near(h.virtual_pad().stick(Stick::Left), (0.0, 0.75)));

    h.key(Key::D, true); // 703: x doubles the speed but stops at the action's 0.4
    assert!(
        near(h.virtual_pad().stick(Stick::Left), (0.25, 0.75)),
        "not normalised"
    );
    h.until(753);
    assert!(near(h.virtual_pad().stick(Stick::Left), (0.35, 0.75)));
    h.until(900);
    assert!(near(h.virtual_pad().stick(Stick::Left), (0.4, 0.75)));
}

#[test]
fn virtual_stick_curve_and_layer_settings() {
    use crate::profile::{LayerVirtual, LayerVirtualSticks, VirtualStickPatch};
    let mut p = Profile::default();
    p.virtual_devices.sticks.right.curve = vec![[0.5, 0.25]];
    p.layers.push(Layer {
        id: 1,
        virtual_devices: LayerVirtual {
            sticks: LayerVirtualSticks {
                right: VirtualStickPatch {
                    curve: Some(Vec::new()),
                    max: Some(0.3),
                    ..Default::default()
                },
                ..Default::default()
            },
        },
        ..Default::default()
    });
    let mut shift = map(b(PadButton::Back), vec![]);
    shift.jump = Some(Jump {
        layer: 1,
        mode: JumpMode::Hold,
        delay_ms: None,
        postpone: false,
    });
    p.mappings.push(shift);
    p.mappings.push(map(
        Control::Key(Key::L),
        vec![push(Stick::Right, 0.5, 0.0)],
    ));
    let mut h = Harness::new(p);

    h.key(Key::L, true);
    assert!(
        near(h.virtual_pad().stick(Stick::Right), (0.25, 0.0)),
        "curve applies"
    );
    h.btn(PadButton::Back, true);
    h.until(10);
    assert!(
        near(h.virtual_pad().stick(Stick::Right), (0.3, 0.0)),
        "layer: linear, max 0.3"
    );
}

#[test]
fn jitter_walks_its_pattern() {
    use crate::profile::{Jitter, JitterPattern};
    let mut p = Profile::default();
    p.virtual_devices.sticks.right.jitter = Jitter {
        enabled: true,
        pattern: JitterPattern::Cross,
        min: 0.05,
        max: 0.05,
        delay_min_ms: 30,
        delay_max_ms: 30,
    };
    let mut h = Harness::new(p);
    let at = |h: &mut Harness, t| {
        h.until(t);
        h.virtual_pad().stick(Stick::Right)
    };
    assert!(near(at(&mut h, 29), (0.05, 0.0)), "right");
    assert!(near(at(&mut h, 30), (-0.05, 0.0)), "left");
    assert!(near(at(&mut h, 60), (0.0, 0.05)), "up");
    assert!(near(at(&mut h, 90), (0.0, -0.05)), "down");
    assert!(near(at(&mut h, 120), (0.05, 0.0)), "and around");
    assert_eq!(h.virtual_pad().stick(Stick::Left), (0.0, 0.0));
}

#[test]
fn shake_rocks_the_accelerometer() {
    use crate::profile::ShakeAxis;
    let mut p = Profile::default();
    p.virtual_devices.motion.shake_ms = 100;
    p.mappings.push(map(
        Control::Key(Key::Q),
        vec![Action::Shake {
            shake: ShakeAxis::PlusY,
        }],
    ));
    let mut h = Harness::new(p);
    h.key(Key::Q, true); // 1
    h.key(Key::Q, false);
    let accel = |h: &Harness| h.virtual_pad().motion.map(|m| m.accel);
    assert_eq!(accel(&h), Some([0.0, 3.0, 0.0]), "rest (1 g up) plus 2 g");
    h.until(26);
    assert_eq!(accel(&h), Some([0.0, -1.0, 0.0]), "flips after 25 ms");
    h.until(51);
    assert_eq!(accel(&h), Some([0.0, 3.0, 0.0]));
    h.until(101);
    assert_eq!(accel(&h), None, "over after shake_ms");
}

#[test]
fn absolute_stick_actions_replace_their_axes() {
    let mut p = Profile::default();
    let set = |x: f32, y: f32| Action::Stick {
        stick: Stick::Left,
        x,
        y,
        absolute: true,
        action: Press::Auto,
    };
    p.mappings
        .push(map(Control::Key(Key::W), vec![set(0.0, 0.3)]));
    p.mappings
        .push(map(Control::Key(Key::C), vec![set(0.0, 0.0)]));
    p.mappings
        .push(map(Control::Key(Key::R), vec![push(Stick::Left, 0.0, 0.3)]));
    let mut h = Harness::new(p);
    h.pad.set_stick(Stick::Left, (0.4, 0.5));
    h.key(Key::W, true);
    assert!(
        near(h.virtual_pad().stick(Stick::Left), (0.4, 0.3)),
        "y set, x passed through"
    );
    h.key(Key::W, false);
    h.key(Key::R, true);
    assert!(
        near(h.virtual_pad().stick(Stick::Left), (0.4, 0.8)),
        "relative adds"
    );
    h.key(Key::R, false);
    h.key(Key::C, true);
    assert_eq!(h.virtual_pad().stick(Stick::Left), (0.0, 0.0), "centred");
}

#[test]
fn random_ms_spreads_turbo_cycles() {
    let mut p = Profile::default();
    let mut m = map(b(PadButton::A), vec![key(Key::F)]);
    m.mode = Mode::Turbo;
    m.turbo_ms = Some(50);
    m.random_ms = Some(20);
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.btn(PadButton::A, true);
    h.until(1000);
    let downs: Vec<u64> = h
        .key_events(Key::F)
        .into_iter()
        .filter(|(_, d)| *d)
        .map(|(t, _)| t)
        .collect();
    let gaps: Vec<u64> = downs.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(gaps.len() > 10, "{gaps:?}");
    assert!(gaps.iter().all(|g| (50..=70).contains(g)), "{gaps:?}");
    assert!(
        gaps.iter().any(|g| *g != gaps[0]),
        "not all the same: {gaps:?}"
    );
}

#[test]
fn turn_action_spreads_over_turn_ms() {
    let mut p = Profile::default();
    p.virtual_devices.mouse.counts_per_turn = 3600.0;
    p.mappings.push(map(
        Control::Key(Key::T),
        vec![Action::Turn { turn: -90.0 }],
    ));
    let mut h = Harness::new(p);
    h.key(Key::T, true);
    h.key(Key::T, false);
    h.until(50);
    let half = h.mouse_moved.0;
    assert!(half < -300.0 && half > -600.0, "about half way: {half}");
    h.until(200);
    assert!((h.mouse_moved.0 + 900.0).abs() < 0.5, "{}", h.mouse_moved.0);
    assert_eq!(h.mouse_moved.1, 0.0);
}

#[test]
fn tilt_mode_starts_off_and_recentres_on_command() {
    use crate::pad::Motion;
    use crate::profile::{GyroMode, GyroSettings};
    let mut p = Profile {
        gyro: GyroSettings {
            mode: GyroMode::Tilt,
            initially_on: false,
            ..Default::default()
        },
        ..Default::default()
    };
    p.mappings.push(map(
        Control::GyroZone(Some(GyroDir::Up), Zone::Med),
        vec![key(Key::U)],
    ));
    p.mappings.push(map(
        b(PadButton::Y),
        vec![Action::Command {
            command: Command::GyroOn,
        }],
    ));
    p.mappings.push(map(
        b(PadButton::X),
        vec![Action::Command {
            command: Command::ResetTilt,
        }],
    ));
    let tilted = |deg: f32| {
        let r = deg.to_radians();
        Some(Motion {
            gyro: [0.0; 3],
            accel: [0.0, r.cos(), -r.sin()],
        })
    };
    let mut h = Harness::new(p);
    h.pad.motion = tilted(0.0); // centre pose
    h.until(5);
    h.pad.motion = tilted(30.0);
    h.until(10);
    assert!(!h.e.is_output_down(Key::U), "gyro starts off");
    h.btn(PadButton::Y, true);
    h.btn(PadButton::Y, false);
    h.until(20);
    assert!(h.e.is_output_down(Key::U), "30° nose up is the medium zone");
    h.btn(PadButton::X, true); // this pose is the new centre
    h.until(30);
    assert!(!h.e.is_output_down(Key::U));
}

#[test]
fn touchpad_gestures_and_split_trackpads() {
    use crate::pad::TouchPoint;
    let mut p = Profile::default();
    p.touchpad.split = true;
    p.mappings.push(map(
        Control::Gesture(TouchGesture::TapRight),
        vec![key(Key::T)],
    ));
    p.mappings.push(map(
        Control::Trackpad(Half::Left, TrackpadPart::Region(TouchRegion::Up)),
        vec![key(Key::U)],
    ));
    p.mappings.push(map(
        Control::Trackpad(Half::Right, TrackpadPart::Click),
        vec![key(Key::C)],
    ));
    let mut h = Harness::new(p);
    // Quick tap on the right third.
    h.pad.touch[0] = Some(TouchPoint { x: 0.9, y: 0.5 });
    h.until(30);
    h.pad.touch[0] = None;
    h.until(200);
    assert_eq!(
        h.key_events(Key::T),
        vec![(31, true), (81, false)],
        "a 50 ms press"
    );
    // Top of the left half; then a click with the finger on the right half.
    h.pad.touch[0] = Some(TouchPoint { x: 0.2, y: 0.05 });
    h.until(250);
    assert!(h.e.is_output_down(Key::U));
    h.pad.touch[0] = Some(TouchPoint { x: 0.8, y: 0.5 });
    h.pad.set(PadButton::Touchpad, true);
    h.until(300);
    assert!(h.e.is_output_down(Key::C) && !h.e.is_output_down(Key::U));
    assert!(
        !h.virtual_pad().pressed(PadButton::Touchpad),
        "the click is taken"
    );
}

#[test]
fn touch_actions_play_on_the_virtual_touchpad() {
    let mut p = Profile::default();
    p.output.virtual_pad = VirtualPadType::DualShock4;
    p.mappings.push(map(
        Control::Key(Key::S),
        vec![Action::Touch {
            touch: TouchGesture::SwipeRight,
            ms: Some(100),
        }],
    ));
    let mut h = Harness::new(p);
    h.key(Key::S, true); // 1
    let x = |h: &Harness| h.virtual_pad().touch[0].map(|f| (f.x * 100.0).round() / 100.0);
    assert_eq!(x(&h), Some(0.2), "starts at a fifth");
    h.until(51);
    assert_eq!(x(&h), Some(0.5));
    h.until(100);
    assert!(x(&h).unwrap() > 0.75);
    h.until(102);
    assert_eq!(x(&h), None, "done after 100 ms");
}

#[test]
fn mouse_speed_zones() {
    let mut p = Profile::default();
    p.mappings.push(map(
        Control::MouseZone(Some(Dir::Right), Zone::Med),
        vec![key(Key::M)],
    ));
    p.mappings
        .push(map(Control::MouseZone(None, Zone::High), vec![key(Key::H)]));
    let mut h = Harness::new(p);
    // 2 counts per ms = 2000 counts/s to the right: medium.
    for _ in 0..100 {
        h.pending_kbm.push(KbmEvent::MouseMove { dx: 2, dy: 0 });
        h.t += 1;
        h.tick();
    }
    assert!(h.e.is_output_down(Key::M) && !h.e.is_output_down(Key::H));
    for _ in 0..100 {
        h.pending_kbm.push(KbmEvent::MouseMove { dx: 4, dy: 0 });
        h.t += 1;
        h.tick();
    }
    assert!(h.e.is_output_down(Key::H), "4000 counts/s is high");
}

#[test]
fn mouse_acceleration_wheel_repeat_and_absolute() {
    let mut p = Profile::default();
    p.virtual_devices.mouse.wheel_repeat_ms = 50;
    p.mappings.push(map(
        Control::Key(Key::A),
        vec![Action::MouseMove {
            mouse_move: Dir::Right,
            speed: 100.0,
            acceleration: 1.0,
        }],
    ));
    p.mappings.push(map(
        Control::Key(Key::W),
        vec![Action::Wheel {
            wheel: WheelDir::Up,
            clicks: 1.0,
        }],
    ));
    p.mappings.push(map(
        Control::Key(Key::C),
        vec![
            Action::MouseTo {
                mouse_to: [0.5, 0.5],
            },
            Action::MouseClip {
                mouse_clip: [0.25, 0.25, 0.75, 0.75],
            },
        ],
    ));
    let mut h = Harness::new(p);
    h.key(Key::A, true);
    h.until(1001);
    // 100 px/s growing to 200 px/s over the second: about 150 px.
    assert!((h.mouse_moved.0 - 150.0).abs() < 3.0, "{}", h.mouse_moved.0);
    h.key(Key::A, false);

    let mut notches = 0;
    h.key(Key::W, true);
    let start = h.t;
    while h.t < start + 199 {
        h.t += 1;
        h.tick();
        notches += h.last.wheel.len();
    }
    h.key(Key::W, false);
    assert_eq!(
        notches, 3,
        "repeats at 50, 100 and 150 ms after the first notch"
    );

    h.key(Key::C, true);
    assert_eq!(h.last.mouse_to, Some((0.5, 0.5)));
    h.until(h.t + 5);
    assert_eq!(h.last.mouse_to, None, "the move is sent once");
    assert_eq!(h.last.mouse_clip, Some([0.25, 0.25, 0.75, 0.75]));
    h.key(Key::C, false);
    assert_eq!(h.last.mouse_clip, None, "released with the mapping");
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
fn touch_and_motion_pass_through_unless_remapped() {
    use crate::pad::{Motion, TouchPoint};
    let finger = Some(TouchPoint { x: 0.2, y: 0.7 });
    let motion = Some(Motion {
        gyro: [1.0, 2.0, 3.0],
        accel: [0.0, 1.0, 0.0],
    });

    let mut h = Harness::new(Profile::default());
    h.pad.touch[0] = finger;
    h.pad.motion = motion;
    h.until(5);
    assert_eq!(h.virtual_pad().touch[0], finger);
    assert_eq!(h.virtual_pad().motion, motion);

    // Touchpad → mouse and a gyro direction mapping both hide their source.
    let mut p = Profile::default();
    p.analog.push(AnalogMapping {
        source: AnalogSource::Touchpad,
        target: AnalogTarget::Mouse,
        ..Default::default()
    });
    p.mappings
        .push(map(Control::Gyro(GyroDir::Left), vec![key(Key::Q)]));
    let mut h = Harness::new(p);
    h.pad.touch[0] = finger;
    h.pad.motion = motion;
    h.until(5);
    assert_eq!(h.virtual_pad().touch, [None, None]);
    assert_eq!(h.virtual_pad().motion, None);
}

#[test]
fn stick_diagonals_are_separate_when_switched_on() {
    let stick_profile = |diagonals: bool| {
        let mut p = Profile::default();
        p.sticks.left.diagonals = diagonals;
        p.mappings.push(map(
            Control::StickDir(Stick::Left, Dir::Up),
            vec![key(Key::W)],
        ));
        p.mappings.push(map(
            Control::StickDir(Stick::Left, Dir::Right),
            vec![key(Key::D)],
        ));
        p.mappings.push(map(
            Control::StickDir(Stick::Left, Dir::UpRight),
            vec![key(Key::E)],
        ));
        p
    };
    let up_right = |h: &mut Harness| {
        h.pad.set_stick(Stick::Left, (0.7, 0.7));
        h.until(h.t + 5);
        [Key::W, Key::D, Key::E].map(|k| h.e.is_output_down(k))
    };
    let mut h = Harness::new(stick_profile(true));
    assert_eq!(
        up_right(&mut h),
        [false, false, true],
        "8-way: only the diagonal"
    );
    let mut h = Harness::new(stick_profile(false));
    assert_eq!(
        up_right(&mut h),
        [true, true, false],
        "4-way: both cardinals"
    );
}

#[test]
fn diagonal_touch_and_mouse_controls_switch_their_source_to_eight_way() {
    use crate::pad::TouchPoint;
    let mut p = Profile::default();
    p.mappings
        .push(map(Control::Touch(TouchRegion::UpLeft), vec![key(Key::Q)]));
    p.mappings
        .push(map(Control::MouseMove(Dir::DownRight), vec![key(Key::C)]));
    p.mappings
        .push(map(Control::Touch(TouchRegion::Up), vec![key(Key::W)]));
    let mut h = Harness::new(p);
    h.pad.touch[0] = Some(TouchPoint { x: 0.05, y: 0.05 });
    h.until(h.t + 5);
    assert!(h.e.is_output_down(Key::Q) && !h.e.is_output_down(Key::W));
    h.pad.touch[0] = Some(TouchPoint { x: 0.5, y: 0.0 });
    h.until(h.t + 5);
    assert!(h.e.is_output_down(Key::W) && !h.e.is_output_down(Key::Q));
    // Mouse moving right and down at 3000 counts/s.
    for _ in 0..30 {
        h.pending_kbm.push(KbmEvent::MouseMove { dx: 3, dy: 3 });
        h.t += 1;
        h.tick();
    }
    assert!(h.e.is_output_down(Key::C));
}

#[test]
fn gyro_commands_switch_gyro_routes() {
    use crate::pad::Motion;
    let mut p = Profile::default();
    p.analog.push(AnalogMapping {
        source: AnalogSource::Gyro,
        target: AnalogTarget::Mouse,
        ..Default::default()
    });
    for (button, command) in [
        (PadButton::X, Command::GyroOff),
        (PadButton::Y, Command::GyroOn),
        (PadButton::B, Command::ToggleGyro),
    ] {
        p.mappings
            .push(map(b(button), vec![Action::Command { command }]));
    }
    let mut h = Harness::new(p);
    h.pad.motion = Some(Motion {
        gyro: [0.0, 100.0, 0.0],
        accel: [0.0, 1.0, 0.0],
    });
    let moved = |h: &mut Harness| {
        h.mouse_moved = (0.0, 0.0);
        h.until(h.t + 20);
        h.mouse_moved.0.abs() > 0.5
    };
    assert!(moved(&mut h), "gyro aims");
    h.btn(PadButton::X, true);
    h.btn(PadButton::X, false);
    assert!(!moved(&mut h), "off");
    h.btn(PadButton::X, true);
    h.btn(PadButton::X, false);
    assert!(!moved(&mut h), "off stays off");
    h.btn(PadButton::Y, true);
    h.btn(PadButton::Y, false);
    assert!(moved(&mut h), "on");
    h.btn(PadButton::B, true);
    h.btn(PadButton::B, false);
    assert!(!moved(&mut h), "toggled off");
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
fn break_steps_cycle_through_segments() {
    let mut p = Profile::default();
    let mut m = map(
        b(PadButton::A),
        vec![key(Key::Q), Action::BREAK, key(Key::E), key(Key::R)],
    );
    m.mode = Mode::Turbo; // not possible with segments: held instead
    p.mappings.push(m);
    let mut h = Harness::new(p);
    let press = |h: &mut Harness| {
        h.btn(PadButton::A, true);
        h.until(h.t + 30);
        let down: Vec<Key> = [Key::Q, Key::E, Key::R]
            .into_iter()
            .filter(|k| h.e.is_output_down(*k))
            .collect();
        h.btn(PadButton::A, false);
        h.until(h.t + 30);
        down
    };
    assert_eq!(press(&mut h), vec![Key::Q]);
    assert!(!h.e.is_output_down(Key::Q), "held until release");
    assert_eq!(press(&mut h), vec![Key::E, Key::R]);
    assert_eq!(press(&mut h), vec![Key::Q], "starts over");
}

#[test]
fn break_cursor_resets_on_layer_change() {
    let mut p = Profile::default();
    let mut m = map(
        b(PadButton::A),
        vec![key(Key::Q), Action::BREAK, key(Key::E)],
    );
    m.mode = Mode::Once;
    p.mappings.push(m);
    let mut sw = map(b(PadButton::Y), vec![]);
    sw.jump = Some(Jump {
        layer: 1,
        mode: JumpMode::Toggle,
        delay_ms: None,
        postpone: false,
    });
    p.mappings.push(sw);
    let mut h = Harness::new(p);
    h.btn(PadButton::A, true);
    h.btn(PadButton::A, false);
    h.until(h.t + 50);
    assert_eq!(h.key_events(Key::Q).len(), 2, "segment 1 tapped Q");
    // Into layer 1 (inherits A) and back.
    h.btn(PadButton::Y, true);
    h.btn(PadButton::Y, false);
    h.btn(PadButton::Y, true);
    h.btn(PadButton::Y, false);
    h.btn(PadButton::A, true);
    h.btn(PadButton::A, false);
    h.until(h.t + 50);
    assert_eq!(h.key_events(Key::Q).len(), 4, "segment 1 again");
    assert!(h.key_events(Key::E).is_empty());
}

#[test]
fn stop_all_releases_latched_outputs() {
    let mut p = Profile::default();
    let mut m = map(b(PadButton::X), vec![key(Key::LeftShift)]);
    m.mode = Mode::Toggle;
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.btn(PadButton::X, true);
    h.btn(PadButton::X, false);
    assert!(h.e.is_output_down(Key::LeftShift));
    h.e.stop_all();
    h.t += 1;
    h.tick();
    assert!(!h.e.is_output_down(Key::LeftShift));
    assert_eq!(h.key_events(Key::LeftShift).last(), Some(&(h.t, false)));
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

fn tzone(t: Trigger, z: Zone) -> Control {
    Control::TriggerZone(t, z)
}

fn szone(s: Stick, d: Option<Dir>, z: Zone) -> Control {
    Control::StickZone(s, d, z)
}

#[test]
fn trigger_zones_follow_the_pull() {
    // Default thresholds: low 0.03, medium ~0.35, high ~0.67.
    let mut p = Profile::default();
    p.mappings.push(map(
        Control::TriggerPress(Trigger::Right),
        vec![key(Key::P)],
    ));
    p.mappings
        .push(map(tzone(Trigger::Right, Zone::Low), vec![key(Key::A)]));
    p.mappings
        .push(map(tzone(Trigger::Right, Zone::Med), vec![key(Key::B)]));
    p.mappings
        .push(map(tzone(Trigger::Right, Zone::High), vec![key(Key::C)]));
    let mut h = Harness::new(p);
    for (t, v) in [(10, 0.2), (20, 0.5), (30, 0.9), (40, 0.5), (50, 0.0)] {
        h.pad.set_axis(Axis::RightTrigger, v);
        h.until(t);
    }
    assert_eq!(h.key_events(Key::P), vec![(1, true), (41, false)]);
    assert_eq!(h.key_events(Key::A), vec![(1, true), (11, false)]);
    assert_eq!(
        h.key_events(Key::B),
        vec![(11, true), (21, false), (31, true), (41, false)]
    );
    assert_eq!(h.key_events(Key::C), vec![(21, true), (31, false)]);
}

#[test]
fn stick_zones_walk_and_run() {
    let mut p = Profile::default();
    p.sticks.left.zones = crate::profile::StickZones {
        low: crate::profile::ZoneRadius::round(0.2),
        med: Some(crate::profile::ZoneRadius::round(0.5)),
        high: Some(crate::profile::ZoneRadius::round(0.8)),
    };
    let up = Some(Dir::Up);
    p.mappings
        .push(map(szone(Stick::Left, up, Zone::Low), vec![key(Key::W)]));
    let mut run = map(
        szone(Stick::Left, up, Zone::High),
        vec![key(Key::LeftShift), key(Key::W)],
    );
    run.key_delay_ms = Some(0);
    p.mappings.push(run);
    p.mappings
        .push(map(szone(Stick::Left, None, Zone::Med), vec![key(Key::M)]));
    let mut h = Harness::new(p);

    h.pad.set_stick(Stick::Left, (0.0, 0.3)); // lightly up: walk
    h.until(10);
    h.pad.set_stick(Stick::Left, (0.0, 0.6)); // medium zone, any direction
    h.until(20);
    h.pad.set_stick(Stick::Left, (0.0, 0.9)); // pushed far: run
    h.until(30);
    h.pad.set_stick(Stick::Left, (0.9, 0.0)); // far right: no up zone
    h.until(40);
    h.pad.set_stick(Stick::Left, (0.6, 0.6)); // diagonal counts as up and right
    h.until(50);
    h.pad.set_stick(Stick::Left, (0.0, 0.0));
    h.until(60);

    assert_eq!(
        h.key_events(Key::W),
        vec![
            (1, true),
            (11, false),
            (21, true),
            (31, false),
            (41, true),
            (51, false)
        ]
    );
    assert_eq!(
        h.key_events(Key::LeftShift),
        vec![(21, true), (31, false), (41, true), (51, false)]
    );
    assert_eq!(h.key_events(Key::M), vec![(11, true), (21, false)]);
}

#[test]
fn zone_controls_in_combos_and_with_press_patterns() {
    let mut p = Profile::default();
    p.mappings.push(map(
        Combo(vec![b(PadButton::LB), tzone(Trigger::Right, Zone::High)]),
        vec![key(Key::X)],
    ));
    let mut long = map(tzone(Trigger::Left, Zone::High), vec![key(Key::L)]);
    long.activator = Activator::Long;
    p.mappings.push(long);
    let mut h = Harness::new(p);

    h.btn(PadButton::LB, true); // 1
    h.until(100);
    h.pad.set_axis(Axis::RightTrigger, 1.0);
    h.until(110); // RTHigh at 101 completes the combo
    h.pad.set_axis(Axis::RightTrigger, 0.0);
    h.until(120);
    assert_eq!(h.key_events(Key::X), vec![(101, true), (111, false)]);

    h.pad.set_axis(Axis::LeftTrigger, 1.0);
    h.until(150);
    h.pad.set_axis(Axis::LeftTrigger, 0.5); // leaves the high zone before the long press
    h.until(500);
    assert!(h.key_events(Key::L).is_empty());
    h.pad.set_axis(Axis::LeftTrigger, 1.0);
    h.until(900);
    assert_eq!(h.key_events(Key::L), vec![(801, true)]);
}

#[test]
fn zone_mappings_follow_layers() {
    let mut p = Profile::default();
    let mut shift = map(b(PadButton::Back), vec![]);
    shift.jump = Some(Jump {
        layer: 1,
        mode: JumpMode::Hold,
        delay_ms: None,
        postpone: false,
    });
    p.mappings.push(shift);
    p.mappings.push(map(
        tzone(Trigger::Right, Zone::High),
        vec![key(Key::Space)],
    ));
    let mut f5 = map(tzone(Trigger::Right, Zone::High), vec![key(Key::F5)]);
    f5.layer = 1;
    p.mappings.push(f5);
    let mut h = Harness::new(p);

    h.pad.set_axis(Axis::RightTrigger, 1.0);
    h.until(10);
    h.btn(PadButton::Back, true); // 11: the held zone is re-pressed in layer 1
    h.until(20);
    h.pad.set_axis(Axis::RightTrigger, 0.0);
    h.until(30);
    assert_eq!(h.key_events(Key::Space), vec![(1, true), (11, false)]);
    assert_eq!(h.key_events(Key::F5), vec![(11, true), (21, false)]);
}

#[test]
fn layers_bring_their_own_stick_and_trigger_settings() {
    use crate::profile::{LayerSticks, LayerTriggers, StickPatch, TriggerPatch, TriggerZones};
    let mut p = Profile::default();
    p.layers.push(Layer {
        id: 1,
        sticks: LayerSticks {
            left: StickPatch {
                deadzone: Some(0.5),
                ..Default::default()
            },
            ..Default::default()
        },
        triggers: LayerTriggers {
            right: TriggerPatch {
                zones: Some(TriggerZones {
                    low: 0.5,
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    });
    let mut shift = map(b(PadButton::Back), vec![]);
    shift.jump = Some(Jump {
        layer: 1,
        mode: JumpMode::Hold,
        delay_ms: None,
        postpone: false,
    });
    p.mappings.push(shift);
    p.mappings.push(map(
        Control::TriggerPress(Trigger::Right),
        vec![key(Key::P)],
    ));
    let mut h = Harness::new(p);

    h.pad.set_stick(Stick::Left, (0.3, 0.0));
    h.pad.set_axis(Axis::RightTrigger, 0.3);
    h.until(10);
    assert!(h.virtual_pad().stick(Stick::Left).0 > 0.0, "base deadzone");
    h.btn(PadButton::Back, true); // 11: layer 1 has a wider stick deadzone and a later RT
    h.until(20);
    assert_eq!(h.virtual_pad().stick(Stick::Left), (0.0, 0.0));
    h.pad.set_axis(Axis::RightTrigger, 0.6);
    h.until(30);
    h.btn(PadButton::Back, false); // 31: back to the base settings
    h.until(40);
    assert!(h.virtual_pad().stick(Stick::Left).0 > 0.0);
    assert_eq!(
        h.key_events(Key::P),
        vec![(1, true), (11, false), (21, true), (31, false), (31, true)],
        "RT lets go at once where layer 1 needs a deeper pull, and is re-pressed on the way back"
    );
}

#[test]
fn zone_mapping_replaces_the_mirrored_control() {
    let mut p = Profile::default();
    p.mappings
        .push(map(tzone(Trigger::Right, Zone::High), vec![key(Key::C)]));
    p.mappings
        .push(map(szone(Stick::Left, None, Zone::Low), vec![key(Key::W)]));
    let mut h = Harness::new(p);
    h.pad.set_axis(Axis::RightTrigger, 0.5);
    h.pad.set_axis(Axis::LeftTrigger, 0.5);
    h.pad.set_stick(Stick::Left, (0.5, 0.0));
    h.pad.set_stick(Stick::Right, (0.5, 0.0));
    h.until(2);
    let v = h.virtual_pad();
    assert_eq!(v.axis(Axis::RightTrigger), 0.0);
    assert_eq!(v.stick(Stick::Left), (0.0, 0.0));
    assert!(
        v.axis(Axis::LeftTrigger) > 0.4,
        "unmapped trigger is mirrored"
    );
    assert_eq!(v.stick(Stick::Right), (0.5, 0.0));
}

fn radial_profile(apply: crate::profile::RadialApply) -> Profile {
    use crate::profile::{RadialMenu, RadialSector};
    let mut p = Profile::default();
    let mut open = map(b(PadButton::LB), vec![]);
    open.jump = Some(Jump {
        layer: 1,
        mode: JumpMode::Hold,
        delay_ms: None,
        postpone: false,
    });
    p.mappings.push(open);
    let keys = [Key::Digit1, Key::Digit2, Key::Digit3, Key::Digit4];
    p.radial_menus.push(RadialMenu {
        layer: 1,
        apply,
        sectors: keys
            .iter()
            .map(|&k| RadialSector {
                actions: vec![key(k)],
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    });
    p
}

#[test]
fn radial_menu_plays_the_pointed_sector_on_release() {
    let mut h = Harness::new(radial_profile(crate::profile::RadialApply::Release));
    h.btn(PadButton::LB, true);
    let shown = h.last.radial.expect("open while the layer is active");
    assert_eq!((shown.menu, shown.pointed), (0, None));
    h.pad.set_stick(Stick::Right, (1.0, 0.0));
    h.until(h.t + 2);
    assert_eq!(h.last.radial.unwrap().pointed, Some(1));
    assert_eq!(
        h.virtual_pad().stick(Stick::Right),
        (0.0, 0.0),
        "the pointing stick does not reach the game"
    );
    h.pad.set_stick(Stick::Right, (0.0, 0.0));
    h.until(h.t + 2);
    assert_eq!(h.last.radial.unwrap().pointed, Some(1), "the choice stays");
    h.btn(PadButton::LB, false);
    h.until(h.t + 50);
    assert!(h.last.radial.is_none());
    assert_eq!(h.key_events(Key::Digit2).len(), 2, "tapped once");
    assert!(h.key_events(Key::Digit1).is_empty());
}

#[test]
fn radial_menu_with_commands() {
    use crate::profile::RadialApply;
    let mut p = radial_profile(RadialApply::Command);
    for (button, command) in [
        (PadButton::RB, Command::RadialNext),
        (PadButton::A, Command::RadialApply),
    ] {
        let mut m = map(b(button), vec![Action::Command { command }]);
        m.layer = 1;
        m.mode = crate::profile::Mode::Once;
        p.mappings.push(m);
    }
    let mut h = Harness::new(p);
    h.btn(PadButton::LB, true);
    for _ in 0..3 {
        h.btn(PadButton::RB, true);
        h.btn(PadButton::RB, false);
        h.until(h.t + 30);
    }
    assert_eq!(h.last.radial.unwrap().pointed, Some(2));
    h.btn(PadButton::A, true);
    h.btn(PadButton::A, false);
    h.until(h.t + 50);
    assert_eq!(h.key_events(Key::Digit3).len(), 2);
    assert!(h.last.radial.is_none(), "closed after applying");
    h.btn(PadButton::LB, false);
    h.until(h.t + 50);
    assert_eq!(
        h.key_events(Key::Digit3).len(),
        2,
        "release does not play it again"
    );
}
