//! Importer for JSON profiles (`schemaVersion` 3.0).
//!
//! The importer is lenient: anything it cannot represent becomes a
//! [`Diagnostic`](crate::Diagnostic) and is skipped, instead of failing the whole file.

use std::collections::HashMap;

use dp_core::control::{Control, Dir, TouchRegion, Zone};
use dp_core::keys::{Key, MouseButton, WheelDir};
use dp_core::pad::{PadButton, Stick, Trigger};
use dp_core::profile::{
    Action, Activator, AnalogMapping, AnalogSource, AnalogTarget, Combo, Command, Jump, JumpMode,
    KeyRepeat, Layer, Mapping, Mode, Press, Profile, VirtualPadType, ZoneRadius,
};
use serde_json::Value;

use crate::diag::Diagnostics;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not a supported JSON profile: {0}")]
    Format(String),
}

/// Raw axis range used throughout the format.
const AXIS_MAX: f32 = 32768.0;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum DeviceKind {
    Gamepad,
    Keyboard,
    Mouse,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum LayerKind {
    Common,
    Toggle,
    Unlock,
    RadialMenu,
}

/// `null` counts as absent everywhere.
fn get<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key).filter(|x| !x.is_null())
}

fn int(v: &Value, key: &str) -> Option<i64> {
    let x = get(v, key)?;
    x.as_i64()
        .or_else(|| x.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64))
}

/// Enum strings are case-insensitive on read.
fn enum_str(v: &Value, key: &str) -> Option<String> {
    get(v, key)
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
}

fn text(v: &Value, key: &str) -> Option<String> {
    get(v, key).and_then(Value::as_str).map(str::to_owned)
}

fn boolean(v: &Value, key: &str) -> bool {
    get(v, key).and_then(Value::as_bool).unwrap_or(false)
}

