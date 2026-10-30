//! The touchpad, the gyro and the mouse as sources and as outputs.

use super::*;

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
    h.tap(PadButton::Y);
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
        h.kbm([KbmEvent::MouseMove { dx: 2, dy: 0 }]);
    }
    assert!(h.e.is_output_down(Key::M) && !h.e.is_output_down(Key::H));
    for _ in 0..100 {
        h.kbm([KbmEvent::MouseMove { dx: 4, dy: 0 }]);
    }
    assert!(h.e.is_output_down(Key::H), "4000 counts/s is high");
}

#[test]
fn four_way_mouse_directions_follow_speed_like_sticks() {
    let mut p = Profile::default();
    p.mouse.zones = [1000.0, 3000.0, 5000.0];
    p.mappings
        .push(map(Control::MouseMove(Dir::Right), vec![key(Key::R)]));
    p.mappings
        .push(map(Control::MouseMove(Dir::Down), vec![key(Key::D)]));
    p.mappings
        .push(map(Control::MouseZone(None, Zone::Low), vec![key(Key::L)]));
    let mut h = Harness::new(p);
    // Diagonally at almost 1000 counts/s on each axis: about 1400 counts/s in all, above the
    // low zone, so the directions fire with the zone (each axis alone stays below it).
    for _ in 0..100 {
        h.kbm([KbmEvent::MouseMove { dx: 1, dy: 1 }]);
    }
    assert!(h.e.is_output_down(Key::L), "the speed is in the low zone");
    assert!(h.e.is_output_down(Key::R) && h.e.is_output_down(Key::D));
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
        h.step();
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
        h.kbm([KbmEvent::MouseMove { dx: 3, dy: 3 }]);
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
    h.tap(PadButton::X);
    assert!(!moved(&mut h), "off");
    h.tap(PadButton::X);
    assert!(!moved(&mut h), "off stays off");
    h.tap(PadButton::Y);
    assert!(moved(&mut h), "on");
    h.tap(PadButton::B);
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
    h.kbm([KbmEvent::MouseMove { dx: 2, dy: 0 }]);
    let (x, _) = h.virtual_pad().stick(Stick::Right);
    assert!(
        x > 0.9,
        "2 counts in 1 ms = 2000 counts/s = full deflection, got {x}"
    );
}
