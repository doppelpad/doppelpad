//! Radial menus.

use super::*;

fn radial_profile(apply: crate::profile::RadialApply) -> Profile {
    use crate::profile::{RadialMenu, RadialSector};
    let mut p = Profile::default();
    p.mappings.push(shift(b(PadButton::LB), 1, JumpMode::Hold));
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
        h.tap(PadButton::RB);
        h.until(h.t + 30);
    }
    assert_eq!(h.last.radial.unwrap().pointed, Some(2));
    h.tap(PadButton::A);
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
