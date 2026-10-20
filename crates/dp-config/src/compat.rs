//! Importer for JSON profiles (`schemaVersion` 3.0).
//!
//! The importer is lenient: anything it cannot represent becomes a
//! [`Diagnostic`](crate::Diagnostic) and is skipped, instead of failing the whole file.

use std::collections::HashMap;

use dp_core::control::{
    Control, Dir, GyroDir, Half, TouchGesture, TouchRegion, TrackpadPart, Zone,
};
use dp_core::keys::{Key, MouseButton, WheelDir};
use dp_core::pad::{PadButton, Stick, Trigger};
use dp_core::profile::{
    Action, Activator, AnalogMapping, AnalogSource, AnalogTarget, Combo, Command, Crosshair,
    CrosshairShape, GyroMode, JitterPattern, Jump, JumpMode, KbmSource, KeyRepeat, Layer,
    LayerSticks, LayerTriggers, LayerVirtualSticks, Light, LightMode, Magnifier, Mapping, Mode,
    Press, Profile, RadialApply, RadialMenu, RadialSector, RadialSelect, ShakeAxis, StickPatch,
    Sticks, TriggerEffect, TriggerPatch, Triggers, Unlock, Vibration, VirtualMotion, VirtualMouse,
    VirtualPadType, VirtualStick, VirtualStickPatch, VirtualSticks, ZoneRadius,
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

/// An effect like a vendor adaptive-trigger preset: `Some(None)` for presets that only
/// inherit, `None` for unknown ones.
fn adaptive_preset(name: &str) -> Option<Option<TriggerEffect>> {
    let resistance = |start, strength| Some(TriggerEffect::Resistance { start, strength });
    let vibration = |amplitude, frequency| {
        Some(TriggerEffect::Vibration {
            start: 0,
            amplitude,
            frequency,
        })
    };
    Some(match name {
        "none" => Some(TriggerEffect::Off),
        "donotinherit" | "inherited" => None,
        "fullpress" => resistance(0, 6),
        "softpress" => resistance(0, 2),
        "mediumpress" => resistance(0, 4),
        "hardpress" => resistance(0, 7),
        "halfpress" => resistance(5, 6),
        "rigidsoft" => resistance(0, 3),
        "rigidmedium" => resistance(0, 5),
        "rigidmax" => resistance(0, 8),
        "rifle" => Some(TriggerEffect::Weapon {
            start: 4,
            end: 6,
            strength: 8,
        }),
        "pulse" => vibration(4, 10),
        "choppy" => vibration(8, 5),
        "vibration" => vibration(6, 40),
        _ => return None,
    })
}

/// A light-bar entry (`mode`, `colors`, `loop`).
fn light(v: &Value) -> Light {
    let mode = match enum_str(v, "mode").as_deref() {
        Some("shimmer") => LightMode::Shimmer,
        Some("heartbeat") => LightMode::Heartbeat,
        Some("stroboscope") => LightMode::Strobe,
        Some("rainbow") => LightMode::Rainbow,
        Some("breath") => LightMode::Breath,
        _ => LightMode::Solid,
    };
    let colors: Vec<String> = array(v, "colors")
        .iter()
        .filter_map(|c| {
            let c: Vec<u8> = c
                .as_array()?
                .iter()
                .filter_map(|x| x.as_u64().map(|n| n.min(255) as u8))
                .collect();
            (c.len() == 3).then(|| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]))
        })
        .take(10)
        .collect();
    Light {
        mode,
        colors: if colors.is_empty() {
            Light::default().colors
        } else {
            colors
        },
        cycle: boolean(v, "loop"),
        ..Default::default()
    }
}

/// An `[r, g, b]` colour as `#rrggbb`.
fn rgb_hex(v: &Value, key: &str) -> Option<String> {
    let c: Vec<u8> = array(v, key)
        .iter()
        .filter_map(|x| x.as_u64().map(|n| n.min(255) as u8))
        .collect();
    (c.len() == 3).then(|| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]))
}

