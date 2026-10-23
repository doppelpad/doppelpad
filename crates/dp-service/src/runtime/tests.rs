use std::time::Duration;

use dp_core::engine::Output;
use dp_core::hotkeys::Hotkeys;
use dp_core::pad::{PadButton, Rgb, Trigger};
use dp_core::profile::{Command, TriggerEffect, VirtualPadType};
use dp_platform::PadFamily;

use super::echo::{ECHO_MOUSE_MS, Echo};
use super::testing;
use super::*;

fn ds4_profile() -> Profile {
    let mut p = Profile::default();
    p.output.virtual_pad = VirtualPadType::DualShock4;
    p
}

#[test]
fn hotkey_buttons_do_not_reach_the_game() {
    use dp_core::pad::PadButton::*;
    let (mut rt, world) = testing::runtime_with(&[("pad1", PadFamily::Xbox)]);
    rt.set_hotkeys(Hotkeys {
        hold_ms: 50,
        ..Default::default()
    });
    rt.apply(Profile::default(), None).unwrap();
    let mut press = |buttons: u32| {
        world.lock().unwrap().pads[0].state.buttons = buttons;
        rt.tick();
        world.lock().unwrap().virtual_pads[0].1.unwrap().buttons
    };
    assert_eq!(press(LB.bit()), LB.bit(), "a partial press passes through");
    let combo = LB.bit() | RB.bit() | X.bit();
    assert_eq!(press(combo | DpadUp.bit()), DpadUp.bit(), "hotkey withheld");
    assert_eq!(press(LB.bit() | RB.bit()), 0, "until released");
    assert_eq!(press(0), 0);
    assert_eq!(press(X.bit()), X.bit(), "free again");
}

#[test]
fn hotkeys_confirm_with_a_rumble_pattern_when_asked() {
    use dp_core::pad::PadButton::*;
    let (mut rt, world) = testing::runtime_with(&[("pad1", PadFamily::Xbox)]);
    rt.set_hotkeys(Hotkeys {
        hold_ms: 50,
        pad_overlay: vec![Back, Start],
        rumble: true,
        ..Default::default()
    });
    rt.apply(Profile::default(), None).unwrap();
    world.lock().unwrap().pads[0].state.buttons = Back.bit() | Start.bit();
    rt.tick();
    rt.advance_ms(60);
    rt.tick();
    world.lock().unwrap().pads[0].state.buttons = 0;
    let on = |w: &testing::Shared| {
        w.lock()
            .unwrap()
            .rumble
            .iter()
            .filter(|(_, r)| r.low > 0.0)
            .count()
    };
    assert_eq!(on(&world), 1, "one short pulse for an overlay hotkey");
    // The game's rumble waits until the pattern is over, then the session sends it again.
    rt.advance_ms(100);
    rt.tick();
    let last = world.lock().unwrap().rumble.last().cloned().unwrap();
    assert_eq!(last.1, Rumble::default(), "the motors stop after the pulse");
    assert!(!rt.pulses.playing(&DeviceId("pad1".into())));
}

#[test]
fn the_navigation_hotkey_counts_while_the_app_has_focus() {
    use dp_core::pad::PadButton::*;
    let (mut rt, world) = testing::runtime_with(&[("pad1", PadFamily::Xbox)]);
    rt.set_hotkeys(Hotkeys {
        hold_ms: 50,
        app_navigation: vec![Back, Guide],
        ..Default::default()
    });
    rt.apply(Profile::default(), None).unwrap();
    // The app has the focus: remapping is suspended, hotkeys still count.
    rt.set_suspended(true);
    let hold = |rt: &mut Runtime, buttons: u32| {
        world.lock().unwrap().pads[0].state.buttons = buttons;
        rt.tick();
        rt.advance_ms(60);
        rt.tick();
        rt.navigation_presses()
    };
    assert_eq!(hold(&mut rt, Back.bit() | Guide.bit()), 1);
    assert_eq!(hold(&mut rt, 0), 1);
    assert_eq!(hold(&mut rt, Back.bit() | Guide.bit()), 2);
}

