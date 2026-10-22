use dp_core::control::{Control, Dir, TouchGesture, Zone};
use dp_core::keys::Key;
use dp_core::pad::{PadButton, Stick, Trigger};
use dp_core::profile::{
    Action, Command, JitterPattern, KbmSource, KeyRepeat, LightMode, Magnifier, Mode, Press,
    RadialApply, RadialSelect, ShakeAxis, Unlock, Vibration, VirtualPadType,
};

use super::ids::{gamepad_control, key_from_description};
use super::*;

const LIZARD_LIKE: &str = r#"{
  "schemaVersion": 3.0,
  "appVersion": "6.2.1",
  "config": { "appName": "Lizard", "minTimeUnit": 1, "processNames": ["game.exe", ""] },
  "devices": {
    "hardware": [ { "id": 1, "name": "gamepad", "default": { "triggers": [ { "name": "left", "zones": [ { "name": "low", "x": 3277 } ], "highZoneMax": 32000 } ] } } ],
    "virtual": { "gamepads": [ { "id": 1, "type": "ds4" } ], "keyboard": { "config": { "repeatMode": "fixed", "repeatValue": 50 } } }
  },
  "masks": [
    { "id": 1, "set": [ { "deviceId": 1, "buttonId": 1, "description": "A" } ] },
    { "id": 2, "set": [ { "deviceId": 1, "buttonId": 5 }, { "deviceId": 1, "buttonId": 6 } ] },
    { "id": 3, "set": [ { "deviceId": 1, "buttonId": 47 } ] },
    { "id": 4, "set": [ { "deviceId": 1, "buttonId": 170 } ] }
  ],
  "mappings": [
    { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } },
      "macros": [ { "keyboard": { "buttonId": 28, "description": "DIK_RETURN" } } ] },
    { "condition": { "mask": { "id": 2, "activator": { "type": "LONG", "mode": "onetime", "params": { "longwaittime": 500 } } } },
      "macros": [ { "keyboard": { "buttonId": 1, "description": "DIK_ESCAPE", "action": "down" } },
                  { "pause": { "value": 30 } }, { "pause": { "value": 20 } },
                  { "keyboard": { "buttonId": 1, "description": "DIK_ESCAPE", "action": "up" } } ] },
    { "condition": { "mask": { "id": 3, "activator": { "type": "single", "mode": "hold_until_release" } } },
      "macros": [ { "mouse": { "direction": "down" } } ] },
    { "condition": { "mask": { "id": 4, "activator": { "type": "single", "mode": "turbo", "params": { "pause": 8 } } } },
      "macros": [ { "mouse": { "buttonId": 1 } } ] }
  ]
}"#;

#[test]
fn imports_core_features() {
    let (p, diags) = import(LIZARD_LIKE).unwrap();
    assert!(!diags.has_errors(), "{:?}", diags);
    assert_eq!(p.name, "Lizard");
    assert_eq!(p.processes, vec!["game.exe"]);
    assert_eq!(p.output.virtual_pad, VirtualPadType::DualShock4);
    assert_eq!(p.virtual_devices.keyboard.repeat, KeyRepeat::Slow);
    assert!((p.triggers.left.zones.low - 0.1).abs() < 0.01);
    assert_eq!(
        p.triggers.left.deadzone, 0.0,
        "zones shape the output only with affectHardware"
    );
    assert_eq!(p.mappings.len(), 4);

    let a = &p.mappings[0];
    assert_eq!(a.trigger, Combo(vec![Control::Button(PadButton::A)]));
    assert_eq!(
        a.actions,
        vec![Action::Key {
            key: Key::Enter,
            action: Press::Auto
        }]
    );

    let combo = &p.mappings[1];
    assert_eq!(
        combo.trigger.0,
        vec![
            Control::Button(PadButton::LB),
            Control::Button(PadButton::RB)
        ]
    );
    // Escape down, pauses (merged), Escape up in one-shot mode collapses to a plain remap.
    assert_eq!(
        (combo.activator, combo.mode, combo.long_press_ms),
        (Activator::Long, Mode::Hold, Some(500))
    );
    assert_eq!(
        combo.actions,
        vec![Action::Key {
            key: Key::Escape,
            action: Press::Auto
        }]
    );

    // "down" in the file format moves the cursor up on screen.
    assert_eq!(
        p.mappings[2].actions[0],
        Action::MouseMove {
            mouse_move: Dir::Up,
            speed: 1000.0,
            acceleration: 4.0
        },
        "the format's default speed and acceleration"
    );
    assert_eq!(
        p.mappings[3].trigger.0[0],
        Control::TriggerFull(Trigger::Right)
    );
    assert_eq!(
        p.mappings[3].turbo_ms,
        Some(10),
        "turbo period clamps to 10 ms"
    );
}

#[test]
fn time_unit_scaling() {
    let doc = r#"{ "schemaVersion": 3, "appVersion": "1.0.0", "config": { "appName": "x" },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" } ] },
      "masks": [ { "id": 1, "set": [ { "deviceId": 1, "buttonId": 2 } ] } ],
      "mappings": [ { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "onetime" } } },
                      "macros": [ { "pause": { "value": 5 } } ] } ] }"#;
    let (p, _) = import(doc).unwrap();
    assert_eq!(
        p.mappings[0].actions[0],
        Action::Pause { pause: 50 },
        "minTimeUnit absent = 10 ms units"
    );
}

