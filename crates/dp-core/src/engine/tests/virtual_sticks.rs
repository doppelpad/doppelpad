//! Virtual sticks and motion driven by mapping actions.

use super::*;

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
    p.mappings
        .push(shift(b(PadButton::Back), 1, JumpMode::Hold));
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
