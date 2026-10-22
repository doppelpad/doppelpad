//! Which physical inputs fire a mapping: buttons, press patterns, stick and trigger zones,
//! wheel notches.

use super::*;

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
    h.kbm([
        KbmEvent::Key {
            key: Key::Q,
            down: true,
        },
        KbmEvent::Key {
            key: Key::Q,
            down: false,
        },
    ]);
    // The tap pressed and released A within the tick; outputs were emitted and cleared.
    assert!(!h.virtual_pad().pressed(PadButton::A));
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
    h.kbm([KbmEvent::Wheel {
        dir: WheelDir::Up,
        detents: 1.0,
    }]);
    assert!(h.virtual_pad().pressed(PadButton::Y));
    h.until(100);
    assert!(!h.virtual_pad().pressed(PadButton::Y));
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