#[test]
fn send_input_devices_select_script_input() {
    let doc = |devices: &str| {
        format!(
            r#"{{ "schemaVersion": 3, "appVersion": "1.0.0", "config": {{ "appName": "x" }},
              "devices": {{ "hardware": [ {devices} ] }} }}"#
        )
    };
    let (p, _) = import(&doc(
        r#"{ "id": 1, "name": "gamepad" }, { "id": 2, "name": "sendInputKeyboard" }"#,
    ))
    .unwrap();
    assert_eq!(p.kbm_source, KbmSource::Scripts);
    let text = crate::to_toml(&p).unwrap();
    assert!(text.contains(r#"kbm_source = "scripts""#), "{text}");
    let (p, d) = import(&doc(
        r#"{ "id": 1, "name": "keyboard" }, { "id": 2, "name": "sendInputMouse" }"#,
    ))
    .unwrap();
    assert_eq!(p.kbm_source, KbmSource::All, "both: everything counts");
    assert!(d.iter().any(|x| x.to_string().contains("injected input")));
    let (p, _) = import(&doc(r#"{ "id": 1, "name": "keyboard" }"#)).unwrap();
    assert!(
        !crate::to_toml(&p).unwrap().contains("kbm_source"),
        "the default is not written"
    );
}

#[test]
fn break_markers_become_segments() {
    let doc = r#"{ "schemaVersion": 3, "appVersion": "1.0.0", "config": { "appName": "x", "minTimeUnit": 1 },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" } ] },
      "masks": [ { "id": 1, "set": [ { "deviceId": 1, "buttonId": 2 } ] } ],
      "mappings": [ { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "turbo", "params": { "pause": 50 } } } },
                      "macros": [ { "keyboard": { "buttonId": 16, "action": "down" } }, { "break": {} }, { "break": {} },
                                  { "keyboard": { "buttonId": 18, "action": "down" } } ] } ] }"#;
    let (p, d) = import(doc).unwrap();
    let m = &p.mappings[0];
    assert_eq!(m.actions.len(), 3, "{:?}", m.actions);
    assert!(m.actions[1].is_break(), "repeated markers collapse");
    assert_eq!(
        (m.mode, m.turbo_ms),
        (Mode::Hold, None),
        "turbo is cancelled"
    );
    assert!(!d.iter().any(|x| x.to_string().contains("break")));
    // The TOML form round-trips.
    let text = crate::to_toml(&p).unwrap();
    assert!(text.contains("break = true"), "{text}");
    assert_eq!(
        crate::parse_toml(&text).unwrap().mappings[0].actions,
        m.actions
    );
}

#[test]
fn system_commands_map_to_ours() {
    let doc = r#"{ "schemaVersion": 3, "appVersion": "1.0.0", "config": { "appName": "x", "minTimeUnit": 1 },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" } ] },
      "commands": [ { "id": 1, "type": 11 }, { "id": 2, "type": 16 }, { "id": 3, "type": 10, "name": "COMMAND_TURN_ANTIBOSS_MODE" } ],
      "masks": [ { "id": 1, "set": [ { "deviceId": 1, "buttonId": 2 } ] } ],
      "mappings": [ { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "onetime" } } },
                      "macros": [ { "command": { "id": 1 } }, { "command": { "id": 2 } }, { "command": { "id": 3 } } ] } ] }"#;
    let (p, _) = import(doc).unwrap();
    let commands: Vec<&Command> = p.mappings[0]
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::Command { command } => Some(command),
            _ => None,
        })
        .collect();
    assert_eq!(
        commands,
        [
            &Command::Screenshot,
            &Command::GyroOff,
            &Command::Unsupported("COMMAND_TURN_ANTIBOSS_MODE".into())
        ]
    );
}

#[test]
fn keyboard_device_masks_and_layers() {
    let doc = r#"{ "schemaVersion": 3.0, "appVersion": "9.2.0", "config": { "appName": null, "minTimeUnit": 1 },
      "devices": { "groups": [ { "id": 1, "name": "keyboard" } ],
                   "hardware": [ { "id": 1, "name": "gamepad" }, { "id": 11, "name": "keyboard", "type": "standard", "groupId": 1 } ] },
      "shifts": [ { "id": 1, "type": "toggle", "set": [ { "deviceId": 1, "buttonId": 7 } ], "unheritableMasks": [ 1 ] } ],
      "masks": [ { "id": 1, "set": [ { "deviceId": 11, "buttonId": 57, "description": "DIK_SPACE" } ] } ],
      "mappings": [ { "condition": { "shiftId": 1, "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } },
                      "macros": [ { "gamepad": { "id": 1, "buttonId": 1 } } ] } ] }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors());
    assert_eq!(p.name, "Game", "null appName falls back");
    assert_eq!(p.layers.len(), 1);
    assert_eq!(
        p.layers[0].not_inherited,
        vec![Combo(vec![Control::Key(Key::Space)])]
    );
    let modifier = p.mappings.iter().find(|m| m.jump.is_some()).unwrap();
    assert_eq!(modifier.trigger.0, vec![Control::Button(PadButton::Back)]);
    assert_eq!(modifier.jump.as_ref().unwrap().mode, JumpMode::Toggle);
    let space = p.mappings.iter().find(|m| m.layer == 1).unwrap();
    assert_eq!(
        space.actions,
        vec![Action::Pad {
            pad: PadButton::A,
            action: Press::Auto
        }]
    );
}