#[test]
fn hotkeys_stay_quiet_without_the_setting() {
    use dp_core::pad::PadButton::*;
    let (mut rt, world) = testing::runtime_with(&[("pad1", PadFamily::Xbox)]);
    rt.set_hotkeys(Hotkeys {
        hold_ms: 50,
        pad_overlay: vec![Back, Start],
        ..Default::default()
    });
    rt.apply(Profile::default(), None).unwrap();
    world.lock().unwrap().pads[0].state.buttons = Back.bit() | Start.bit();
    rt.tick();
    rt.advance_ms(60);
    rt.tick();
    rt.advance_ms(300);
    rt.tick();
    assert!(
        world
            .lock()
            .unwrap()
            .rumble
            .iter()
            .all(|(_, r)| r.low == 0.0)
    );
}

#[test]
fn picking_another_controller_moves_the_hiding() {
    let (mut rt, world) =
        testing::runtime_with(&[("pro", PadFamily::SwitchPro), ("ds", PadFamily::DualSense)]);
    rt.apply(Profile::default(), None).unwrap();
    rt.tick();
    assert_eq!(world.lock().unwrap().hidden, ["path-pro"]);

    rt.set_preferred_device(Some(DeviceId("ds".into())));
    rt.tick();
    assert_eq!(rt.primary_device(), Some(&DeviceId("ds".into())));
    assert_eq!(
        world.lock().unwrap().hidden,
        ["path-ds"],
        "the unused pad is visible again"
    );
}

#[test]
fn mirrors_are_used_only_when_picked() {
    let (mut rt, world) =
        testing::runtime_with(&[("steam", PadFamily::Xbox), ("ds", PadFamily::DualSense)]);
    std::sync::Arc::make_mut(&mut world.lock().unwrap().pads[0].info).mirror = true;
    rt.apply(Profile::default(), None).unwrap();
    rt.tick();
    assert_eq!(
        rt.primary_device(),
        Some(&DeviceId("ds".into())),
        "not the mirror listed first"
    );

    let ds = world.lock().unwrap().pads.remove(1);
    rt.tick();
    assert_eq!(rt.primary_device(), None, "never the mirror on its own");

    rt.set_preferred_device(Some(DeviceId("steam".into())));
    rt.tick();
    assert_eq!(rt.primary_device(), Some(&DeviceId("steam".into())));

    // Picked, it stays, as any other controller would.
    world.lock().unwrap().pads.push(ds);
    rt.tick();
    assert_eq!(rt.primary_device(), Some(&DeviceId("steam".into())));
}

#[test]
fn default_session_returns_to_its_controller_after_a_drop() {
    use dp_core::pad::PadButton;
    let (mut rt, world) =
        testing::runtime_with(&[("pro", PadFamily::SwitchPro), ("ds", PadFamily::DualSense)]);
    rt.apply(Profile::default(), None).unwrap();
    rt.tick();
    let pro = world.lock().unwrap().pads.remove(0);
    rt.tick();
    assert_eq!(
        rt.primary_device(),
        Some(&DeviceId("ds".into())),
        "falls back"
    );
    assert_eq!(world.lock().unwrap().hidden, ["path-ds"]);

    // Back (now listed after the other pad): the session returns to it.
    world.lock().unwrap().pads.push(pro);
    rt.tick();
    assert_eq!(rt.primary_device(), Some(&DeviceId("pro".into())));
    assert_eq!(world.lock().unwrap().hidden, ["path-pro"]);
    world.lock().unwrap().pads[1].state.set(PadButton::A, true);
    rt.tick();
    let out = world.lock().unwrap().virtual_pads[0].1.unwrap();
    assert!(
        out.pressed(PadButton::A),
        "its input reaches the virtual pad"
    );

    // Once back, a later drop of the other pad changes nothing.
    world.lock().unwrap().pads.remove(0);
    rt.tick();
    assert_eq!(rt.primary_device(), Some(&DeviceId("pro".into())));
}

