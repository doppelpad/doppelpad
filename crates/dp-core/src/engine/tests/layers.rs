//! Layer jumps, inherited and layer-own mappings and settings.

use super::*;

#[test]
fn hold_layer_switch() {
    let mut p = shift_profile();
    p.layers.push(Layer {
        id: 1,
        name: "Shift".into(),
        ..Default::default()
    });
    p.mappings
        .push(map(b(PadButton::B), vec![key(Key::Escape)])); // inherited into layer 1
    let mut h = Harness::new(p);

    h.btn(PadButton::Back, true);
    assert_eq!(h.e.layer(), 1);
    assert!(h.last.layer_changed);
    h.tap(PadButton::A);
    h.tap(PadButton::B);
    h.btn(PadButton::Back, false);
    h.until(50);
    assert_eq!(h.e.layer(), 0, "release returns to the base layer");
    h.tap(PadButton::A);

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
    p.mappings
        .push(shift(b(PadButton::Back), 2, JumpMode::Toggle));
    let mut h = Harness::new(p);
    h.tap(PadButton::Back);
    assert_eq!(h.e.layer(), 2);
    h.until(20);
    h.tap(PadButton::Back);
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
        p.mappings
            .push(shift(b(PadButton::Back), 1, JumpMode::Switch));
        let mut h = Harness::new(p);
        for button in [PadButton::X, PadButton::Y] {
            h.tap(button);
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
    let mut h = Harness::new(shift_profile());
    h.btn(PadButton::A, true); // Space down
    h.btn(PadButton::Back, true); // switch: Space up, F5 down
    assert_eq!(h.key_events(Key::Space), vec![(1, true), (2, false)]);
    assert_eq!(h.key_events(Key::F5), vec![(2, true)]);
}

#[test]
fn without_repress_held_buttons_keep_their_old_layer_action() {
    let mut p = shift_profile();
    p.layer_change.repress = false;
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
        delay_ms: Some(300),
        postpone: true,
        ..jump(1, JumpMode::Hold)
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
        delay_ms: Some(300),
        ..jump(1, JumpMode::Hold)
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
    p.mappings
        .push(shift(b(PadButton::Back), 1, JumpMode::Hold));
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
    h.tap(PadButton::B);
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
fn break_cursor_resets_on_layer_change() {
    let mut p = Profile::default();
    let mut m = map(
        b(PadButton::A),
        vec![key(Key::Q), Action::BREAK, key(Key::E)],
    );
    m.mode = Mode::Once;
    p.mappings.push(m);
    p.mappings.push(shift(b(PadButton::Y), 1, JumpMode::Toggle));
    let mut h = Harness::new(p);
    h.tap(PadButton::A);
    h.until(h.t + 50);
    assert_eq!(h.key_events(Key::Q).len(), 2, "segment 1 tapped Q");
    // Into layer 1 (inherits A) and back.
    h.tap(PadButton::Y);
    h.tap(PadButton::Y);
    h.tap(PadButton::A);
    h.until(h.t + 50);
    assert_eq!(h.key_events(Key::Q).len(), 4, "segment 1 again");
    assert!(h.key_events(Key::E).is_empty());
}

#[test]
fn zone_mappings_follow_layers() {
    let mut p = Profile::default();
    p.mappings
        .push(shift(b(PadButton::Back), 1, JumpMode::Hold));
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
    p.mappings
        .push(shift(b(PadButton::Back), 1, JumpMode::Hold));
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