#[test]
fn jump_pairs_fold_into_hold_shift() {
    let doc = r#"{ "schemaVersion": 3.0, "appVersion": "9.2.0", "config": { "appName": "x", "minTimeUnit": 1 },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" } ] },
      "shifts": [ { "id": 1, "type": "default" } ],
      "masks": [ { "id": 1, "set": [ { "deviceId": 1, "buttonId": 7 } ] } ],
      "mappings": [
        { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } }, "jumpToLayer": { "layer": 1 } },
        { "condition": { "shiftId": 1, "mask": { "id": 1, "activator": { "type": "release", "mode": "onetime" } } }, "jumpToLayer": { "layer": 0 } }
      ] }"#;
    let (p, _) = import(doc).unwrap();
    assert_eq!(p.mappings.len(), 1);
    assert_eq!(p.mappings[0].jump.as_ref().unwrap().mode, JumpMode::Hold);
}

#[test]
fn vibration_strengths_swap_and_gap() {
    let doc = r#"{ "schemaVersion": 3.0, "appVersion": "9.2.0", "config": { "appName": "x", "minTimeUnit": 1 },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad", "default": {
        "vibration": [ { "motor": { "name": "left", "amplitude": 60, "insteadMotorRight": true } },
                       { "motor": { "name": "right", "turnedOff": true, "insteadMotorLeft": true } } ],
        "vibrationRumbleDelay": 40 } } ] } }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors());
    assert_eq!(
        p.vibration,
        Vibration {
            left: 0.6,
            right: 0.0,
            swap: true,
            min_delay_ms: 40
        }
    );
}

#[test]
fn light_bars_of_the_profile_and_layers() {
    let doc = r#"{ "schemaVersion": 3.0, "appVersion": "9.2.0",
      "config": { "appName": "x", "minTimeUnit": 1,
        "led": { "default": { "mode": "solid", "colors": [[255, 0, 0]] },
                 "shifts": [ { "shiftId": 1, "mode": "stroboscope", "colors": [[0, 0, 255], [0, 255, 0]], "loop": true } ] } },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" } ] },
      "shifts": [ { "id": 1, "type": "default" } ] }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors());
    let base = p.lightbar.unwrap();
    assert_eq!(
        (base.mode, base.colors),
        (LightMode::Solid, vec!["#ff0000".to_string()])
    );
    let l = p.layers[0].lightbar.as_ref().unwrap();
    assert_eq!(
        (l.mode, l.cycle, l.colors.len()),
        (LightMode::Strobe, true, 2)
    );
}

#[test]
fn radial_menus_sectors_and_navigation() {
    let doc = r#"{ "schemaVersion": 3.0, "appVersion": "9.2.0", "config": { "appName": "x", "minTimeUnit": 1 },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" } ] },
      "shifts": [ { "id": 2, "type": "radialMenu" } ],
      "radialMenuCircles": [ { "id": 1, "sectors": [ 1, 2 ], "isTintedBackground": true, "tintBackground": 40 },
                             { "id": 2, "parentSectorId": 2, "sectors": [ 3 ] } ],
      "radialMenuSectors": [ { "id": 1, "parentCircleId": 1, "description": "Heal", "color": [0, 255, 0] },
                             { "id": 2, "parentCircleId": 1, "childCircleId": 2, "description": "More" },
                             { "id": 3, "parentCircleId": 2, "description": "Map", "isCloseOnApply": false } ],
      "masks": [ { "id": 1, "set": [ { "deviceId": 1, "buttonId": 1 } ] },
                 { "id": 5, "radialMenuSet": [ { "circleId": 1, "sectorId": 1 } ] } ],
      "commands": [ { "id": 1, "type": 131 }, { "id": 2, "type": 132 } ],
      "mappings": [
        { "condition": { "mask": { "id": 5, "activator": { "type": "single", "mode": "onetime" } } },
          "macros": [ { "keyboard": { "buttonId": 18, "description": "DIK_E" } } ] },
        { "condition": { "shiftId": 2, "mask": { "id": 1, "activator": { "type": "single", "mode": "onetime" } } },
          "macros": [ { "command": { "id": 1 } } ] }
      ] }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors());
    assert_eq!(p.radial_menus.len(), 2);
    let root = &p.radial_menus[0];
    assert_eq!((root.layer, root.tint), (2, 0.4));
    assert_eq!(
        root.select,
        RadialSelect::Commands,
        "navigated by a command"
    );
    assert_eq!(root.apply, RadialApply::Release, "no apply command mapped");
    assert_eq!(root.sectors[0].label, "Heal");
    assert_eq!(root.sectors[0].color, "#00ff00");
    assert!(matches!(
        root.sectors[0].actions[..],
        [Action::Key { key: Key::E, .. }]
    ));
    assert_eq!(root.sectors[1].submenu, Some(2));
    assert_eq!(
        p.radial_menus[1].layer, 0,
        "a sub-menu has no layer of its own"
    );
    assert!(!p.radial_menus[1].sectors[0].close);
    assert_eq!(
        p.mappings.len(),
        1,
        "the sector mask has no mapping of its own"
    );
    assert_eq!(
        p.mappings[0].actions,
        [Action::Command {
            command: Command::RadialNext
        }]
    );
}