#[test]
fn held_keys_repeat_on_the_virtual_keyboard() {
    use dp_core::keys::Key;
    use dp_core::pad::PadButton;
    use dp_core::profile::{Action, KeyRepeat, Mapping, Press};
    let (mut rt, world) = testing::runtime_with(&[("pad1", PadFamily::Xbox)]);
    let mut p = Profile::default();
    p.output.virtual_pad = VirtualPadType::None;
    p.virtual_devices.keyboard.repeat = KeyRepeat::Every(5);
    p.mappings.push(Mapping::new(
        dp_core::control::Control::Button(PadButton::A),
        vec![Action::Key {
            key: Key::Space,
            action: Press::Auto,
        }],
    ));
    rt.apply(p, None).unwrap();
    rt.tick();
    world.lock().unwrap().pads[0].state.set(PadButton::A, true);
    rt.tick();
    rt.advance_ms(30);
    rt.tick();
    world.lock().unwrap().pads[0].state.set(PadButton::A, false);
    rt.tick();
    rt.advance_ms(30);
    rt.tick();
    let keys = world.lock().unwrap().keys.clone();
    assert_eq!(keys.first(), Some(&(Key::Space, true)));
    assert_eq!(keys.last(), Some(&(Key::Space, false)), "{keys:?}");
    assert!(
        keys.iter().filter(|k| k.1).count() >= 2,
        "the held key repeated: {keys:?}"
    );
}

#[test]
fn system_commands_reach_the_platform() {
    use dp_core::pad::PadButton;

    let (mut rt, world) = testing::runtime_with(&[("pad1", PadFamily::Xbox)]);
    let mut p = Profile::default();
    p.mappings.push(testing::command_mapping(
        PadButton::A,
        &[Command::Screenshot, Command::CloseWindow, Command::Sleep],
    ));
    rt.apply(p, None).unwrap();
    rt.tick();
    world.lock().unwrap().pads[0].state.set(PadButton::A, true);
    for _ in 0..3 {
        rt.tick();
        rt.advance_ms(15);
    }
    assert_eq!(
        world.lock().unwrap().system,
        ["Screenshot", "CloseWindow", "Sleep"]
    );
}

#[test]
fn overlay_commands_toggle_the_overlays() {
    use dp_core::pad::PadButton;

    let (mut rt, world) = testing::runtime_with(&[("pad1", PadFamily::Xbox)]);
    let mut p = Profile::default();
    p.mappings.push(testing::command_mapping(
        PadButton::A,
        &[Command::PadOverlay, Command::MappingsOverlay],
    ));
    rt.apply(p, None).unwrap();
    rt.tick();
    assert_eq!(rt.overlays(), (false, false));
    world.lock().unwrap().pads[0].state.set(PadButton::A, true);
    for _ in 0..3 {
        rt.tick();
        rt.advance_ms(15);
    }
    assert_eq!(rt.overlays(), (true, true));
}