fn array<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    get(v, key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Zone order of the format's zone button ids.
const ZONES: [Zone; 3] = [Zone::Low, Zone::Med, Zone::High];

/// Gamepad `buttonId` as an input control.
fn gamepad_control(id: i64) -> Option<Control> {
    use Control::*;
    use PadButton as P;
    let zone = |first: i64| ZONES[(id - first) as usize];
    Some(match id {
        37..=39 => StickZone(Stick::Left, None, zone(37)),
        44..=46 => StickZone(Stick::Right, None, zone(44)),
        52..=54 => TriggerZone(Trigger::Left, zone(52)),
        56..=58 => TriggerZone(Trigger::Right, zone(56)),
        // Zone × direction: low up/down/left/right, then med, then high; left stick first.
        109..=132 => {
            let i = (id - 109) as usize;
            let stick = if i < 12 { Stick::Left } else { Stick::Right };
            StickZone(stick, Some(Dir::ALL[i % 4]), ZONES[i % 12 / 4])
        }
        1 => Button(P::A),
        2 => Button(P::B),
        3 => Button(P::X),
        4 => Button(P::Y),
        5 => Button(P::LB),
        6 => Button(P::RB),
        7 => Button(P::Back),
        8 => Button(P::Start),
        9 => Button(P::LS),
        10 => Button(P::RS),
        11 => Button(P::Guide),
        12 | 13 => Button(P::Misc),
        // DualSense Edge Fn/back buttons and Elite-style paddles share our paddle slots.
        14 | 29 => Button(P::PaddleUL),
        15 | 30 => Button(P::PaddleUR),
        16 | 31 => Button(P::PaddleLL),
        17 | 32 => Button(P::PaddleLR),
        33 => Button(P::DpadUp),
        34 => Button(P::DpadDown),
        35 => Button(P::DpadLeft),
        36 => Button(P::DpadRight),
        40 => StickDir(Stick::Left, Dir::Up),
        41 => StickDir(Stick::Left, Dir::Down),
        42 => StickDir(Stick::Left, Dir::Left),
        43 => StickDir(Stick::Left, Dir::Right),
        47 => StickDir(Stick::Right, Dir::Up),
        48 => StickDir(Stick::Right, Dir::Down),
        49 => StickDir(Stick::Right, Dir::Left),
        50 => StickDir(Stick::Right, Dir::Right),
        51 => TriggerPress(Trigger::Left),
        55 => TriggerPress(Trigger::Right),
        64 => MouseMove(Dir::Up),
        65 => MouseMove(Dir::Down),
        66 => MouseMove(Dir::Left),
        67 => MouseMove(Dir::Right),
        68 | 244 => Gyro(Dir::Up),
        69 | 245 => Gyro(Dir::Down),
        70 | 246 => Gyro(Dir::Left),
        71 | 247 => Gyro(Dir::Right),
        79 => Touch(TouchRegion::Center),
        91 => Touch(TouchRegion::Up),
        92 => Touch(TouchRegion::Down),
        93 => Touch(TouchRegion::Left),
        94 => Touch(TouchRegion::Right),
        99 => Button(P::Touchpad),
        169 => TriggerFull(Trigger::Left),
        170 => TriggerFull(Trigger::Right),
        171 => Wheel(WheelDir::Up),
        172 => Wheel(WheelDir::Down),
        173 => Wheel(WheelDir::Left),
        174 => Wheel(WheelDir::Right),
        _ => return None,
    })
}

/// Key-table name → key, for when `buttonId` is missing or out of range.
fn key_from_description(desc: &str) -> Option<Key> {
    let d = desc.trim().to_ascii_uppercase();
    let alias = match d.as_str() {
        "DIK_BACKSPACE" => "DIK_BACK",
        "DIK_NUMPADSTAR" => "DIK_MULTIPLY",
        "DIK_LALT" => "DIK_LMENU",
        "DIK_RALT" => "DIK_RMENU",
        "DIK_CAPSLOCK" => "DIK_CAPITAL",
        "DIK_NUMPADMINUS" => "DIK_SUBTRACT",
        "DIK_NUMPADPLUS" => "DIK_ADD",
        "DIK_NUMPADPERIOD" => "DIK_DECIMAL",
        "DIK_NUMPADSLASH" => "DIK_DIVIDE",
        "DIK_UPARROW" => "DIK_UP",
        "DIK_DOWNARROW" => "DIK_DOWN",
        "DIK_LEFTARROW" => "DIK_LEFT",
        "DIK_RIGHTARROW" => "DIK_RIGHT",
        "DIK_PGUP" => "DIK_PRIOR",
        "DIK_PGDN" => "DIK_NEXT",
        other => other,
    };
    let dik = match alias {
        "DIK_ESCAPE" => 0x01,
        "DIK_MINUS" => 0x0C,
        "DIK_EQUALS" => 0x0D,
        "DIK_BACK" => 0x0E,
        "DIK_TAB" => 0x0F,
        "DIK_LBRACKET" => 0x1A,
        "DIK_RBRACKET" => 0x1B,
        "DIK_RETURN" => 0x1C,
        "DIK_LCONTROL" => 0x1D,
        "DIK_SEMICOLON" => 0x27,
        "DIK_APOSTROPHE" => 0x28,
        "DIK_GRAVE" => 0x29,
        "DIK_LSHIFT" => 0x2A,
        "DIK_BACKSLASH" => 0x2B,
        "DIK_COMMA" => 0x33,
        "DIK_PERIOD" => 0x34,
        "DIK_SLASH" => 0x35,
        "DIK_RSHIFT" => 0x36,
        "DIK_MULTIPLY" => 0x37,
        "DIK_LMENU" => 0x38,
        "DIK_SPACE" => 0x39,
        "DIK_CAPITAL" => 0x3A,
        "DIK_NUMLOCK" => 0x45,
        "DIK_SCROLL" => 0x46,
        "DIK_SUBTRACT" => 0x4A,
        "DIK_ADD" => 0x4E,
        "DIK_DECIMAL" => 0x53,
        "DIK_OEM_102" => 0x56,
        "DIK_NUMPADENTER" => 0x9C,
        "DIK_RCONTROL" => 0x9D,
        "DIK_DIVIDE" => 0xB5,
        "DIK_SYSRQ" => 0xB7,
        "DIK_RMENU" => 0xB8,
        "DIK_PAUSE" => 0xC5,
        "DIK_HOME" => 0xC7,
        "DIK_UP" => 0xC8,
        "DIK_PRIOR" => 0xC9,
        "DIK_LEFT" => 0xCB,
        "DIK_RIGHT" => 0xCD,
        "DIK_END" => 0xCF,
        "DIK_DOWN" => 0xD0,
        "DIK_NEXT" => 0xD1,
        "DIK_INSERT" => 0xD2,
        "DIK_DELETE" => 0xD3,
        "DIK_LWIN" => 0xDB,
        "DIK_RWIN" => 0xDC,
        "DIK_APPS" => 0xDD,
        "DIK_MUTE" => 0xA0,
        "DIK_PLAYPAUSE" => 0xA2,
        "DIK_MEDIASTOP" => 0xA4,
        "DIK_VOLUMEDOWN" => 0xAE,
        "DIK_VOLUMEUP" => 0xB0,
        "DIK_PREVTRACK" => 0x90,
        "DIK_NEXTTRACK" => 0x99,
        _ => {
            let rest = alias.strip_prefix("DIK_")?;
            return match rest {
                n if n.len() == 1 => Key::from_name(n),
                n => n
                    .strip_prefix("NUMPAD")
                    .and_then(|d| Key::from_name(&format!("Numpad{d}")))
                    .or_else(|| Key::from_name(n)),
            };
        }
    };
    Key::from_dik(dik)
}

/// Keyboard key by table index (= DIK code), cross-checked with `description`.
fn keyboard_key(v: &Value, diags: &mut Diagnostics, path: &str) -> Option<Key> {
    let by_id = int(v, "buttonId")
        .and_then(|id| u16::try_from(id).ok())
        .and_then(Key::from_dik);
    let by_desc = text(v, "description")
        .as_deref()
        .and_then(key_from_description);
    match (by_id, by_desc) {
        (Some(a), Some(b)) if a != b => {
            diags.warn(
                path,
                format!("buttonId says {a}, description says {b}; using the description"),
            );
            Some(b)
        }
        (a, b) => {
            let k = b.or(a);
            if k.is_none() {
                diags.warn(path, format!("unknown keyboard key {}", v));
            }
            k
        }
    }
}

fn mouse_button(id: i64) -> Option<MouseButton> {
    match id {
        1 => Some(MouseButton::Left),
        2 => Some(MouseButton::Right),
        3 => Some(MouseButton::Middle),
        4 => Some(MouseButton::X1),
        5 => Some(MouseButton::X2),
        _ => None,
    }
}

/// Highest low-zone threshold imported, so the medium and high zones keep some room.
const MAX_LOW_ZONE: f32 = 0.95;

/// One `zones[]` entry as a fraction of full deflection; a missing `y` equals `x`.
fn zone_entry(zones: &[Value], name: &str) -> Option<ZoneRadius> {
    let z = zones
        .iter()
        .find(|z| enum_str(z, "name").as_deref() == Some(name))?;
    let x = int(z, "x")?;
    let y = int(z, "y").unwrap_or(x);
    let frac = |v: i64| (v as f32 / AXIS_MAX).clamp(0.001, 1.0);
    Some(ZoneRadius {
        x: frac(x),
        y: frac(y),
    })
}

/// The zone control of a stick direction or trigger press, for zone activators.
fn in_zone(c: Control, z: Zone) -> Option<Control> {
    match c {
        Control::StickDir(s, d) => Some(Control::StickZone(s, Some(d), z)),
        Control::TriggerPress(t) => Some(Control::TriggerZone(t, z)),
        _ => None,
    }
}

fn dir_of(s: &str) -> Option<Dir> {
    match s {
        "up" => Some(Dir::Up),
        "down" => Some(Dir::Down),
        "left" => Some(Dir::Left),
        "right" => Some(Dir::Right),
        _ => None,
    }
}

fn wheel_of(s: &str) -> Option<WheelDir> {
    match s {
        "up" => Some(WheelDir::Up),
        "down" => Some(WheelDir::Down),
        "left" => Some(WheelDir::Left),
        "right" => Some(WheelDir::Right),
        _ => None,
    }
}

fn press_of(v: &Value) -> Press {
    match enum_str(v, "action").as_deref() {
        Some("down" | "single_down") => Press::Down,
        Some("up") => Press::Up,
        _ => Press::Auto,
    }
}

struct Importer<'a> {
    root: &'a Value,
    diags: Diagnostics,
    /// Time values are in 10 ms units unless `minTimeUnit == 1`.
    time_scale: u32,
    devices: HashMap<i64, DeviceKind>,
    masks: HashMap<i64, Combo>,
    layer_kinds: HashMap<i64, LayerKind>,
    commands: HashMap<i64, &'a Value>,
    /// Masks that zone activators turned into zone controls: (mask, zoned trigger).
    zoned: Vec<(Combo, Combo)>,
}