#[test]
fn crosshairs_masks_and_commands() {
    let doc = r#"{ "schemaVersion": 3.0, "appVersion": "9.2.0", "config": { "appName": "x", "minTimeUnit": 1 },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" } ] },
      "shifts": [ { "id": 1, "type": "default" } ],
      "crosshairs": [
        { "id": 2, "guid": "b", "name": "Scope", "crosshairType": "whatever", "color": [255, 0, 0], "opacity": 50,
          "size": 40, "thickness": 30, "edgingThickness": 10, "horizontalDelta": 5, "verticalDelta": -3,
          "useMagnifier": true, "magnification": 4, "magnificationSizePercents": 30 },
        { "id": 1, "guid": "a", "name": "Dot" } ],
      "masks": [ { "id": 1, "set": [ { "deviceId": 1, "buttonId": 1 } ] },
                 { "id": 2, "crosshairSet": [ { "crosshairGuid": "b", "layer": 1 } ] } ],
      "commands": [ { "id": 1, "type": 102 }, { "id": 2, "type": 120 } ],
      "mappings": [ { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "onetime" } } },
                      "macros": [ { "command": { "id": 1 } }, { "command": { "id": 2 } } ] },
                    { "condition": { "mask": { "id": 2, "activator": { "type": "single", "mode": "onetime" } } } } ] }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors());
    let names: Vec<&str> = p.crosshairs.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Dot", "Scope"], "in id order");
    let scope = &p.crosshairs[1];
    assert_eq!(
        (
            scope.color.as_str(),
            scope.opacity,
            scope.size,
            scope.thickness,
            scope.outline
        ),
        ("#ff0000", 0.5, 40, 30, 2)
    );
    assert_eq!(scope.offset, [5, -3]);
    assert_eq!(scope.layers, [1]);
    assert_eq!(
        scope.magnifier,
        Some(Magnifier {
            zoom: 4.0,
            size: 0.3
        })
    );
    assert_eq!(
        p.mappings.len(),
        1,
        "the crosshair mask has no mapping of its own"
    );
    assert_eq!(
        p.mappings[0].actions,
        [
            Action::Command {
                command: Command::Crosshair(2)
            },
            Action::Command {
                command: Command::ToggleCrosshair
            }
        ]
    );
}

#[test]
fn unlock_layers_multi_button_modifiers_and_postpone() {
    let doc = r#"{ "schemaVersion": 3.0, "appVersion": "9.2.0", "config": { "appName": "x", "minTimeUnit": 1 },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" } ] },
      "shifts": [ { "id": 1, "type": "unlockMouseOnly", "set": [ { "deviceId": 1, "buttonId": 5 }, { "deviceId": 1, "buttonId": 6 } ] },
                  { "id": 2, "type": "unlock" }, { "id": 3, "type": "unlockKeyboardOnly" } ],
      "masks": [ { "id": 1, "set": [ { "deviceId": 1, "buttonId": 7 } ] } ],
      "mappings": [
        { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } },
          "jumpToLayer": { "layer": 2, "delay": 250, "postpone": true } }
      ] }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors());
    let unlocks: Vec<_> = p.layers.iter().map(|l| l.unlock).collect();
    assert_eq!(
        unlocks,
        vec![
            Some(Unlock::Mouse),
            Some(Unlock::All),
            Some(Unlock::Keyboard)
        ]
    );
    let modifier = p
        .mappings
        .iter()
        .find(|m| m.jump.as_ref().is_some_and(|j| j.layer == 1))
        .unwrap();
    assert_eq!(modifier.trigger.0.len(), 2, "both buttons held together");
    let delayed = p
        .mappings
        .iter()
        .find_map(|m| m.jump.as_ref().filter(|j| j.layer == 2))
        .unwrap();
    assert_eq!((delayed.delay_ms, delayed.postpone), (Some(250), true));
}

#[test]
fn onetime_press_release_collapses_to_remap() {
    let doc = r#"{ "schemaVersion": 3.0, "appVersion": "9.2.0", "config": { "appName": "x", "minTimeUnit": 1 },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" } ] },
      "masks": [ { "id": 1, "set": [ { "deviceId": 1, "buttonId": 3 } ] } ],
      "mappings": [ { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "onetime", "params": { "singlewaittime": 200, "doublewaittime": 400 } } } },
        "macros": [ { "keyboard": { "buttonId": 18, "description": "DIK_E", "action": "down" } }, { "pause": { "value": 30 } },
                    { "keyboard": { "buttonId": 18, "description": "DIK_E", "action": "up" } } ] } ] }"#;
    let (p, _) = import(doc).unwrap();
    let m = &p.mappings[0];
    assert_eq!(m.mode, Mode::Hold);
    assert_eq!(
        m.actions,
        vec![Action::Key {
            key: Key::E,
            action: Press::Auto
        }]
    );
    assert_eq!((m.double_tap_ms, m.triple_tap_ms), (Some(200), Some(400)));
}

