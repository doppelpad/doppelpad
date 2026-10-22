//! The format's button, key and direction ids.

use dp_core::control::{
    Control, Dir, GyroDir, Half, TouchGesture, TouchRegion, TrackpadPart, Zone,
};
use dp_core::keys::{Key, MouseButton, WheelDir};
use dp_core::pad::{PadButton, Stick, Trigger};
use dp_core::profile::Press;
use serde_json::Value;

use super::json::{boolean, enum_str, int, text};
use crate::diag::Diagnostics;

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
pub(super) fn gamepad_control(id: i64) -> Option<Control> {
    use Control::*;
    use PadButton as P;
    // Zone ids run low, medium, high, like `Zone::ALL`.
    let zone = |first: i64| Zone::ALL[(id - first) as usize];
    Some(match id {
        37..=39 => StickZone(Stick::Left, None, zone(37)),
        44..=46 => StickZone(Stick::Right, None, zone(44)),
        52..=54 => TriggerZone(Trigger::Left, zone(52)),
        56..=58 => TriggerZone(Trigger::Right, zone(56)),
        // Zone × direction: low up/down/left/right, then med, then high; left stick first.
        109..=132 => {
            let i = (id - 109) as usize;
            let stick = if i < 12 { Stick::Left } else { Stick::Right };
            StickZone(stick, Some(Dir::CARDINALS[i % 4]), Zone::ALL[i % 12 / 4])
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
            MouseZone(Some(Dir::CARDINALS[i % 4]), Zone::ALL[i / 4])
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
            GyroZone(Some(GyroDir::ALL[i % 4]), Zone::ALL[i / 4])
        }
        157..=162 => {
            let i = (id - 157) as usize;
            let lean = [GyroDir::LeanLeft, GyroDir::LeanRight][i % 2];
            GyroZone(Some(lean), Zone::ALL[i / 2])
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
        91..=98 => Touch(TRACKPAD_REGIONS[(id - 91) as usize]),
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
pub(super) fn key_from_description(desc: &str) -> Option<Key> {
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
pub(super) fn keyboard_key(v: &Value, diags: &mut Diagnostics, path: &str) -> Option<Key> {
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

pub(super) fn mouse_button(id: i64) -> Option<MouseButton> {
    match id {
        1 => Some(MouseButton::Left),
        2 => Some(MouseButton::Right),
        3 => Some(MouseButton::Middle),
        4 => Some(MouseButton::X1),
        5 => Some(MouseButton::X2),
        _ => None,
    }
}

/// The zone control of a stick direction or trigger press, for zone activators.
pub(super) fn in_zone(c: Control, z: Zone) -> Option<Control> {
    match c {
        Control::StickDir(s, d) => Some(Control::StickZone(s, Some(d), z)),
        Control::TriggerPress(t) => Some(Control::TriggerZone(t, z)),
        _ => None,
    }
}

pub(super) fn dir_of(s: &str) -> Option<Dir> {
    match s {
        "up" => Some(Dir::Up),
        "down" => Some(Dir::Down),
        "left" => Some(Dir::Left),
        "right" => Some(Dir::Right),
        _ => None,
    }
}

pub(super) fn wheel_of(s: &str) -> Option<WheelDir> {
    match s {
        "up" => Some(WheelDir::Up),
        "down" => Some(WheelDir::Down),
        "left" => Some(WheelDir::Left),
        "right" => Some(WheelDir::Right),
        _ => None,
    }
}

pub(super) fn press_of(v: &Value) -> Press {
    match enum_str(v, "action").as_deref() {
        Some("down" | "single_down") => Press::Down,
        Some("up") => Press::Up,
        // A pulsed item is a short tap.
        _ if boolean(v, "pulsation") => Press::Tap,
        _ => Press::Auto,
    }
}