/// Recognize the format's encoding of shift keys and fold each pair into one
/// mapping: base `[T, a] -> N` plus layer-N `[T, release] -> 0` is a hold shift, and
/// base `[T, a] -> N` plus layer-N `[T, a] -> 0` is a toggle shift.
fn fold_jump_pairs(mappings: &mut Vec<Mapping>) {
    let mut remove = Vec::new();
    for i in 0..mappings.len() {
        let m = &mappings[i];
        let Some(j) = &m.jump else { continue };
        if m.layer != 0 || j.layer == 0 || j.mode != JumpMode::Switch || remove.contains(&i) {
            continue;
        }
        let partner = mappings.iter().enumerate().position(|(k, o)| {
            k != i
                && !remove.contains(&k)
                && o.layer == j.layer
                && o.trigger.normalized() == m.trigger.normalized()
                && o.actions.is_empty()
                && o.jump
                    .as_ref()
                    .is_some_and(|oj| oj.layer == 0 && oj.mode == JumpMode::Switch)
                && (o.activator == Activator::Release || o.activator == m.activator)
        });
        if let Some(k) = partner {
            let mode = if mappings[k].activator == Activator::Release {
                JumpMode::Hold
            } else {
                JumpMode::Toggle
            };
            if let Some(j) = mappings[i].jump.as_mut() {
                j.mode = mode;
            }
            remove.push(k);
        }
    }
    remove.sort_unstable();
    for k in remove.into_iter().rev() {
        mappings.remove(k);
    }
}

/// Import a JSON profile document.
pub fn import(json: &str) -> Result<(Profile, Diagnostics), ImportError> {
    let json = json.trim_start_matches('\u{feff}').trim_start();
    if !json.starts_with('{') {
        return Err(ImportError::Format("expected a JSON object".into()));
    }
    let root: Value = serde_json::from_str(json)?;
    match root.get("schemaVersion").and_then(Value::as_f64) {
        Some(3.0) => {}
        Some(v) => {
            return Err(ImportError::Format(format!(
                "unsupported schemaVersion {v} (only 3.0 is supported)"
            )));
        }
        None => return Err(ImportError::Format("missing schemaVersion".into())),
    }
    let mut imp = Importer {
        root: &root,
        diags: Diagnostics::default(),
        time_scale: 10,
        devices: HashMap::new(),
        masks: HashMap::new(),
        layer_kinds: HashMap::new(),
        commands: HashMap::new(),
        zoned: Vec::new(),
    };
    let profile = imp.run();
    Ok((profile, imp.diags))
}