fn array<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    get(v, key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Zone order of the format's zone button ids.
const ZONES: [Zone; 3] = [Zone::Low, Zone::Med, Zone::High];
/// Trackpad direction ids in order: up, down, left, right, then the diagonals.
const TRACKPAD_REGIONS: [TouchRegion; 8] = [
    TouchRegion::Up,
    TouchRegion::Down,
    TouchRegion::Left,
    TouchRegion::Right,
    TouchRegion::UpLeft,
    TouchRegion::UpRight,
    TouchRegion::DownLeft,
    TouchRegion::DownRight,
];

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
        61..=63 => MouseZone(None, zone(61)),
        // Mouse zone × direction: low up/down/left/right, then med, then high.
        133..=144 => {
            let i = (id - 133) as usize;
            MouseZone(Some(Dir::ALL[i % 4]), ZONES[i / 4])
        }
        64 => MouseMove(Dir::Up),
        65 => MouseMove(Dir::Down),
        66 => MouseMove(Dir::Left),
        67 => MouseMove(Dir::Right),
        // Gyro: up, down, left, right, lean left, lean right; then zones in any direction;
        // then zone × direction (low up/down/left/right, med…, high…), then lean zones.
        68..=73 => Gyro(GyroDir::ALL[(id - 68) as usize]),
        244..=247 => Gyro(GyroDir::ALL[(id - 244) as usize]),
        74..=76 => GyroZone(None, zone(74)),
        145..=156 => {
            let i = (id - 145) as usize;
            GyroZone(Some(GyroDir::ALL[i % 4]), ZONES[i / 4])
        }
        157..=162 => {
            let i = (id - 157) as usize;
            let lean = [GyroDir::LeanLeft, GyroDir::LeanRight][i % 2];
            GyroZone(Some(lean), ZONES[i / 2])
        }
        // Touchpad gestures: taps left/right/centre, multi-tap, zoom in/out, swipes with one
        // finger (up/down/left/right), then with two.
        77..=90 => Gesture(
            [
                TouchGesture::TapLeft,
                TouchGesture::TapRight,
                TouchGesture::TapCenter,
                TouchGesture::MultiTap,
                TouchGesture::ZoomIn,
                TouchGesture::ZoomOut,
                TouchGesture::SwipeUp,
                TouchGesture::SwipeDown,
                TouchGesture::SwipeLeft,
                TouchGesture::SwipeRight,
                TouchGesture::Swipe2Up,
                TouchGesture::Swipe2Down,
                TouchGesture::Swipe2Left,
                TouchGesture::Swipe2Right,
            ][(id - 77) as usize],
        ),
        163 => Gesture(TouchGesture::Tap),
        // The second trackpad (right half of a split pad): directions, click, tap.
        100..=107 => Trackpad(
            Half::Right,
            TrackpadPart::Region(TRACKPAD_REGIONS[(id - 100) as usize]),
        ),
        108 => Trackpad(Half::Right, TrackpadPart::Click),
        164 => Trackpad(Half::Right, TrackpadPart::Tap),
        91 => Touch(TouchRegion::Up),
        92 => Touch(TouchRegion::Down),
        93 => Touch(TouchRegion::Left),
        94 => Touch(TouchRegion::Right),
        95 => Touch(TouchRegion::UpLeft),
        96 => Touch(TouchRegion::UpRight),
        97 => Touch(TouchRegion::DownLeft),
        98 => Touch(TouchRegion::DownRight),
        99 => Button(P::Touchpad),
        169 => TriggerFull(Trigger::Left),
        170 => TriggerFull(Trigger::Right),
        171 => Wheel(WheelDir::Up),
        172 => Wheel(WheelDir::Down),
        173 => Wheel(WheelDir::Left),
        174 => Wheel(WheelDir::Right),
        175 => StickDir(Stick::Left, Dir::UpLeft),
        176 => StickDir(Stick::Left, Dir::UpRight),
        177 => StickDir(Stick::Left, Dir::DownLeft),
        178 => StickDir(Stick::Left, Dir::DownRight),
        179 => StickDir(Stick::Right, Dir::UpLeft),
        180 => StickDir(Stick::Right, Dir::UpRight),
        181 => StickDir(Stick::Right, Dir::DownLeft),
        182 => StickDir(Stick::Right, Dir::DownRight),
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

/// A stick's response curve (`curves.points.horizontal`), sorted by travel.
fn curve_points(s: &Value) -> Option<Vec<[f32; 2]>> {
    let points = get(s, "curves")
        .and_then(|c| get(c, "points"))
        .map(|p| array(p, "horizontal"))
        .unwrap_or(&[]);
    if points.is_empty() {
        return None;
    }
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
    Some(curve)
}

/// The format's virtual stick defaults: buttons start at 7000/32768 and ramp up (initial
/// speed 8, acceleration 1).
fn vendor_virtual_stick() -> VirtualStick {
    VirtualStick {
        initial: 7000.0 / AXIS_MAX,
        speed: 8.0 * VIRTUAL_SPEED_UNIT,
        acceleration: VIRTUAL_ACCEL_UNIT,
        ..VirtualStick::default()
    }
}

/// One step of `initialSpeed`, in deflection per second (4096 / 32768).
const VIRTUAL_SPEED_UNIT: f32 = 0.125;
/// One step of `acceleration`, in deflection per second² (16384 / 32768).
const VIRTUAL_ACCEL_UNIT: f32 = 0.5;

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
        // A pulsed item is a short tap.
        _ if boolean(v, "pulsation") => Press::Tap,
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
    /// Per-layer stick and trigger settings by layer id, with where they came from.
    layer_analog: HashMap<i64, ((LayerSticks, LayerTriggers), String)>,
    /// Turn calibration from a stick's mouse settings (flick values).
    turn: Option<VirtualMouse>,
    /// The gyro stick entry asks for tilt mode.
    gyro_tilt: bool,
    /// The profile uses the second trackpad, so the touchpad is split into halves.
    split_touch: bool,
    /// Touchpad regions only while the pad is clicked.
    touch_click_required: bool,
    /// Speed (px/s) and acceleration of button-driven mouse motion, from the virtual mouse.
    mouse_speed: (f32, f32),
    /// Crosshair ids in profile order, and the index of each crosshair GUID.
    crosshair_ids: Vec<i64>,
    crosshair_guids: HashMap<String, usize>,
    /// Sector id of each radial-menu sector mask, and menu and position of each sector id.
    radial_masks: HashMap<i64, i64>,
    radial_sectors: HashMap<i64, (usize, usize)>,
    /// Light bars of layers, set once the layers exist.
    layer_lights: Vec<(u8, Light)>,
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
        layer_analog: HashMap::new(),
        turn: None,
        gyro_tilt: false,
        split_touch: false,
        touch_click_required: false,
        mouse_speed: (1000.0, 4.0),
        crosshair_ids: Vec::new(),
        crosshair_guids: HashMap::new(),
        radial_masks: HashMap::new(),
        radial_sectors: HashMap::new(),
        layer_lights: Vec::new(),
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
        if let Some(led) = get(&config, "led") {
            p.lightbar = get(led, "default").map(light);
            for s in array(led, "shifts") {
                if let Some(id) = int(s, "shiftId").filter(|id| (1..=255).contains(id)) {
                    self.layer_lights.push((id as u8, light(s)));
                }
            }
        }

        self.devices(&mut p);
        for c in array(self.root, "commands") {
            if let Some(id) = int(c, "id") {
                self.commands.insert(id, c);
            }
        }
        self.split_touch = self.uses_second_trackpad();
        p.touchpad.split = self.split_touch;
        self.crosshairs(&mut p);
        self.radial_menus(&mut p);
        self.masks(&mut p);
        self.layers(&mut p);
        for (id, light) in std::mem::take(&mut self.layer_lights) {
            match p.layers.iter_mut().find(|l| l.id == id) {
                Some(l) => l.lightbar = Some(light),
                None => self.diags.warn(
                    "config.led",
                    format!("light bar of layer {id}, which does not exist"),
                ),
            }
        }
        let mappings = array(self.root, "mappings");
        for (i, m) in mappings.iter().enumerate() {
            let path = format!("mappings[{i}]");
            if self.radial_sector_mapping(m, &mut p, &path) {
                continue;
            }
            if let Some(mapping) = self.mapping(m, &path) {
                p.mappings.push(mapping);
            }
        }
        fold_jump_pairs(&mut p.mappings);
        self.virtual_sticks(&mut p);
        if let Some(turn) = self.turn.take() {
            p.virtual_devices.mouse.counts_per_turn = turn.counts_per_turn;
            p.virtual_devices.mouse.turn_ms = turn.turn_ms;
        }
        if self.gyro_tilt {
            p.gyro.mode = GyroMode::Tilt;
        }
        p.touchpad.click_required = self.touch_click_required;
        // A mask a layer does not inherit stays uninherited in its zone form too.
        for l in &mut p.layers {
            for (mask, zoned) in &self.zoned {
                if l.not_inherited.contains(mask) && !l.not_inherited.contains(zoned) {
                    l.not_inherited.push(zoned.clone());
                }
            }
        }
        self.finish_radial(&mut p);
        p
    }

    fn devices(&mut self, p: &mut Profile) {
        let Some(devices) = get(self.root, "devices") else {
            self.diags.error("devices", "missing");
            return;
        };
        // Keyboard and mouse groups of injected input, and of the physical devices.
        let (mut scripts, mut physical) = (false, false);
        for (i, hw) in array(devices, "hardware").iter().enumerate() {
            let path = format!("devices.hardware[{i}]");
            let Some(id) = int(hw, "id") else {
                self.diags.warn(&path, "device without id");
                continue;
            };
            let kind = match enum_str(hw, "name").as_deref() {
                Some("gamepad") => DeviceKind::Gamepad,
                Some("keyboard") => {
                    physical = true;
                    DeviceKind::Keyboard
                }
                Some("mouse") => {
                    physical = true;
                    DeviceKind::Mouse
                }
                Some("sendinputkeyboard") => {
                    scripts = true;
                    DeviceKind::Keyboard
                }
                Some("sendinputmouse") => {
                    scripts = true;
                    DeviceKind::Mouse
                }
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
                    let (mut sticks, mut triggers) = (p.sticks.clone(), p.triggers.clone());
                    self.analog_settings(&mut sticks, &mut triggers, d, &format!("{path}.default"));
                    (p.sticks, p.triggers) = (sticks, triggers);
                    self.vibration(&mut p.vibration, d, &format!("{path}.default"));
                }
                // Per-layer blocks hold the same settings as `default`; a layer keeps only
                // what differs from the base.
                for (k, s) in array(hw, "shifts").iter().enumerate() {
                    let spath = format!("{path}.shifts[{k}]");
                    let Some(layer) = int(s, "shiftId") else {
                        self.diags.warn(&spath, "missing shiftId");
                        continue;
                    };
                    let (mut sticks, mut triggers) = (p.sticks.clone(), p.triggers.clone());
                    self.analog_settings(&mut sticks, &mut triggers, s, &spath);
                    let patch = (
                        LayerSticks {
                            left: StickPatch::between(&p.sticks.left, &sticks.left),
                            right: StickPatch::between(&p.sticks.right, &sticks.right),
                        },
                        LayerTriggers {
                            left: TriggerPatch::between(&p.triggers.left, &triggers.left),
                            right: TriggerPatch::between(&p.triggers.right, &triggers.right),
                        },
                    );
                    self.layer_analog.insert(layer, (patch, spath));
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
                // Spring mode is the format's default; the noise filter defaults to 3.
                route.spring = get(stick, "springMode")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                route.noise_filter = int(stick, "noiseFilter").unwrap_or(3).clamp(1, 20) as f32;
                if let Some(z) = int(stick, "snapZone") {
                    route.snap = (z as f32 / AXIS_MAX).clamp(0.0, 0.9);
                }
                self.diags.info(
                    format!("{path}.default.stick"),
                    "mouse-as-stick imported as a mouse → right stick route; tune its sensitivity",
                );
                p.analog.push(route);
            }
        }

        match (scripts, physical) {
            (true, false) => p.kbm_source = KbmSource::Scripts,
            (true, true) => self.diags.info(
                "devices.hardware",
                "keyboard and mouse mappings react to both the physical devices and injected input",
            ),
            _ => {}
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
        // Virtual mouse: button-driven speed starts at deflection · 125 · sensitivity / 4 px/s
        // and grows by acceleration / 2 of itself per second; the wheel repeats every
        // (17 − wheelDeflection) · 20 ms while held. Absent values are the format's defaults.
        let mouse = virt
            .and_then(|v| get(v, "mouse"))
            .and_then(|m| get(m, "config"))
            .cloned()
            .unwrap_or(Value::Null);
        let in_range = |key: &str, max: i64| int(&mouse, key).filter(|v| (1..=max).contains(v));
        let sensitivity = in_range("sensitivity", 100).unwrap_or(4) as f32;
        let deflection = in_range("deflection", 16).unwrap_or(8) as f32;
        let acceleration = int(&mouse, "acceleration")
            .filter(|v| (0..=16).contains(v))
            .unwrap_or(8);
        self.mouse_speed = (
            deflection * 125.0 * sensitivity / 4.0,
            acceleration as f32 / 2.0,
        );
        let wheel = in_range("wheelDeflection", 16).unwrap_or(13);
        p.virtual_devices.mouse.wheel_repeat_ms = ((17 - wheel) * 20) as u32;
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

    /// How mappings push the virtual sticks (`devices.virtual.gamepads[0]`), on the base
    /// layer and per layer. Sticks that buttons push follow the format's ramp even without
    /// settings; the others keep ours.
    fn virtual_sticks(&mut self, p: &mut Profile) {
        let path = "devices.virtual.gamepads[0]";
        let pad = get(self.root, "devices")
            .and_then(|d| get(d, "virtual"))
            .and_then(|v| array(v, "gamepads").first());
        let mut base = VirtualSticks {
            left: vendor_virtual_stick(),
            right: vendor_virtual_stick(),
        };
        let mut used = [false; 2];
        if let Some(d) = pad.and_then(|g| get(g, "default")) {
            used = self.virtual_stick_settings(
                &mut base,
                &mut p.virtual_devices.motion,
                d,
                &format!("{path}.default"),
            );
        }
        let mut layers = Vec::new();
        for (k, s) in pad
            .map(|g| array(g, "shifts"))
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            let spath = format!("{path}.shifts[{k}]");
            let Some(id) = int(s, "shiftId") else {
                self.diags.warn(&spath, "missing shiftId");
                continue;
            };
            let mut sticks = base.clone();
            let seen =
                self.virtual_stick_settings(&mut sticks, &mut p.virtual_devices.motion, s, &spath);
            used = [used[0] || seen[0], used[1] || seen[1]];
            layers.push((id, sticks, spath));
        }
        for a in p.mappings.iter().flat_map(|m| &m.actions) {
            if let Action::Stick { stick, .. } = a {
                used[*stick as usize] = true;
            }
        }
        let pick = |v: &VirtualStick, i: usize| {
            if used[i] {
                v.clone()
            } else {
                VirtualStick::default()
            }
        };
        let main = VirtualSticks {
            left: pick(&base.left, 0),
            right: pick(&base.right, 1),
        };
        for (id, sticks, spath) in layers {
            let patch = LayerVirtualSticks {
                left: VirtualStickPatch::between(&main.left, &pick(&sticks.left, 0)),
                right: VirtualStickPatch::between(&main.right, &pick(&sticks.right, 1)),
            };
            match p.layers.iter_mut().find(|l| i64::from(l.id) == id) {
                Some(l) => l.virtual_devices.sticks = patch,
                None => self.diags.warn(
                    spath,
                    format!("settings for layer {id}, which does not exist"),
                ),
            }
        }
        p.virtual_devices.sticks = main;
    }

    /// One block of virtual stick settings; returns which sticks (left, right) it set.
    fn virtual_stick_settings(
        &mut self,
        sticks: &mut VirtualSticks,
        motion: &mut VirtualMotion,
        d: &Value,
        path: &str,
    ) -> [bool; 2] {
        let mut seen = [false; 2];
        for s in array(d, "sticks") {
            // Shake settings belong to the virtual accelerometer, whichever entry holds them.
            if let Some(v) = int(s, "shakingDuration").filter(|v| *v > 0) {
                motion.shake_ms = v.clamp(50, 300) as u32;
            }
            if let Some(v) = int(s, "shakingAmplitude").filter(|v| *v > 0) {
                motion.shake_g = v.clamp(1, 8) as f32;
            }
            let (cfg, i) = match enum_str(s, "name").as_deref() {
                Some("left") => (&mut sticks.left, 0),
                Some("right") => (&mut sticks.right, 1),
                Some("gyro") => {
                    self.diags.info(
                        format!("{path}.sticks"),
                        "virtual gyro stick settings are not supported",
                    );
                    continue;
                }
                _ => continue,
            };
            seen[i] = true;
            let fraction = |v: i64| (v as f32 / AXIS_MAX).clamp(0.0, 1.0);
            if let Some(v) = int(s, "initialDeflection") {
                cfg.initial = fraction(v);
            }
            if let Some(v) = int(s, "maxDeflection") {
                cfg.max = fraction(v);
            }
            if let Some(v) = int(s, "initialSpeed") {
                cfg.speed = v.clamp(0, 16) as f32 * VIRTUAL_SPEED_UNIT;
            }
            if let Some(v) = int(s, "acceleration") {
                cfg.acceleration = v.clamp(0, 4) as f32 * VIRTUAL_ACCEL_UNIT;
            }
            // A multiplier of 0 means 1.
            if let Some(v) = int(s, "multiplierX") {
                cfg.multiplier_x = v.clamp(1, 5) as f32;
            }
            if let Some(v) = int(s, "multiplierY") {
                cfg.multiplier_y = v.clamp(1, 5) as f32;
            }
            if let Some(curve) = curve_points(s) {
                cfg.curve = curve;
            }
            let vertical: Vec<&str> = [
                "initialDeflectionY",
                "maxDeflectionY",
                "initialSpeedY",
                "accelerationY",
            ]
            .into_iter()
            .filter(|k| get(s, k).is_some())
            .collect();
            if !vertical.is_empty() {
                self.diags.info(
                    format!("{path}.sticks"),
                    format!(
                        "separate vertical values ({}) are not supported; the horizontal ones apply",
                        vertical.join(", ")
                    ),
                );
            }
            // Jitter sizes are 1–100 for 0.1–10 % of full deflection.
            let j = &mut cfg.jitter;
            j.enabled = boolean(s, "jitter");
            j.pattern = match enum_str(s, "jitterType").as_deref() {
                Some("square") => JitterPattern::Square,
                Some("cross") => JitterPattern::Cross,
                _ => JitterPattern::Circle,
            };
            let size = |v: i64| v.clamp(1, 100) as f32 / 1000.0;
            if let Some(v) = int(s, "jitterDeflectionMin") {
                j.min = size(v);
            }
            if let Some(v) = int(s, "jitterDeflectionMax") {
                j.max = size(v);
            }
            if let Some(v) = int(s, "jitterDelayMin") {
                j.delay_min_ms = v.clamp(10, 100) as u32;
            }
            if let Some(v) = int(s, "jitterDelayMax") {
                j.delay_max_ms = v.clamp(10, 100) as u32;
            }
            (j.min, j.max) = (j.min.min(j.max), j.min.max(j.max));
            (j.delay_min_ms, j.delay_max_ms) = (
                j.delay_min_ms.min(j.delay_max_ms),
                j.delay_min_ms.max(j.delay_max_ms),
            );
            if j.enabled && int(s, "jitterOptionalParameter").is_some_and(|v| v != 1) {
                self.diags.info(
                    format!("{path}.sticks"),
                    "the jitter's optional parameter is not supported",
                );
            }
        }
        seen
    }

    /// Flick values in a stick's mouse settings: the turn calibration (sensitivity is
    /// counts per degree × 10) and the flick length.
    fn flick_settings(&mut self, mc: &Value, path: &str) {
        let sens = int(mc, "flickSensitivity");
        let ms = int(mc, "flickDuration");
        if sens.is_none() && ms.is_none() {
            return;
        }
        let turn = self.turn.get_or_insert_with(VirtualMouse::default);
        if let Some(v) = sens {
            turn.counts_per_turn = 36.0 * v.clamp(1, 2000) as f32;
        }
        if let Some(v) = ms {
            turn.turn_ms = v.clamp(0, 1500) as u32;
        }
        if get(mc, "flickThreshold").is_some() || get(mc, "flickSmoothing").is_some() {
            self.diags.info(
                path,
                "flick threshold and smoothing are set on a flick route; add one to use them",
            );
        }
    }

    /// Gamepad stick/trigger/touchpad settings.
    /// Motor strengths, a motor swap and the command gap from a device's `default` block.
    fn vibration(&mut self, v: &mut Vibration, d: &Value, path: &str) {
        let mut swapped = [false; 2];
        for item in array(d, "vibration") {
            if get(item, "trigger").is_some() {
                self.diags.info(
                    format!("{path}.vibration"),
                    "trigger rumble settings are not supported",
                );
                continue;
            }
            let Some(m) = get(item, "motor") else {
                continue;
            };
            let (strength, other, i) = match enum_str(m, "name").as_deref() {
                Some("left") => (&mut v.left, "insteadMotorRight", 0),
                Some("right") => (&mut v.right, "insteadMotorLeft", 1),
                _ => continue,
            };
            if let Some(a) = get(m, "amplitude").and_then(Value::as_f64) {
                *strength = (a / 100.0).clamp(0.0, 1.0) as f32;
            }
            if boolean(m, "turnedOff") {
                *strength = 0.0;
            }
            swapped[i] = boolean(m, other);
        }
        v.swap = swapped == [true, true];
        if swapped[0] != swapped[1] {
            self.diags.info(
                format!("{path}.vibration"),
                "one motor redirected onto the other is not supported; the motors stay apart",
            );
        }
        if let Some(gap) = int(d, "vibrationRumbleDelay") {
            v.min_delay_ms = gap.clamp(10, 200) as u32;
        }
        if get(d, "vibrationRumbleMode").is_some() {
            self.diags.info(
                format!("{path}.vibrationRumbleMode"),
                "the rumble mode is not imported",
            );
        }
    }

    fn analog_settings(
        &mut self,
        sticks: &mut Sticks,
        triggers: &mut Triggers,
        d: &Value,
        path: &str,
    ) {
        for s in array(d, "sticks") {
            let cfg = match enum_str(s, "name").as_deref() {
                Some("left") => &mut sticks.left,
                Some("right") => &mut sticks.right,
                Some("gyro") => {
                    if boolean(s, "tilt") {
                        self.gyro_tilt = true;
                    }
                    self.diags.info(
                        format!("{path}.sticks"),
                        "gyro sensitivity and curves are not imported; tune the gyro route",
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
            cfg.diagonals = boolean(s, "diagonalZones");
            // The direction sensitivity is the tangent of the cardinal sector's half width,
            // in 1/32768 units.
            if let Some(sens) = get(s, "sensitivity") {
                let parts: Vec<f32> = [int(sens, "x"), int(sens, "y")]
                    .into_iter()
                    .flatten()
                    .map(|v| v.clamp(1, 32768) as f32 / AXIS_MAX)
                    .collect();
                if !parts.is_empty() {
                    let ratio = parts.iter().sum::<f32>() / parts.len() as f32;
                    cfg.direction_width = ratio.atan().to_degrees().clamp(1.0, 44.0);
                }
            }
            if let Some(mc) = get(s, "mouse") {
                self.flick_settings(mc, &format!("{path}.sticks"));
            }
            if let Some(r) = int(s, "rotation") {
                cfg.rotation = r.clamp(-90, 90) as f32;
            }
            if let Some(curve) = curve_points(s) {
                cfg.curve = curve;
            }
        }
        for t in array(d, "triggers") {
            let cfg = match enum_str(t, "name").as_deref() {
                Some("left") => &mut triggers.left,
                Some("right") => &mut triggers.right,
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
            if let Some(preset) = enum_str(t, "adaptiveDescription") {
                match adaptive_preset(&preset) {
                    Some(effect) => {
                        cfg.effect = effect;
                        self.diags.info(
                            format!("{path}.triggers"),
                            format!(
                                "adaptive trigger preset `{preset}` is played as a similar effect"
                            ),
                        );
                    }
                    None => self.diags.info(
                        format!("{path}.triggers"),
                        format!("adaptive trigger preset `{preset}` is not supported"),
                    ),
                }
            }
        }
        for tp in array(d, "touchpads") {
            if boolean(tp, "zoneClickRequired") {
                self.touch_click_required = true;
            }
        }
        if !array(d, "touchpads").is_empty() {
            self.diags.info(
                format!("{path}.touchpads"),
                "touchpad tuning is imported as region and gesture buttons and the click switch only",
            );
        }
    }

    /// Whether masks use the second trackpad (right half), which means the touchpad is
    /// split into two trackpads.
    fn uses_second_trackpad(&self) -> bool {
        array(self.root, "masks")
            .iter()
            .flat_map(|m| array(m, "set"))
            .any(|e| {
                let gamepad = int(e, "deviceId")
                    .and_then(|d| self.devices.get(&d))
                    .is_some_and(|k| *k == DeviceKind::Gamepad);
                gamepad && int(e, "buttonId").is_some_and(|id| matches!(id, 100..=108 | 164))
            })
    }

    /// A gamepad control id; on a split touchpad the first trackpad's ids mean the left half.
    fn pad_control(&self, id: i64) -> Option<Control> {
        let c = gamepad_control(id)?;
        if !self.split_touch {
            return Some(c);
        }
        Some(match (id, c) {
            (91..=98, Control::Touch(r)) => Control::Trackpad(Half::Left, TrackpadPart::Region(r)),
            (99, _) => Control::Trackpad(Half::Left, TrackpadPart::Click),
            (163, _) => Control::Trackpad(Half::Left, TrackpadPart::Tap),
            _ => c,
        })
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
                (Some(DeviceKind::Gamepad), Some(id)) => self.pad_control(id),
                (Some(DeviceKind::Keyboard), Some(_)) => {
                    return keyboard_key(e, &mut self.diags, path).map(Control::Key);
                }
                (Some(DeviceKind::Mouse), Some(id)) => mouse_button(id)
                    .map(Control::Mouse)
                    .or_else(|| match gamepad_control(id) {
                        c @ Some(
                            Control::MouseMove(_) | Control::MouseZone(..) | Control::Wheel(_),
                        ) => c,
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

    fn masks(&mut self, p: &mut Profile) {
        for (i, m) in array(self.root, "masks").iter().enumerate() {
            let path = format!("masks[{i}]");
            let Some(id) = int(m, "id") else { continue };
            let set = array(m, "set");
            if set.is_empty() {
                // A crosshair mask shows its crosshair on a layer; mappings on it are dropped.
                for c in array(m, "crosshairSet") {
                    let index = text(c, "crosshairGuid").and_then(|g| self.crosshair_guids.get(&g));
                    match (index, int(c, "layer")) {
                        (Some(&x), Some(layer @ 0..=255)) => {
                            let layers = &mut p.crosshairs[x].layers;
                            if !layers.contains(&(layer as u8)) {
                                layers.push(layer as u8);
                            }
                        }
                        _ => self
                            .diags
                            .warn(&path, "crosshair mask of an unknown crosshair"),
                    }
                }
                // A sector mask: mappings on it are what the sector does.
                if let Some(s) = array(m, "radialMenuSet")
                    .first()
                    .and_then(|s| int(s, "sectorId"))
                {
                    self.radial_masks.insert(id, s);
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

    /// Circles become radial menus (in id order) and their sectors keep their order. Root
    /// circles open with the radial-menu layer; the others are sub-menus.
    fn radial_menus(&mut self, p: &mut Profile) {
        let mut circles: Vec<&Value> = array(self.root, "radialMenuCircles").iter().collect();
        if circles.is_empty() {
            return;
        }
        circles.sort_by_key(|c| int(c, "id").unwrap_or(i64::MAX));
        let sectors = array(self.root, "radialMenuSectors");
        let layer = array(self.root, "shifts")
            .iter()
            .find(|s| enum_str(s, "type").as_deref() == Some("radialmenu"))
            .and_then(|s| int(s, "id"))
            .filter(|id| (1..=255).contains(id));
        let index: HashMap<i64, usize> = circles
            .iter()
            .enumerate()
            .filter_map(|(i, c)| int(c, "id").map(|id| (id, i)))
            .collect();
        let mut icons = false;
        for (i, c) in circles.iter().enumerate() {
            let root = get(c, "parentSectorId").is_none();
            let mut menu = RadialMenu {
                name: format!("Menu {}", i + 1),
                layer: if root { layer.unwrap_or(0) as u8 } else { 0 },
                ..Default::default()
            };
            if boolean(c, "isDelayBeforeOpening") {
                menu.delay_ms = int(c, "delayBeforeOpening").unwrap_or(500).clamp(0, 5000) as u32;
            }
            if boolean(c, "isTintedBackground") {
                let t = get(c, "tintBackground")
                    .and_then(Value::as_f64)
                    .unwrap_or(40.0);
                menu.tint = (if t > 1.0 { t / 100.0 } else { t }).clamp(0.0, 1.0) as f32;
            }
            for sid in array(c, "sectors").iter().filter_map(Value::as_i64) {
                let Some(s) = sectors.iter().find(|s| int(s, "id") == Some(sid)) else {
                    continue;
                };
                icons |= get(s, "icon").is_some();
                self.radial_sectors.insert(sid, (i, menu.sectors.len()));
                menu.sectors.push(RadialSector {
                    label: text(s, "description").unwrap_or_default(),
                    color: rgb_hex(s, "color").unwrap_or_default(),
                    submenu: int(s, "childCircleId")
                        .and_then(|c| index.get(&c))
                        .map(|&m| m as u8 + 1),
                    close: get(s, "isCloseOnApply")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    ..Default::default()
                });
            }
            p.radial_menus.push(menu);
        }
        if layer.is_none() {
            self.diags.warn(
                "radialMenuCircles",
                "no radial-menu layer; the menus only open as sub-menus",
            );
        }
        if icons {
            self.diags
                .info("radialMenuSectors", "sector icons are not imported");
        }
    }

    /// A mapping on a sector mask: its macro becomes the sector's actions.
    fn radial_sector_mapping(&mut self, m: &Value, p: &mut Profile, path: &str) -> bool {
        let Some(&sector) = get(m, "condition")
            .and_then(|c| get(c, "mask"))
            .and_then(|k| int(k, "id"))
            .and_then(|id| self.radial_masks.get(&id))
        else {
            return false;
        };
        let Some(&(menu, k)) = self.radial_sectors.get(&sector) else {
            self.diags
                .warn(path, "mapping of an unknown radial-menu sector");
            return true;
        };
        for (j, item) in array(m, "macros").iter().enumerate() {
            if let Some(a) = self.macro_item(item, &format!("{path}.macros[{j}]")) {
                p.radial_menus[menu].sectors[k].actions.push(a);
            }
        }
        true
    }

    /// Menus no mapping navigates point with the right stick; without an apply command they
    /// run the pointed sector when their layer ends.
    fn finish_radial(&self, p: &mut Profile) {
        let uses = |want: &dyn Fn(&Command) -> bool| {
            p.mappings
                .iter()
                .flat_map(|m| &m.actions)
                .any(|a| matches!(a, Action::Command { command } if want(command)))
        };
        let apply = uses(&|c| *c == Command::RadialApply);
        let navigate = uses(&|c| {
            matches!(
                c,
                Command::RadialNext
                    | Command::RadialPrev
                    | Command::RadialUp
                    | Command::RadialDown
                    | Command::RadialLeft
                    | Command::RadialRight
            )
        });
        for menu in &mut p.radial_menus {
            menu.apply = if apply {
                RadialApply::Command
            } else {
                RadialApply::Release
            };
            menu.select = if navigate {
                RadialSelect::Commands
            } else {
                RadialSelect::RightStick
            };
        }
    }

    fn crosshairs(&mut self, p: &mut Profile) {
        let mut items: Vec<&Value> = array(self.root, "crosshairs").iter().collect();
        items.sort_by_key(|c| int(c, "id").unwrap_or(i64::MAX));
        let rgb = rgb_hex;
        let num = |c: &Value, key: &str| get(c, key).and_then(Value::as_f64);
        let mut shapes = false;
        for (i, c) in items.into_iter().enumerate() {
            if i == 10 {
                self.diags
                    .warn("crosshairs", "only the first ten crosshairs were kept");
                break;
            }
            let d = Crosshair::default();
            shapes |= get(c, "crosshairType").is_some();
            let magnifier = boolean(c, "useMagnifier").then(|| Magnifier {
                zoom: num(c, "magnification").unwrap_or(2.0).clamp(1.0, 16.0) as f32,
                size: num(c, "magnificationSizePercents")
                    .map_or(0.2, |v| v / 100.0)
                    .clamp(0.1, 1.0) as f32,
            });
            let delta = |key| int(c, key).unwrap_or(0).clamp(-10_000, 10_000) as i16;
            p.crosshairs.push(Crosshair {
                name: text(c, "name").unwrap_or_default(),
                shape: CrosshairShape::Cross,
                color: rgb(c, "color").unwrap_or(d.color),
                opacity: num(c, "opacity").map_or(1.0, |v| (v / 100.0).clamp(0.0, 1.0) as f32),
                size: int(c, "size").map_or(d.size, |v| v.clamp(10, 200) as u16),
                thickness: int(c, "thickness").map_or(d.thickness, |v| v.clamp(10, 100) as u8),
                outline: num(c, "edgingThickness")
                    .map_or(0, |v| (v / 5.0).round().clamp(0.0, 10.0) as u8),
                outline_color: rgb(c, "edgingColor").unwrap_or(d.outline_color),
                offset: [delta("horizontalDelta"), delta("verticalDelta")],
                visible: get(c, "useCrosshair")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                magnifier,
                layers: Vec::new(),
            });
            if let Some(g) = text(c, "guid") {
                self.crosshair_guids.insert(g, i);
            }
            self.crosshair_ids.push(int(c, "id").unwrap_or(-1));
        }
        if shapes {
            self.diags.info(
                "crosshairs",
                "crosshairs are drawn with Doppelpad's own shapes; each became a cross",
            );
        }
    }

    fn layers(&mut self, p: &mut Profile) {
        for (i, s) in array(self.root, "shifts").iter().enumerate() {
            let path = format!("shifts[{i}]");
            let Some(id) = int(s, "id").filter(|id| (1..=255).contains(id)) else {
                self.diags.warn(&path, "invalid layer id");
                continue;
            };
            let ty = enum_str(s, "type");
            let unlock = match ty.as_deref() {
                Some("unlock") => Some(Unlock::All),
                Some("unlockkeyboardonly") => Some(Unlock::Keyboard),
                Some("unlockmouseonly") => Some(Unlock::Mouse),
                _ => None,
            };
            let kind = match ty.as_deref() {
                Some("toggle") => LayerKind::Toggle,
                _ if unlock.is_some() => LayerKind::Unlock,
                Some("radialmenu") => LayerKind::RadialMenu,
                _ => LayerKind::Common,
            };
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
            let (sticks, triggers) = self
                .layer_analog
                .remove(&id)
                .map(|(patch, _)| patch)
                .unwrap_or_default();
            p.layers.push(Layer {
                id: id as u8,
                name: text(s, "description").unwrap_or_else(|| format!("Layer {id}")),
                inherit: true,
                not_inherited,
                sticks,
                triggers,
                unlock,
                ..Default::default()
            });

            // Legacy modifier (one button or several held together) → synthesized jump mapping.
            let set = array(s, "set");
            let controls: Vec<Control> = set
                .iter()
                .enumerate()
                .filter_map(|(k, e)| self.element(e, &format!("{path}.set[{k}]")))
                .collect();
            if !set.is_empty() && controls.len() == set.len() {
                let mut m = Mapping::new(Combo(controls), Vec::new());
                m.jump = Some(Jump {
                    layer: id as u8,
                    mode: if kind == LayerKind::Toggle {
                        JumpMode::Toggle
                    } else {
                        JumpMode::Hold
                    },
                    delay_ms: None,
                    postpone: false,
                });
                m.description = format!("layer {id} modifier");
                p.mappings.push(m);
            }
        }
        let mut orphans: Vec<_> = self.layer_analog.drain().collect();
        orphans.sort_by_key(|(id, _)| *id);
        for (id, (_, path)) in orphans {
            self.diags.warn(
                path,
                format!("settings for layer {id}, which does not exist"),
            );
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
                postpone: boolean(j, "postpone"),
            });
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
                // A flick item turns a quarter to that side.
                if boolean(ms, "flick") && matches!(d, Dir::Left | Dir::Right) {
                    let turn = if d == Dir::Left { -90.0 } else { 90.0 };
                    return Some(Action::Turn { turn });
                }
                let (speed, acceleration) = self.mouse_speed;
                return Some(Action::MouseMove {
                    mouse_move: screen,
                    speed,
                    acceleration,
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
                2 => Command::PowerOff,
                4 => Command::SwitchGyroAxis,
                5 => Command::ResetTilt,
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
                101..=110 => {
                    // By crosshair id; the position counts in the profile.
                    let id = ty - 100;
                    let n = self
                        .crosshair_ids
                        .iter()
                        .position(|&c| c == id)
                        .map_or(id, |i| i as i64 + 1);
                    Command::Crosshair(n as u8)
                }
                120 => Command::ToggleCrosshair,
                130 => Command::RadialPrev,
                131 => Command::RadialNext,
                132 => Command::RadialApply,
                133 => Command::RadialBack,
                134 => Command::RadialUp,
                135 => Command::RadialDown,
                136 => Command::RadialRight,
                137 => Command::RadialLeft,
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
                absolute: boolean(g, "absolute"),
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
            // Gestures on the virtual touchpad; `value` is the length.
            77..=90 => match gamepad_control(id) {
                Some(Control::Gesture(touch)) => Some(Action::Touch {
                    touch,
                    ms: value.map(|v| self.time(v)),
                }),
                _ => None,
            },
            // Accelerometer shakes: x+, x-, y+, y-, z+, z-.
            248..=253 => Some(Action::Shake {
                shake: ShakeAxis::ALL[(id - 248) as usize],
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
        let text = crate::to_toml(&p);
        assert!(text.contains(r#"kbm_source = "scripts""#), "{text}");
        let (p, d) = import(&doc(
            r#"{ "id": 1, "name": "keyboard" }, { "id": 2, "name": "sendInputMouse" }"#,
        ))
        .unwrap();
        assert_eq!(p.kbm_source, KbmSource::All, "both: everything counts");
        assert!(d.iter().any(|x| x.to_string().contains("injected input")));
        let (p, _) = import(&doc(r#"{ "id": 1, "name": "keyboard" }"#)).unwrap();
        assert!(
            !crate::to_toml(&p).contains("kbm_source"),
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
        let toml = crate::to_toml(&p);
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
        let toml = crate::to_toml(&p);
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
}