#[test]
fn imports_zones_and_zone_activators() {
    let doc = r#"{ "schemaVersion": 3.0, "appVersion": "9.2.0", "config": { "appName": "x", "minTimeUnit": 1 },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad", "default": {
          "sticks": [ { "name": "left", "zones": [ { "name": "low", "x": 6554, "y": 9830 }, { "name": "med", "x": 16384 },
                                                  { "name": "high", "x": 26214, "y": 29491 } ] },
                      { "name": "right", "zones": [ { "name": "low", "x": 9830 }, { "name": "med", "x": 6554 } ] } ],
          "triggers": [ { "name": "right", "zones": [ { "name": "low", "x": 3277 }, { "name": "high", "x": 22938 } ],
                          "highZoneMax": 32768, "affectHardware": true } ] } } ] },
      "shifts": [ { "id": 1, "type": "default", "unheritableMasks": [ 1 ] } ],
      "masks": [
        { "id": 1, "set": [ { "deviceId": 1, "buttonId": 55 } ] },
        { "id": 2, "set": [ { "deviceId": 1, "buttonId": 40 } ] },
        { "id": 3, "set": [ { "deviceId": 1, "buttonId": 5 }, { "deviceId": 1, "buttonId": 51 } ] },
        { "id": 4, "set": [ { "deviceId": 1, "buttonId": 38 } ] },
        { "id": 5, "set": [ { "deviceId": 1, "buttonId": 130 } ] },
        { "id": 6, "set": [ { "deviceId": 1, "buttonId": 1 } ] }
      ],
      "mappings": [
        { "condition": { "mask": { "id": 1, "activator": { "type": "zoneHigh", "mode": "hold_until_release" } } },
          "macros": [ { "keyboard": { "buttonId": 46, "description": "DIK_C" } } ] },
        { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } },
          "macros": [ { "keyboard": { "buttonId": 25, "description": "DIK_P" } } ] },
        { "condition": { "mask": { "id": 2, "activator": { "type": "zonelow", "mode": "hold_until_release" } } },
          "macros": [ { "keyboard": { "buttonId": 17, "description": "DIK_W" } } ] },
        { "condition": { "mask": { "id": 3, "activator": { "type": "zoneMed", "mode": "hold_until_release" } } },
          "macros": [ { "keyboard": { "buttonId": 50, "description": "DIK_M" } } ] },
        { "condition": { "mask": { "id": 4, "activator": { "type": "single", "mode": "hold_until_release" } } },
          "macros": [ { "keyboard": { "buttonId": 30, "description": "DIK_A" } } ] },
        { "condition": { "mask": { "id": 5, "activator": { "type": "single", "mode": "hold_until_release" } } },
          "macros": [ { "keyboard": { "buttonId": 31, "description": "DIK_S" } } ] },
        { "condition": { "mask": { "id": 6, "activator": { "type": "zoneHigh", "mode": "hold_until_release" } } },
          "macros": [ { "keyboard": { "buttonId": 32, "description": "DIK_D" } } ] }
      ] }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors(), "{diags:?}");
    let near = |a: f32, b: f32| (a - b).abs() < 0.001;

    let left = &p.sticks.left.zones;
    assert!(near(left.low.x, 0.2) && near(left.low.y, 0.3), "{left:?}");
    let med = left.med.unwrap();
    assert!(near(med.x, 0.5) && near(med.y, 0.5), "missing y equals x");
    assert!(near(left.high.unwrap().y, 0.9));
    assert_eq!(
        p.sticks.left.deadzone, 0.0,
        "the low zone is no output deadzone"
    );
    let right = &p.sticks.right.zones;
    assert!(
        near(right.low.x, 0.3) && right.med.is_none(),
        "out-of-order medium zone falls back to its default"
    );
    assert!(diags.0.iter().any(|d| d.message.contains("out of order")));

    let rt = &p.triggers.right;
    assert!(near(rt.zones.low, 0.1) && near(rt.zones.high.unwrap(), 0.7));
    assert_eq!(rt.zones.med, None);
    assert_eq!(rt.full_pull, 1.0);
    assert!(
        near(rt.deadzone, 0.1),
        "affectHardware applies the low zone to the output"
    );

    let trig = |i: usize| p.mappings[i].trigger.0.clone();
    let all = |i: usize| (p.mappings[i].activator, p.mappings[i].mode);
    assert_eq!(
        trig(0),
        vec![Control::TriggerZone(Trigger::Right, Zone::High)]
    );
    assert_eq!(all(0), (Activator::Single, Mode::Hold));
    assert_eq!(trig(1), vec![Control::TriggerPress(Trigger::Right)]);
    assert_eq!(
        trig(2),
        vec![Control::StickZone(Stick::Left, Some(Dir::Up), Zone::Low)]
    );
    assert_eq!(
        trig(3),
        vec![
            Control::Button(PadButton::LB),
            Control::TriggerZone(Trigger::Left, Zone::Med)
        ]
    );
    assert_eq!(
        trig(4),
        vec![Control::StickZone(Stick::Left, None, Zone::Med)]
    );
    assert_eq!(
        trig(5),
        vec![Control::StickZone(
            Stick::Right,
            Some(Dir::Down),
            Zone::High
        )]
    );
    assert_eq!(trig(6), vec![Control::Button(PadButton::A)]);
    assert!(
        diags
            .0
            .iter()
            .any(|d| d.path == "mappings[6]" && d.message.contains("ordinary press"))
    );

    assert_eq!(
        p.layers[0].not_inherited,
        vec![
            Combo(vec![Control::TriggerPress(Trigger::Right)]),
            Combo(vec![Control::TriggerZone(Trigger::Right, Zone::High)])
        ]
    );
}