#[test]
fn stays_awake_while_a_mapping_holds_output() {
    use dp_core::control::Control;
    use dp_core::pad::PadButton;
    use dp_core::profile::{Action, Mapping};
    let (mut rt, world) = testing::runtime_with(&[("pad1", PadFamily::Xbox)]);
    let m = Mapping::new(
        Control::Button(PadButton::A),
        vec![Action::Key {
            key: dp_core::keys::Key::Space,
            action: Default::default(),
        }],
    );
    let mut p = Profile::default();
    p.output.virtual_pad = VirtualPadType::None;
    p.mappings.push(m);
    rt.apply(p, None).unwrap();
    rt.tick();
    let mut press = |on: bool| {
        world.lock().unwrap().pads[0].state.set(PadButton::A, on);
        for _ in 0..3 {
            rt.tick();
            rt.advance_ms(15);
        }
    };
    press(true);
    let awake = |w: &testing::Shared| {
        w.lock()
            .unwrap()
            .system
            .iter()
            .filter(|s| s.starts_with("awake"))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(awake(&world), ["awake true"], "Space is held");
    press(false);
    assert_eq!(awake(&world), ["awake true", "awake false"]);
}

#[test]
fn power_off_command_and_idle_timeout() {
    use dp_core::pad::PadButton;

    let (mut rt, world) = testing::runtime_with(&[("pad1", PadFamily::DualSense)]);
    let mut p = Profile::default();
    p.mappings
        .push(testing::command_mapping(PadButton::A, &[Command::PowerOff]));
    rt.apply(p, None).unwrap();
    rt.tick();
    world.lock().unwrap().pads[0].state.set(PadButton::A, true);
    for _ in 0..3 {
        rt.tick();
        rt.advance_ms(15);
    }
    let off = std::mem::take(&mut world.lock().unwrap().powered_off);
    assert_eq!(
        off,
        [DeviceId("pad1".into())],
        "the command turns its controller off"
    );

    rt.set_power_off_after(Some(Duration::from_millis(60)));
    // `wiggle` swings the stick every tick; otherwise it rests with a little drift.
    let mut run_for = |ms: u64, wiggle: bool| {
        let mut flip = 1.0;
        for _ in 0..ms / 5 {
            flip = -flip;
            world.lock().unwrap().pads[0].state.axes[0] = if wiggle { 0.9 * flip } else { 0.1 };
            rt.tick();
            rt.advance_ms(5);
        }
        std::mem::take(&mut world.lock().unwrap().powered_off).len()
    };
    assert_eq!(run_for(150, true), 0, "input keeps it on");
    assert_eq!(run_for(150, false), 1, "drift is no input");
    assert_eq!(run_for(150, false), 0, "only once");
    assert_eq!(run_for(20, true), 0);
    assert_eq!(run_for(150, false), 1, "input starts the count again");
}

#[test]
fn crosshair_commands_choose_and_hide() {
    use dp_core::pad::PadButton;

    let (mut rt, world) = testing::runtime_with(&[("pad1", PadFamily::Xbox)]);
    let mut p = Profile::default();
    for name in ["dot", "scope"] {
        p.crosshairs.push(Crosshair {
            name: name.into(),
            ..Default::default()
        });
    }
    p.crosshairs[0].layers = vec![0];
    for (b, command) in [
        (PadButton::A, Command::Crosshair(2)),
        (PadButton::B, Command::ToggleCrosshair),
    ] {
        p.mappings.push(testing::command_mapping(b, &[command]));
    }
    rt.apply(p, None).unwrap();
    rt.tick();
    let name = |rt: &Runtime| rt.crosshair().map(|c| c.name.clone());
    assert_eq!(name(&rt).as_deref(), Some("dot"), "bound to the base layer");
    let press = |rt: &mut Runtime, b: PadButton| {
        for down in [true, false] {
            world.lock().unwrap().pads[0].state.set(b, down);
            for _ in 0..3 {
                rt.tick();
                rt.advance_ms(15);
            }
        }
    };
    press(&mut rt, PadButton::A);
    assert_eq!(name(&rt).as_deref(), Some("scope"));
    press(&mut rt, PadButton::A);
    assert_eq!(name(&rt), None, "the same command again hides it");
    press(&mut rt, PadButton::B);
    assert_eq!(name(&rt).as_deref(), Some("scope"), "toggle brings it back");
    press(&mut rt, PadButton::B);
    assert_eq!(name(&rt), None);
}

#[test]
fn profile_light_bar_follows_the_layer_and_indicators_change_once() {
    use dp_core::control::Control;
    use dp_core::pad::PadButton;
    use dp_core::profile::{Jump, JumpMode, Layer, Light, LightMode, Mapping};
    let (mut rt, world) = testing::runtime_with(&[("pad1", PadFamily::DualSense)]);
    let mut p = ds4_profile();
    let solid = |c: &str| Light {
        mode: LightMode::Solid,
        colors: vec![c.into()],
        ..Default::default()
    };
    p.lightbar = Some(solid("#ff0000"));
    p.layers.push(Layer {
        id: 1,
        lightbar: Some(solid("#0000ff")),
        ..Default::default()
    });
    let mut m = Mapping::new(Control::Button(PadButton::LB), Vec::new());
    m.jump = Some(Jump {
        layer: 1,
        mode: JumpMode::Hold,
        delay_ms: None,
        postpone: false,
    });
    p.mappings.push(m);
    rt.apply(p, None).unwrap();
    rt.tick();
    let last = |w: &testing::Shared| w.lock().unwrap().lightbar.last().and_then(|l| l.1);
    assert_eq!(last(&world), Some(Rgb { r: 255, g: 0, b: 0 }));
    world.lock().unwrap().pads[0].state.set(PadButton::LB, true);
    rt.tick();
    rt.tick();
    assert_eq!(
        last(&world),
        Some(Rgb { r: 0, g: 0, b: 255 }),
        "the layer's colour"
    );

    let leds = Leds {
        player: Some(2),
        mic: Some(true),
    };
    rt.set_indicators(|_| leds);
    rt.set_indicators(|_| leds);
    assert_eq!(
        world.lock().unwrap().leds,
        [(DeviceId("pad1".into()), leds)]
    );
}

#[test]
fn trigger_effects_follow_the_profile_and_end_with_it() {
    let (mut rt, world) = testing::runtime_with(&[("pad1", PadFamily::DualSense)]);
    let mut p = ds4_profile();
    let rigid = TriggerEffect::Resistance {
        start: 2,
        strength: 6,
    };
    p.triggers.right.effect = Some(rigid);
    rt.apply(p, None).unwrap();
    rt.tick();
    rt.tick();
    let id = DeviceId("pad1".into());
    assert_eq!(
        world.lock().unwrap().triggers,
        [(id.clone(), [None, Some(rigid)])],
        "sent once"
    );
    rt.stop();
    assert_eq!(
        world.lock().unwrap().triggers.last(),
        Some(&(id, [Some(TriggerEffect::Off); 2]))
    );
}

#[test]
fn game_feedback_reaches_the_controller() {
    let (mut rt, world) = testing::runtime_with(&[("pad1", PadFamily::DualSense)]);
    rt.apply(ds4_profile(), None).unwrap();
    rt.tick();

    let red = Rgb { r: 255, g: 0, b: 0 };
    let buzz = Rumble {
        low: 1.0,
        high: 0.5,
    };
    {
        let mut w = world.lock().unwrap();
        w.game_rumble = Some(buzz);
        w.game_lightbar = Some(red);
    }
    rt.tick();
    // Rumble commands keep the profile's gap (20 ms by default).
    rt.advance_ms(25);
    rt.tick();
    {
        let w = world.lock().unwrap();
        let id = DeviceId("pad1".into());
        assert_eq!(w.rumble.last(), Some(&(id.clone(), buzz)));
        assert_eq!(w.lightbar.last(), Some(&(id, Some(red))));
        assert_eq!(
            w.lightbar.iter().filter(|(_, c)| *c == Some(red)).count(),
            1,
            "sent once"
        );
        assert_eq!(w.virtual_pads[0].0, VirtualPadKind::DualShock4);
        assert!(
            w.virtual_pads[0].1.is_some(),
            "the virtual pad gets reports"
        );
    }

    rt.stop();
    let w = world.lock().unwrap();
    let id = DeviceId("pad1".into());
    assert_eq!(w.rumble.last(), Some(&(id.clone(), Rumble::default())));
    assert_eq!(w.lightbar.last(), Some(&(id, None)), "light bar released");
}

#[test]
fn echo_reports_changes_once() {
    use dp_core::keys::Key;
    let mut e = Echo::default();
    let mut p = PadState::default();
    p.set(PadButton::A, true);
    p.set_axis(Trigger::Right.axis(), 0.5);
    let out = Output {
        keys: vec![(Key::Space, true)],
        mouse_move: (3.0, 0.0),
        ..Default::default()
    };
    e.record(Some(&p), &out, 0);
    e.record(Some(&p), &Output::default(), 10);
    e.record(None, &Output::default(), ECHO_MOUSE_MS);
    assert_eq!(
        e.events,
        [
            format!("controller  {:<9} pressed", PadButton::A.name()),
            "controller  RT        pressed".into(),
            format!("  -> key           {} down", Key::Space),
            "  -> mouse moved   +3, +0 px".into(),
        ]
    );
}