impl<'a> Importer<'a> {
    fn run(&mut self) -> Profile {
        let mut p = Profile::default();
        // The format never mutes a mapped key implicitly; explicit "unmap" entries do.
        p.output.mute_mapped_inputs = false;
        let config = get(self.root, "config").cloned().unwrap_or(Value::Null);
        if int(&config, "minTimeUnit") == Some(1) {
            self.time_scale = 1;
        }
        p.name = text(&config, "appName")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                text(&config, "comment")
                    .and_then(|c| {
                        c.strip_prefix("Custom configuration to play ")
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| "Game".into())
            });
        p.description = text(&config, "description").unwrap_or_default();
        p.processes = array(&config, "processNames")
            .iter()
            .filter_map(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(Into::into)
            .collect();
        if get(&config, "led").is_some() {
            self.diags.info(
                "config.led",
                "light-bar settings are not supported yet and were ignored",
            );
        }

        self.devices(&mut p);
        for c in array(self.root, "commands") {
            if let Some(id) = int(c, "id") {
                self.commands.insert(id, c);
            }
        }
        self.masks();
        self.layers(&mut p);
        let mappings = array(self.root, "mappings");
        for (i, m) in mappings.iter().enumerate() {
            let path = format!("mappings[{i}]");
            if let Some(mapping) = self.mapping(m, &path) {
                p.mappings.push(mapping);
            }
        }
        fold_jump_pairs(&mut p.mappings);
        // A mask a layer does not inherit stays uninherited in its zone form too.
        for l in &mut p.layers {
            for (mask, zoned) in &self.zoned {
                if l.not_inherited.contains(mask) && !l.not_inherited.contains(zoned) {
                    l.not_inherited.push(zoned.clone());
                }
            }
        }
        for key in ["radialMenuCircles", "radialMenuSectors", "crosshairs"] {
            if !array(self.root, key).is_empty() {
                self.diags.warn(
                    key,
                    "radial menus and crosshair overlays are not supported yet and were ignored",
                );
            }
        }
        p
    }

    fn devices(&mut self, p: &mut Profile) {
        let Some(devices) = get(self.root, "devices") else {
            self.diags.error("devices", "missing");
            return;
        };
        for (i, hw) in array(devices, "hardware").iter().enumerate() {
            let path = format!("devices.hardware[{i}]");
            let Some(id) = int(hw, "id") else {
                self.diags.warn(&path, "device without id");
                continue;
            };
            let kind = match enum_str(hw, "name").as_deref() {
                Some("gamepad") => DeviceKind::Gamepad,
                Some("keyboard") => DeviceKind::Keyboard,
                Some("mouse") => DeviceKind::Mouse,
                other => {
                    self.diags
                        .warn(&path, format!("unknown device kind {other:?}"));
                    continue;
                }
            };
            self.devices.insert(id, kind);
            if kind == DeviceKind::Gamepad {
                if get(hw, "hidePhysicalGamepad").is_some() {
                    p.output.hide_physical = boolean(hw, "hidePhysicalGamepad");
                }
                if let Some(d) = get(hw, "default") {
                    self.analog_settings(p, d, &format!("{path}.default"));
                }
                if !array(hw, "shifts").is_empty() {
                    self.diags.info(format!("{path}.shifts"), "per-layer stick/trigger settings are not supported; base settings apply to all layers");
                }
            } else if kind == DeviceKind::Mouse
                && let Some(stick) = get(hw, "default").and_then(|d| get(d, "stick"))
            {
                // A mouse with stick settings is used as a stick (mouse → stick).
                let mut route = AnalogMapping {
                    source: AnalogSource::Mouse,
                    target: AnalogTarget::RightStick,
                    ..Default::default()
                };
                if let Some(t) = int(stick, "returnTime") {
                    route.decay_ms = t.clamp(0, 255) as u32;
                }
                if let Some(s) = int(stick, "smoothing") {
                    route.smoothing_ms = (s.clamp(0, 100) * 4) as u32;
                }
                self.diags.info(
                    format!("{path}.default.stick"),
                    "mouse-as-stick imported as a mouse → right stick route; tune its sensitivity",
                );
                p.analog.push(route);
            }
        }

        let virt = get(devices, "virtual");
        let pads = virt.map(|v| array(v, "gamepads")).unwrap_or(&[]);
        p.output.virtual_pad = match pads.first().and_then(|g| enum_str(g, "type")).as_deref() {
            None => VirtualPadType::Xbox360,
            Some("360") => VirtualPadType::Xbox360,
            Some("ds4") => VirtualPadType::DualShock4,
            Some("keyboardandmouse") => VirtualPadType::None,
            Some(other) => {
                let fallback = if other == "ds3" {
                    VirtualPadType::DualShock4
                } else {
                    VirtualPadType::Xbox360
                };
                self.diags.warn(
                    "devices.virtual.gamepads[0].type",
                    format!("virtual `{other}` controllers need a custom driver; using {fallback:?} instead"),
                );
                fallback
            }
        };
        if pads.len() > 1 {
            self.diags.info(
                "devices.virtual.gamepads",
                "only the first virtual gamepad is used; gamepad outputs in this format do not say which one they mean. To drive a second virtual controller, give that physical controller slots of its own",
            );
        }
        // Only the four combinations the editor offers mean something; others stay native.
        if let Some(cfg) = virt
            .and_then(|v| get(v, "keyboard"))
            .and_then(|k| get(k, "config"))
        {
            p.virtual_devices.keyboard.repeat = match (
                enum_str(cfg, "repeatMode").as_deref(),
                int(cfg, "repeatValue"),
            ) {
                (Some("fixed"), Some(10)) => KeyRepeat::Fast,
                (Some("fixed"), Some(50)) => KeyRepeat::Slow,
                (Some("fixed"), Some(0)) => KeyRepeat::Off,
                _ => KeyRepeat::Native,
            };
        }
    }