#[test]
fn per_layer_device_settings_become_layer_patches() {
    let doc = r#"{ "schemaVersion": 3.0, "appVersion": "9.2.0", "config": { "appName": "x" },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad",
          "default": { "sticks": [ { "name": "left", "rotation": 10, "zones": [ { "name": "low", "x": 6554 } ] } ] },
          "shifts": [
            { "shiftId": 1,
              "sticks": [ { "name": "left", "rotation": 10, "zones": [ { "name": "low", "x": 13107 } ] } ],
              "triggers": [ { "name": "right", "highZoneMax": 29491 } ] },
            { "shiftId": 3, "sticks": [ { "name": "right", "rotation": 5 } ] }
          ] } ] },
      "shifts": [ { "id": 1, "type": "default" } ] }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors(), "{diags:?}");
    assert!(
        diags.iter().all(|d| !d.message.contains("not supported")),
        "{diags:?}"
    );
    let l = &p.layers[0];
    let zones = l.sticks.left.zones.as_ref().expect("low zone differs");
    assert!((zones.low.x - 0.4).abs() < 0.001);
    assert_eq!(l.sticks.left.rotation, None, "same as the base");
    assert!(l.sticks.right.is_empty() && l.triggers.left.is_empty());
    assert!((l.triggers.right.full_pull.unwrap() - 0.9).abs() < 0.001);
    assert!(
        diags
            .iter()
            .any(|d| d.path == "devices.hardware[0].shifts[1]"
                && d.message.contains("does not exist")),
        "{diags:?}"
    );
    assert!(!crate::validate(&p).has_errors());
}

#[test]
fn virtual_stick_ramp_settings() {
    let doc = r#"{ "schemaVersion": 3.0, "appVersion": "9.2.0", "config": { "appName": "x" },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" }, { "id": 11, "name": "keyboard" } ],
        "virtual": { "gamepads": [ { "id": 1, "type": "360",
          "default": { "sticks": [ { "name": "right", "initialDeflection": 16384, "initialSpeed": 4,
                                     "multiplierX": 0, "multiplierY": 3, "initialSpeedY": 2 } ] },
          "shifts": [ { "shiftId": 1, "sticks": [ { "name": "left", "maxDeflection": 16384 } ] },
                      { "shiftId": 4, "sticks": [ { "name": "left", "maxDeflection": 100 } ] } ] } ] } },
      "shifts": [ { "id": 1, "type": "default" } ],
      "masks": [ { "id": 1, "set": [ { "deviceId": 11, "buttonId": 17, "description": "DIK_W" } ] } ],
      "mappings": [ { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } },
                      "macros": [ { "gamepad": { "id": 1, "buttonId": 40 } } ] } ] }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors(), "{diags:?}");
    let near = |a: f32, b: f32| (a - b).abs() < 0.001;
    let vs = &p.virtual_devices.sticks;
    // The left stick is pushed by W, so it ramps with the format's defaults.
    assert!(near(vs.left.initial, 7000.0 / 32768.0) && near(vs.left.speed, 1.0));
    assert!(near(vs.left.acceleration, 0.5) && vs.left.max == 1.0);
    let r = &vs.right;
    assert!(near(r.initial, 0.5) && near(r.speed, 0.5));
    assert_eq!((r.multiplier_x, r.multiplier_y), (1.0, 3.0), "0 means 1");
    let l = &p.layers[0].virtual_devices.sticks;
    assert_eq!(l.left.max, Some(0.5));
    assert!(l.left.initial.is_none() && l.right.is_empty());
    assert!(
        diags.iter().any(|d| d.message.contains("initialSpeedY")),
        "{diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.path == "devices.virtual.gamepads[0].shifts[1]")
    );
    assert!(!crate::validate(&p).has_errors());

    // Without buttons on a stick, nothing is written.
    let plain = r#"{ "schemaVersion": 3.0, "config": {}, "devices": { "hardware": [ { "id": 1, "name": "gamepad" } ] } }"#;
    let (p, _) = import(plain).unwrap();
    assert_eq!(p.virtual_devices.sticks, Default::default());
}

#[test]
fn jitter_and_shake() {
    let doc = r#"{ "schemaVersion": 3.0, "config": { "appName": "x" },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" }, { "id": 11, "name": "keyboard" } ],
        "virtual": { "gamepads": [ { "id": 1, "type": "ds4",
          "default": { "sticks": [
            { "name": "right", "jitter": true, "jitterType": "square", "jitterDeflectionMin": 40,
              "jitterDeflectionMax": 20, "jitterDelayMin": 50, "jitterDelayMax": 60, "jitterOptionalParameter": 3 },
            { "name": "gyro", "shakingDuration": 200, "shakingAmplitude": 5 } ] } } ] } },
      "masks": [ { "id": 1, "set": [ { "deviceId": 11, "buttonId": 16, "description": "DIK_Q" } ] } ],
      "mappings": [ { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } },
                      "macros": [ { "gamepad": { "id": 1, "buttonId": 251 } } ] } ] }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors(), "{diags:?}");
    let j = &p.virtual_devices.sticks.right.jitter;
    assert!(j.enabled && j.pattern == JitterPattern::Square);
    assert_eq!((j.min, j.max), (0.02, 0.04), "sizes in order");
    assert_eq!((j.delay_min_ms, j.delay_max_ms), (50, 60));
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("optional parameter"))
    );
    assert_eq!(p.virtual_devices.motion.shake_ms, 200);
    assert_eq!(p.virtual_devices.motion.shake_g, 5.0);
    assert_eq!(
        p.mappings[0].actions,
        vec![Action::Shake {
            shake: ShakeAxis::MinusY
        }]
    );
    assert!(!crate::validate(&p).has_errors());
}

