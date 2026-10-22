//! Shortcuts: controls pressed together within the combo window.

use super::*;

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