    /// Gamepad stick/trigger/touchpad settings.
    fn analog_settings(&mut self, p: &mut Profile, d: &Value, path: &str) {
        for s in array(d, "sticks") {
            let cfg = match enum_str(s, "name").as_deref() {
                Some("left") => &mut p.sticks.left,
                Some("right") => &mut p.sticks.right,
                Some("gyro") => {
                    self.diags.info(
                        format!("{path}.sticks"),
                        "gyro tuning is not imported; add a gyro route manually",
                    );
                    continue;
                }
                _ => continue,
            };
            // The low zone is the deadzone of digital mappings; an output deadzone, if
            // any, is part of the curve.
            let zones = array(s, "zones");
            if let Some(low) = zone_entry(zones, "low") {
                cfg.zones.low = ZoneRadius {
                    x: low.x.min(MAX_LOW_ZONE),
                    y: low.y.min(MAX_LOW_ZONE),
                };
            }
            cfg.zones.med = zone_entry(zones, "med");
            cfg.zones.high = zone_entry(zones, "high");
            let [low, med, high] = cfg.zone_thresholds();
            if crate::zone_problem([low.x, med.x, high.x])
                .or(crate::zone_problem([low.y, med.y, high.y]))
                .is_some()
            {
                self.diags.warn(
                    format!("{path}.sticks"),
                    "stick zones are out of order; the medium and high zones use their defaults",
                );
                cfg.zones.med = None;
                cfg.zones.high = None;
            }
            if boolean(s, "diagonalZones") {
                self.diags.info(
                    format!("{path}.sticks"),
                    "separate diagonal directions are not supported; a diagonal presses both of its directions",
                );
            }
            if let Some(r) = int(s, "rotation") {
                cfg.rotation = r.clamp(-90, 90) as f32;
            }
            let points = get(s, "curves")
                .and_then(|c| get(c, "points"))
                .map(|p| array(p, "horizontal"))
                .unwrap_or(&[]);
            if !points.is_empty() {
                let mut curve: Vec<[f32; 2]> = points
                    .iter()
                    .filter_map(|pt| {
                        Some([
                            int(pt, "travel")? as f32 / AXIS_MAX,
                            int(pt, "value")? as f32 / AXIS_MAX,
                        ])
                    })
                    .collect();
                curve.sort_by(|a, b| a[0].total_cmp(&b[0]));
                cfg.curve = curve;
            }
        }
        for t in array(d, "triggers") {
            let cfg = match enum_str(t, "name").as_deref() {
                Some("left") => &mut p.triggers.left,
                Some("right") => &mut p.triggers.right,
                _ => continue,
            };
            let zones = array(t, "zones");
            if let Some(low) = zone_entry(zones, "low") {
                cfg.zones.low = low.x.min(MAX_LOW_ZONE);
            }
            cfg.zones.med = zone_entry(zones, "med").map(|r| r.x);
            cfg.zones.high = zone_entry(zones, "high").map(|r| r.x);
            // Zones shape the trigger's output only when they apply at hardware level.
            if boolean(t, "affectHardware") {
                cfg.deadzone = cfg.zones.low;
            }
            if let Some(max) = int(t, "highZoneMax") {
                cfg.full_pull = (max as f32 / AXIS_MAX).clamp(cfg.zones.low + 0.01, 1.0);
            }
            if crate::zone_problem(cfg.zone_thresholds()).is_some() {
                self.diags.warn(
                    format!("{path}.triggers"),
                    "trigger zones are out of order; the medium and high zones use their defaults",
                );
                cfg.zones.med = None;
                cfg.zones.high = None;
            }
            if get(t, "adaptiveMode").is_some() || get(t, "adaptiveDescription").is_some() {
                self.diags.info(
                    format!("{path}.triggers"),
                    "adaptive trigger effects are not supported",
                );
            }
        }
        if !array(d, "touchpads").is_empty() {
            self.diags.info(
                format!("{path}.touchpads"),
                "touchpad tuning is imported as region buttons only",
            );
        }
    }

    /// Resolve one `{deviceId, buttonId, description}` element to a control.
    fn element(&mut self, e: &Value, path: &str) -> Option<Control> {
        let device = int(e, "deviceId").and_then(|d| self.devices.get(&d).copied());
        let id = int(e, "buttonId");
        let control =
            match (device, id) {
                (None, _) => {
                    self.diags.warn(path, "references an unknown device");
                    return None;
                }
                (_, None) => {
                    self.diags.warn(path, "missing buttonId");
                    return None;
                }
                (Some(DeviceKind::Gamepad), Some(id)) => gamepad_control(id),
                (Some(DeviceKind::Keyboard), Some(_)) => {
                    return keyboard_key(e, &mut self.diags, path).map(Control::Key);
                }
                (Some(DeviceKind::Mouse), Some(id)) => mouse_button(id)
                    .map(Control::Mouse)
                    .or_else(|| match gamepad_control(id) {
                        c @ Some(Control::MouseMove(_) | Control::Wheel(_)) => c,
                        _ => None,
                    }),
            };
        if control.is_none() {
            let desc = text(e, "description").unwrap_or_default();
            self.diags.warn(
                path,
                format!("input {} ({desc}) is not supported", id.unwrap_or_default()),
            );
        }
        control
    }

    fn masks(&mut self) {
        for (i, m) in array(self.root, "masks").iter().enumerate() {
            let path = format!("masks[{i}]");
            let Some(id) = int(m, "id") else { continue };
            let set = array(m, "set");
            if set.is_empty() {
                if get(m, "radialMenuSet").is_some() || get(m, "crosshairSet").is_some() {
                    self.diags
                        .warn(&path, "radial-menu / crosshair masks are not supported");
                }
                continue;
            }
            let mut combo = Vec::new();
            for (j, e) in set.iter().enumerate() {
                match self.element(e, &format!("{path}.set[{j}]")) {
                    Some(c) => combo.push(c),
                    None => {
                        combo.clear();
                        break;
                    }
                }
            }
            if !combo.is_empty() {
                self.masks.insert(id, Combo(combo));
            }
        }
    }