#[test]
fn absolute_and_pulsed_items() {
    let doc = r#"{ "schemaVersion": 3.0, "config": { "appName": "x" },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" }, { "id": 11, "name": "keyboard" } ] },
      "masks": [ { "id": 1, "set": [ { "deviceId": 11, "buttonId": 17, "description": "DIK_W" } ] } ],
      "mappings": [ { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } },
                      "macros": [ { "gamepad": { "id": 1, "buttonId": 40, "value": 16383, "absolute": true } },
                                  { "gamepad": { "id": 1, "buttonId": 1, "pulsation": true } },
                                  { "keyboard": { "buttonId": 57, "description": "DIK_SPACE", "pulsation": true } } ] } ] }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors(), "{diags:?}");
    let a = &p.mappings[0].actions;
    assert!(
        matches!(a[0], Action::Stick { absolute: true, y, .. } if (y - 0.5).abs() < 0.001),
        "{a:?}"
    );
    assert!(
        matches!(
            a[1],
            Action::Pad {
                action: Press::Tap,
                ..
            }
        ),
        "{a:?}"
    );
    assert!(
        matches!(
            a[2],
            Action::Key {
                action: Press::Tap,
                ..
            }
        ),
        "{a:?}"
    );
}

#[test]
fn flick_items_and_calibration() {
    let doc = r#"{ "schemaVersion": 3.0, "config": { "appName": "x" },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad", "default": { "sticks": [
          { "name": "right", "mouse": { "flickSensitivity": 400, "flickDuration": 150, "flickThreshold": 20000 } } ] } },
        { "id": 11, "name": "keyboard" } ] },
      "masks": [ { "id": 1, "set": [ { "deviceId": 11, "buttonId": 16, "description": "DIK_Q" } ] } ],
      "mappings": [ { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } },
                      "macros": [ { "mouse": { "direction": "left", "flick": true } } ] } ] }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors(), "{diags:?}");
    assert_eq!(p.mappings[0].actions, vec![Action::Turn { turn: -90.0 }]);
    assert_eq!(p.virtual_devices.mouse.counts_per_turn, 14400.0);
    assert_eq!(p.virtual_devices.mouse.turn_ms, 150);
    assert!(diags.iter().any(|d| d.message.contains("flick threshold")));
    let toml = crate::to_toml(&p).unwrap();
    assert_eq!(crate::parse_toml(&toml).unwrap(), p, "{toml}");
}

#[test]
fn gyro_controls_tilt_and_commands() {
    let doc = r#"{ "schemaVersion": 3.0, "config": { "appName": "x" },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad", "default": { "sticks": [ { "name": "gyro", "tilt": true } ] } } ] },
      "commands": [ { "id": 1, "type": 5 }, { "id": 2, "type": 4 } ],
      "masks": [
        { "id": 1, "set": [ { "deviceId": 1, "buttonId": 72 } ] },
        { "id": 2, "set": [ { "deviceId": 1, "buttonId": 75 } ] },
        { "id": 3, "set": [ { "deviceId": 1, "buttonId": 155 } ] },
        { "id": 4, "set": [ { "deviceId": 1, "buttonId": 160 } ] }
      ],
      "mappings": [
        { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } },
          "macros": [ { "command": { "id": 1 } } ] },
        { "condition": { "mask": { "id": 2, "activator": { "type": "single", "mode": "hold_until_release" } } },
          "macros": [ { "command": { "id": 2 } } ] },
        { "condition": { "mask": { "id": 3, "activator": { "type": "single", "mode": "hold_until_release" } } },
          "macros": [ { "keyboard": { "buttonId": 30, "description": "DIK_A" } } ] },
        { "condition": { "mask": { "id": 4, "activator": { "type": "single", "mode": "hold_until_release" } } },
          "macros": [ { "keyboard": { "buttonId": 31, "description": "DIK_S" } } ] }
      ] }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors(), "{diags:?}");
    assert_eq!(p.gyro.mode, GyroMode::Tilt);
    let triggers: Vec<String> = p.mappings.iter().map(|m| m.trigger.to_string()).collect();
    assert_eq!(
        triggers,
        [
            "GyroLeanLeft",
            "GyroMed",
            "GyroLeftHigh",
            "GyroLeanRightMed"
        ]
    );
    assert_eq!(
        p.mappings[0].actions,
        vec![Action::Command {
            command: Command::ResetTilt
        }]
    );
    assert_eq!(
        p.mappings[1].actions,
        vec![Action::Command {
            command: Command::SwitchGyroAxis
        }]
    );
}

#[test]
fn touchpad_gestures_and_split_trackpads() {
    let doc = r#"{ "schemaVersion": 3.0, "config": { "appName": "x" },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad", "default": { "touchpads": [ { "id": 1, "zoneClickRequired": true } ] } } ] },
      "masks": [
        { "id": 1, "set": [ { "deviceId": 1, "buttonId": 79 } ] },
        { "id": 2, "set": [ { "deviceId": 1, "buttonId": 88 } ] },
        { "id": 3, "set": [ { "deviceId": 1, "buttonId": 91 } ] },
        { "id": 4, "set": [ { "deviceId": 1, "buttonId": 108 } ] },
        { "id": 5, "set": [ { "deviceId": 1, "buttonId": 163 } ] }
      ],
      "mappings": [
        { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } }, "macros": [ { "keyboard": { "buttonId": 30, "description": "DIK_A" } } ] },
        { "condition": { "mask": { "id": 2, "activator": { "type": "single", "mode": "hold_until_release" } } }, "macros": [ { "keyboard": { "buttonId": 31, "description": "DIK_S" } } ] },
        { "condition": { "mask": { "id": 3, "activator": { "type": "single", "mode": "hold_until_release" } } }, "macros": [ { "keyboard": { "buttonId": 32, "description": "DIK_D" } } ] },
        { "condition": { "mask": { "id": 4, "activator": { "type": "single", "mode": "hold_until_release" } } }, "macros": [ { "keyboard": { "buttonId": 33, "description": "DIK_F" } } ] },
        { "condition": { "mask": { "id": 5, "activator": { "type": "single", "mode": "hold_until_release" } } }, "macros": [ { "keyboard": { "buttonId": 34, "description": "DIK_G" } } ] }
      ] }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors(), "{diags:?}");
    assert!(p.touchpad.split && p.touchpad.click_required);
    let triggers: Vec<String> = p.mappings.iter().map(|m| m.trigger.to_string()).collect();
    assert_eq!(
        triggers,
        [
            "TouchTapCenter",
            "TouchSwipe2Down",
            "TrackpadLeftUp",
            "TrackpadRightClick",
            "TrackpadLeftTap"
        ]
    );
}

#[test]
fn touch_gesture_outputs() {
    let doc = r#"{ "schemaVersion": 3.0, "config": { "appName": "x" },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" }, { "id": 11, "name": "keyboard" } ],
                   "virtual": { "gamepads": [ { "id": 1, "type": "ds4" } ] } },
      "masks": [ { "id": 1, "set": [ { "deviceId": 11, "buttonId": 16, "description": "DIK_Q" } ] } ],
      "mappings": [ { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } },
                      "macros": [ { "gamepad": { "id": 1, "buttonId": 89, "value": 30 } },
                                  { "gamepad": { "id": 1, "buttonId": 81 } } ] } ] }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors(), "{diags:?}");
    assert_eq!(
        p.mappings[0].actions,
        vec![
            Action::Touch {
                touch: TouchGesture::Swipe2Left,
                ms: Some(300)
            },
            Action::Touch {
                touch: TouchGesture::ZoomIn,
                ms: None
            }
        ]
    );
    let toml = crate::to_toml(&p).unwrap();
    assert!(toml.contains("touch = \"swipe2_left\""), "{toml}");
    assert_eq!(crate::parse_toml(&toml).unwrap(), p);
}

#[test]
fn mouse_extras() {
    let doc = r#"{ "schemaVersion": 3.0, "config": { "appName": "x" },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" },
          { "id": 2, "name": "mouse", "default": { "stick": { "springMode": false, "noiseFilter": 5, "snapZone": 3277 } } } ],
        "virtual": { "mouse": { "config": { "sensitivity": 8, "deflection": 4, "acceleration": 0, "wheelDeflection": 7 } } } },
      "masks": [ { "id": 1, "set": [ { "deviceId": 2, "buttonId": 62 } ] }, { "id": 2, "set": [ { "deviceId": 2, "buttonId": 141 } ] } ],
      "mappings": [
        { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } }, "macros": [ { "mouse": { "direction": "left" } } ] },
        { "condition": { "mask": { "id": 2, "activator": { "type": "single", "mode": "hold_until_release" } } }, "macros": [ { "keyboard": { "buttonId": 30, "description": "DIK_A" } } ] }
      ] }"#;
    let (p, diags) = import(doc).unwrap();
    assert!(!diags.has_errors(), "{diags:?}");
    let r = &p.analog[0];
    assert!(!r.spring && r.noise_filter == 5.0 && (r.snap - 0.1).abs() < 0.001);
    assert_eq!(p.virtual_devices.mouse.wheel_repeat_ms, 200);
    assert_eq!(
        p.mappings[0].actions,
        vec![Action::MouseMove {
            mouse_move: Dir::Left,
            speed: 1000.0,
            acceleration: 0.0
        }]
    );
    let triggers: Vec<String> = p.mappings.iter().map(|m| m.trigger.to_string()).collect();
    assert_eq!(triggers, ["MouseMove:Med", "MouseMove:UpHigh"]);
}

#[test]
fn zone_button_ids() {
    use Control::StickZone;
    assert_eq!(
        gamepad_control(37),
        Some(StickZone(Stick::Left, None, Zone::Low))
    );
    assert_eq!(
        gamepad_control(46),
        Some(StickZone(Stick::Right, None, Zone::High))
    );
    assert_eq!(
        gamepad_control(54),
        Some(Control::TriggerZone(Trigger::Left, Zone::High))
    );
    assert_eq!(
        gamepad_control(56),
        Some(Control::TriggerZone(Trigger::Right, Zone::Low))
    );
    assert_eq!(
        gamepad_control(109),
        Some(StickZone(Stick::Left, Some(Dir::Up), Zone::Low))
    );
    assert_eq!(
        gamepad_control(116),
        Some(StickZone(Stick::Left, Some(Dir::Right), Zone::Med))
    );
    assert_eq!(
        gamepad_control(123),
        Some(StickZone(Stick::Right, Some(Dir::Left), Zone::Low))
    );
    assert_eq!(
        gamepad_control(132),
        Some(StickZone(Stick::Right, Some(Dir::Right), Zone::High))
    );
}

#[test]
fn rejects_other_versions() {
    assert!(import(r#"{"schemaVersion": 2.0}"#).is_err());
    assert!(import("[]").is_err());
}

#[test]
fn description_lookup() {
    assert_eq!(key_from_description("DIK_UPARROW"), Some(Key::Up));
    assert_eq!(key_from_description("dik_numpad7"), Some(Key::Numpad7));
    assert_eq!(key_from_description("DIK_F9"), Some(Key::F9));
    assert_eq!(key_from_description("DIK_Q"), Some(Key::Q));
    assert_eq!(key_from_description("DIK_RETURN"), Some(Key::Enter));
}