    fn layers(&mut self, p: &mut Profile) {
        for (i, s) in array(self.root, "shifts").iter().enumerate() {
            let path = format!("shifts[{i}]");
            let Some(id) = int(s, "id").filter(|id| (1..=255).contains(id)) else {
                self.diags.warn(&path, "invalid layer id");
                continue;
            };
            let kind = match enum_str(s, "type").as_deref() {
                Some("toggle") => LayerKind::Toggle,
                Some("unlock" | "unlockkeyboardonly" | "unlockmouseonly") => LayerKind::Unlock,
                Some("radialmenu") => LayerKind::RadialMenu,
                _ => LayerKind::Common,
            };
            if kind == LayerKind::RadialMenu {
                self.diags
                    .warn(&path, "radial-menu layers are not supported");
            }
            self.layer_kinds.insert(id, kind);
            let not_inherited = get(s, "unheritableMasks")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(Value::as_i64)
                        .filter_map(|m| self.masks.get(&m).cloned())
                        .collect()
                })
                .unwrap_or_default();
            p.layers.push(Layer {
                id: id as u8,
                name: text(s, "description").unwrap_or_else(|| format!("Layer {id}")),
                inherit: true,
                not_inherited,
            });

            // Legacy single-button modifier → synthesized jump mapping.
            let set = array(s, "set");
            if set.len() == 1
                && let Some(c) = self.element(&set[0], &format!("{path}.set[0]"))
            {
                let mut m = Mapping::new(c, Vec::new());
                m.jump = Some(Jump {
                    layer: id as u8,
                    mode: if kind == LayerKind::Toggle {
                        JumpMode::Toggle
                    } else {
                        JumpMode::Hold
                    },
                    delay_ms: None,
                });
                m.description = format!("layer {id} modifier");
                p.mappings.push(m);
            } else if set.len() > 1 {
                self.diags.warn(
                    format!("{path}.set"),
                    "multi-button layer modifiers are not supported",
                );
            }
        }
    }

    fn time(&self, v: i64) -> u32 {
        (v.max(0) as u32).saturating_mul(self.time_scale)
    }

    fn mapping(&mut self, m: &Value, path: &str) -> Option<Mapping> {
        let cond = get(m, "condition")?;
        let layer = int(cond, "shiftId").unwrap_or(0).clamp(0, 255) as u8;

        if let Some(hw) = get(cond, "hardware") {
            return self.hardware_mapping(hw, get(m, "hardware"), layer, path);
        }
        let mask = get(cond, "mask")?;
        let mask_id = int(mask, "id")?;
        let Some(combo) = self.masks.get(&mask_id).cloned() else {
            // The mask itself was already reported when it failed to resolve.
            return None;
        };
        let act = get(mask, "activator").cloned().unwrap_or(Value::Null);
        let params = get(&act, "params").cloned().unwrap_or(Value::Null);

        let mut out = Mapping::new(combo.clone(), Vec::new());
        out.layer = layer;
        out.description = text(m, "description").unwrap_or_default();
        out.activator = match enum_str(&act, "type").as_deref() {
            None | Some("single") => Activator::Single,
            Some("double") => Activator::Double,
            Some("triple") => Activator::Triple,
            Some("long") => Activator::Long,
            Some("start") => Activator::Start,
            Some("release") => Activator::Release,
            Some(z @ ("zonelow" | "zonemed" | "zonehigh")) => {
                // The zone applies to the mask's stick directions and triggers, which
                // become their zone controls (`RT` → `RTHigh`).
                let zone = match z {
                    "zonelow" => Zone::Low,
                    "zonemed" => Zone::Med,
                    _ => Zone::High,
                };
                let mut zoned = false;
                for c in &mut out.trigger.0 {
                    if let Some(zc) = in_zone(*c, zone) {
                        *c = zc;
                        zoned = true;
                    }
                }
                if zoned {
                    self.zoned.push((combo.clone(), out.trigger.clone()));
                } else {
                    self.diags.warn(
                        path,
                        format!(
                            "zone activator `{z}` on {} is not supported; imported as an ordinary press",
                            out.trigger
                        ),
                    );
                }
                Activator::Single
            }
            Some(other) => {
                self.diags
                    .warn(path, format!("unknown activator `{other}`"));
                return None;
            }
        };
        out.mode = match enum_str(&act, "mode").as_deref() {
            None | Some("hold_until_release") => Mode::Hold,
            Some("turbo") => Mode::Turbo,
            Some("toggle") => Mode::Toggle,
            Some("onetime") => Mode::Once,
            Some("repeat") => Mode::Repeat,
            Some(other) => {
                self.diags
                    .warn(path, format!("unknown activation mode `{other}`"));
                Mode::Hold
            }
        };
        if let Some(t) = int(&params, "pause") {
            match out.mode {
                Mode::Turbo => out.turbo_ms = Some(self.time(t.min(60_000)).max(10)),
                Mode::Repeat => out.repeat_pause_ms = Some(self.time(t.min(2550))),
                _ => {}
            }
        }
        if out.mode == Mode::Repeat {
            out.repeat = Some(int(&params, "times").unwrap_or(1).clamp(1, 9999) as u32);
        }
        // Key delay is in time units like pauses.
        if let Some(d) = int(&params, "delay")
            .map(|d| self.time(d))
            .filter(|d| *d <= 500)
        {
            out.key_delay_ms = Some(d);
        }
        // Wait times are plain milliseconds. `singlewaittime` is the double-press window
        // and `doublewaittime` the triple-press window.
        if let Some(t) = int(&params, "longwaittime").filter(|t| (1..=2500).contains(t)) {
            out.long_press_ms = Some(t as u32);
        }
        let double = int(&params, "singlewaittime").filter(|t| (1..=900).contains(t));
        let triple = int(&params, "doublewaittime").filter(|t| (1..=1000).contains(t));
        if let Some(d) = double {
            out.double_tap_ms = Some(d as u32);
        }
        if let Some(t) = triple.filter(|t| double.is_none_or(|d| *t > d)) {
            out.triple_tap_ms = Some(t as u32);
        }

        let mut break_seen = false;
        for (j, item) in array(m, "macros").iter().enumerate() {
            let ipath = format!("{path}.macros[{j}]");
            if get(item, "break").is_some() {
                break_seen = true;
                if !out.actions.last().is_some_and(Action::is_break) {
                    out.actions.push(Action::BREAK);
                }
                continue;
            }
            if let Some(a) = self.macro_item(item, &ipath) {
                // Consecutive pauses merge.
                if let (Action::Pause { pause: new }, Some(Action::Pause { pause })) =
                    (&a, out.actions.last_mut())
                {
                    *pause += new;
                    continue;
                }
                out.actions.push(a);
            }
        }
        if break_seen {
            // Segments play one per press: no turbo, toggle or repetition.
            out.mode = match out.mode {
                Mode::Turbo | Mode::Toggle => Mode::Hold,
                Mode::Repeat => Mode::Once,
                other => other,
            };
            out.turbo_ms = None;
            out.repeat = None;
            out.repeat_pause_ms = None;
        }
        // A one-shot "press K, maybe wait, release K" is a plain remap of K,
        // and so is a single item without an explicit action.
        if out.mode == Mode::Once && !boolean(&params, "macro") {
            let plain = match out.actions.as_slice() {
                [
                    Action::Key {
                        key: a,
                        action: Press::Down,
                    },
                    rest @ ..,
                ] => match rest {
                    [
                        Action::Key {
                            key: b,
                            action: Press::Up,
                        },
                    ]
                    | [
                        Action::Pause { .. },
                        Action::Key {
                            key: b,
                            action: Press::Up,
                        },
                    ] if a == b => Some(Action::Key {
                        key: *a,
                        action: Press::Auto,
                    }),
                    _ => None,
                },
                [single]
                    if matches!(
                        single,
                        Action::Key {
                            action: Press::Auto,
                            ..
                        } | Action::Pad {
                            action: Press::Auto,
                            ..
                        } | Action::Mouse {
                            action: Press::Auto,
                            ..
                        }
                    ) =>
                {
                    Some(single.clone())
                }
                _ => None,
            };
            if let Some(a) = plain {
                out.mode = Mode::Hold;
                out.actions = vec![a];
            }
        }

        if let Some(j) = get(m, "jumpToLayer") {
            // Jumps in this format are absolute "go to layer N"; hold/toggle shifts are pairs of
            // jumps, which `fold_jump_pairs` turns back into our hold/toggle form.
            let target = int(j, "layer").unwrap_or(0).clamp(0, 255);
            out.jump = Some(Jump {
                layer: target as u8,
                mode: JumpMode::Switch,
                delay_ms: int(j, "delay")
                    .filter(|d| *d > 0)
                    .map(|d| d.min(5000) as u32),
            });
            if boolean(j, "postpone") {
                self.diags.info(
                    path,
                    "`postpone` on layer jumps is not supported; outputs start immediately",
                );
            }
            if out.mode == Mode::Toggle && out.actions.is_empty() {
                out.mode = Mode::Hold;
            }
        }
        if out.actions.is_empty() && out.jump.is_none() {
            // A mapping without outputs still consumes its trigger ("unmap").
            out.description = if out.description.is_empty() {
                "unmapped".into()
            } else {
                out.description
            };
        }
        Some(out)
    }

    fn macro_item(&mut self, item: &Value, path: &str) -> Option<Action> {
        if let Some(k) = get(item, "keyboard") {
            let key = keyboard_key(k, &mut self.diags, path)?;
            return Some(Action::Key {
                key,
                action: press_of(k),
            });
        }
        if let Some(ms) = get(item, "mouse") {
            if let Some(b) = int(ms, "buttonId") {
                let Some(mouse) = mouse_button(b) else {
                    self.diags.warn(path, format!("unknown mouse button {b}"));
                    return None;
                };
                return Some(Action::Mouse {
                    mouse,
                    action: press_of(ms),
                });
            }
            if let Some(d) = enum_str(ms, "direction").as_deref().and_then(dir_of) {
                // Vertical directions in this format are inverted relative to the screen.
                let screen = match d {
                    Dir::Up => Dir::Down,
                    Dir::Down => Dir::Up,
                    other => other,
                };
                if boolean(ms, "flick") {
                    self.diags
                        .info(path, "flick-stick turns are approximated as mouse movement");
                }
                return Some(Action::MouseMove {
                    mouse_move: screen,
                    speed: 800.0,
                });
            }
            if let Some(w) = enum_str(ms, "wheel").as_deref().and_then(wheel_of) {
                return Some(Action::Wheel {
                    wheel: w,
                    clicks: 1.0,
                });
            }
            self.diags.warn(path, "unrecognized mouse item");
            return None;
        }
        if let Some(g) = get(item, "gamepad") {
            return self.gamepad_item(g, path);
        }
        if let Some(pz) = get(item, "pause") {
            return Some(Action::Pause {
                pause: self.time(int(pz, "value").unwrap_or(50)),
            });
        }
        if let Some(r) = get(item, "rumble") {
            let motor = get(r, "amplitude").and_then(|a| get(a, "motor"));
            let amp = |side: &str| {
                motor.and_then(|m| int(m, side)).unwrap_or(80).clamp(0, 100) as f32 / 100.0
            };
            let ms = self.time(int(r, "duration").unwrap_or(15)).clamp(100, 2550);
            return Some(Action::Rumble {
                rumble: ms,
                low: amp("left"),
                high: amp("right"),
            });
        }
        if let Some(c) = get(item, "command") {
            let id = int(c, "id")?;
            let Some(cmd) = self.commands.get(&id).copied() else {
                self.diags
                    .warn(path, format!("command {id} is not defined"));
                return None;
            };
            let ty = int(cmd, "type").unwrap_or(0);
            let command = match ty {
                1 => Command::RemapOff,
                6 => Command::ToggleGyro,
                7 => Command::LogOff,
                8 => Command::Shutdown,
                9 => Command::Sleep,
                11 => Command::Screenshot,
                12 => Command::CloseWindow,
                13 => Command::Hibernate,
                15 => Command::GyroOn,
                16 => Command::GyroOff,
                17 => Command::Restart,
                1000..=1999 => {
                    let params = get(cmd, "params").cloned().unwrap_or(Value::Null);
                    Command::Launch {
                        path: text(&params, "path").unwrap_or_default(),
                        args: text(&params, "arguments").unwrap_or_default(),
                    }
                }
                _ => {
                    let name = text(cmd, "name").unwrap_or_else(|| format!("command type {ty}"));
                    self.diags
                        .warn(path, format!("command `{name}` is not supported"));
                    Command::Unsupported(name)
                }
            };
            return Some(Action::Command { command });
        }
        self.diags
            .warn(path, format!("unrecognized macro item {item}"));
        None
    }

    /// Virtual-gamepad output item.
    fn gamepad_item(&mut self, g: &Value, path: &str) -> Option<Action> {
        let id = int(g, "buttonId")?;
        let action = press_of(g);
        let value = int(g, "value");
        let deflection = |v: Option<i64>| {
            v.map(|v| ((v + 1) as f32 / 32767.0).clamp(0.0, 1.0))
                .unwrap_or(1.0)
        };
        let stick = |s: Stick, d: Dir| {
            let (x, y) = d.vector();
            let m = deflection(value);
            Some(Action::Stick {
                stick: s,
                x: x * m,
                y: y * m,
                action,
            })
        };
        match id {
            40 => stick(Stick::Left, Dir::Up),
            41 => stick(Stick::Left, Dir::Down),
            42 => stick(Stick::Left, Dir::Left),
            43 => stick(Stick::Left, Dir::Right),
            47 => stick(Stick::Right, Dir::Up),
            48 => stick(Stick::Right, Dir::Down),
            49 => stick(Stick::Right, Dir::Left),
            50 => stick(Stick::Right, Dir::Right),
            51 | 59 => Some(Action::Trigger {
                trigger: Trigger::Left,
                value: deflection(value),
                action,
            }),
            55 | 60 => Some(Action::Trigger {
                trigger: Trigger::Right,
                value: deflection(value),
                action,
            }),
            _ => match gamepad_control(id) {
                Some(Control::Button(pad)) => Some(Action::Pad { pad, action }),
                _ => {
                    let desc = text(g, "description").unwrap_or_default();
                    self.diags.warn(
                        path,
                        format!("virtual output {id} ({desc}) is not supported"),
                    );
                    None
                }
            },
        }
    }

    /// Low-level remaps.
    fn hardware_mapping(
        &mut self,
        cond: &Value,
        target: Option<&Value>,
        layer: u8,
        path: &str,
    ) -> Option<Mapping> {
        let target = target?;
        if get(cond, "stick").is_some() {
            self.diags.warn(
                path,
                "stick axis remaps are not supported; use the stick invert options instead",
            );
            return None;
        }
        if get(cond, "trigger").is_some() {
            self.diags.warn(path, "trigger swaps are not supported");
            return None;
        }
        let source = self.element(cond, path)?;
        let mut m = Mapping::new(source, Vec::new());
        m.layer = layer;
        if boolean(target, "unmap") {
            m.description = "unmapped".into();
            return Some(m);
        }
        let device = int(cond, "deviceId").and_then(|d| self.devices.get(&d).copied());
        let to = int(target, "buttonId")?;
        let action = match device {
            Some(DeviceKind::Gamepad) => match gamepad_control(to) {
                Some(Control::Button(pad)) => Action::Pad {
                    pad,
                    action: Press::Auto,
                },
                _ => {
                    self.diags
                        .warn(path, format!("cannot remap to gamepad input {to}"));
                    return None;
                }
            },
            Some(DeviceKind::Keyboard) => Action::Key {
                key: keyboard_key(target, &mut self.diags, path)?,
                action: Press::Auto,
            },
            Some(DeviceKind::Mouse) => Action::Mouse {
                mouse: mouse_button(to)?,
                action: Press::Auto,
            },
            None => return None,
        };
        m.actions.push(action);
        m.description = "hardware remap".into();
        Some(m)
    }
}

#[cfg(test)]
mod tests {
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
                speed: 800.0
            }
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
        let text = crate::to_toml(&p);
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
}
