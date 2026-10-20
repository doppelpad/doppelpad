//! Names for every input a mapping can be triggered by ([`Control`]) and the
//! string syntax used for them in profiles.
//!
//! | Syntax                      | Meaning                                        |
//! |-----------------------------|------------------------------------------------|
//! | `A`, `LB`, `DpadUp`, ...    | gamepad button ([`PadButton`] name)            |
//! | `LeftStickUp`, ...          | stick pushed past its deadzone in a direction  |
//! | `LeftStickLow`, ...         | stick deflected into a zone, any direction     |
//! | `LeftStickUpHigh`, ...      | stick deflected into a zone in a direction     |
//! | `LT`, `RT`                  | trigger pulled past its deadzone               |
//! | `LTLow`, `RTMed`, ...       | trigger pulled into a zone                     |
//! | `LTFull`, `RTFull`          | trigger fully pulled                           |
//! | `TouchUp`, `TouchCenter`... | finger resting in a touchpad region            |
//! | `TouchTapLeft`, `TouchSwipe2Up`... | touchpad gesture (a short press)     |
//! | `TrackpadLeftUp`, `TrackpadRightTap`... | one half of a split touchpad    |
//! | `GyroUp`, `GyroLeanLeft`... | controller tilted / turned in a direction      |
//! | `GyroLow`, `GyroUpHigh`...  | gyro or tilt into a zone (any or one direction) |
//! | `Key:Space`                 | keyboard key ([`Key`] name)                    |
//! | `Mouse:Left`                | mouse button                                   |
//! | `Wheel:Up`                  | one wheel notch (momentary)                    |
//! | `MouseMove:Up`              | physical mouse moving in a direction           |

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::keys::{Key, MouseButton, WheelDir};
use crate::pad::{PadButton, Stick, Trigger};

/// Direction: the four cardinals, and the four diagonals for sources that tell them apart.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
    UpLeft,
    UpRight,
    DownLeft,
    DownRight,
}

impl Dir {
    /// The four cardinal directions.
    pub const ALL: [Dir; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];
    pub const DIAGONALS: [Dir; 4] = [Dir::UpLeft, Dir::UpRight, Dir::DownLeft, Dir::DownRight];
    /// Cardinals, then diagonals.
    pub const ALL8: [Dir; 8] = [
        Dir::Up,
        Dir::Down,
        Dir::Left,
        Dir::Right,
        Dir::UpLeft,
        Dir::UpRight,
        Dir::DownLeft,
        Dir::DownRight,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Dir::Up => "Up",
            Dir::Down => "Down",
            Dir::Left => "Left",
            Dir::Right => "Right",
            Dir::UpLeft => "UpLeft",
            Dir::UpRight => "UpRight",
            Dir::DownLeft => "DownLeft",
            Dir::DownRight => "DownRight",
        }
    }

    fn parse(s: &str) -> Option<Dir> {
        Dir::ALL8
            .into_iter()
            .find(|d| d.name().eq_ignore_ascii_case(s))
    }

    pub fn is_diagonal(self) -> bool {
        Dir::DIAGONALS.contains(&self)
    }

    /// The diagonal between a vertical and a horizontal cardinal.
    pub fn diagonal(vertical: Dir, horizontal: Dir) -> Dir {
        match (vertical, horizontal) {
            (Dir::Up, Dir::Left) => Dir::UpLeft,
            (Dir::Up, _) => Dir::UpRight,
            (_, Dir::Left) => Dir::DownLeft,
            _ => Dir::DownRight,
        }
    }

    /// Unit vector (x right, y up).
    pub fn vector(self) -> (f32, f32) {
        const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
        match self {
            Dir::Up => (0.0, 1.0),
            Dir::Down => (0.0, -1.0),
            Dir::Left => (-1.0, 0.0),
            Dir::Right => (1.0, 0.0),
            Dir::UpLeft => (-D, D),
            Dir::UpRight => (D, D),
            Dir::DownLeft => (-D, -D),
            Dir::DownRight => (D, -D),
        }
    }
}

/// Band of a stick's deflection or a trigger's pull. Bands are exclusive: a
/// control is in at most one zone at a time.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Zone {
    Low,
    Med,
    High,
}

impl Zone {
    pub const ALL: [Zone; 3] = [Zone::Low, Zone::Med, Zone::High];

    pub fn name(self) -> &'static str {
        match self {
            Zone::Low => "Low",
            Zone::Med => "Med",
            Zone::High => "High",
        }
    }

    /// Split a trailing zone name off a lowercase control name (`"rthigh"` →
    /// `("rt", High)`).
    fn strip_suffix(lower: &str) -> Option<(&str, Zone)> {
        [
            ("low", Zone::Low),
            ("medium", Zone::Med),
            ("med", Zone::Med),
            ("high", Zone::High),
        ]
        .into_iter()
        .find_map(|(suffix, z)| lower.strip_suffix(suffix).map(|rest| (rest, z)))
    }
}

/// A touchpad gesture, recognised from the finger tracks; each is a short press. Also played
/// on the virtual controller's touchpad by touch actions (snake case there: `swipe2_left`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchGesture {
    /// A short tap anywhere.
    Tap,
    /// A tap in the left, middle or right third.
    TapLeft,
    TapCenter,
    TapRight,
    /// Two fingers tapping together.
    MultiTap,
    SwipeUp,
    SwipeDown,
    SwipeLeft,
    SwipeRight,
    /// Two-finger swipes.
    Swipe2Up,
    Swipe2Down,
    Swipe2Left,
    Swipe2Right,
    /// Two fingers spreading apart, or pinching together.
    ZoomIn,
    ZoomOut,
}

impl TouchGesture {
    pub const ALL: [TouchGesture; 15] = [
        TouchGesture::Tap,
        TouchGesture::TapLeft,
        TouchGesture::TapCenter,
        TouchGesture::TapRight,
        TouchGesture::MultiTap,
        TouchGesture::SwipeUp,
        TouchGesture::SwipeDown,
        TouchGesture::SwipeLeft,
        TouchGesture::SwipeRight,
        TouchGesture::Swipe2Up,
        TouchGesture::Swipe2Down,
        TouchGesture::Swipe2Left,
        TouchGesture::Swipe2Right,
        TouchGesture::ZoomIn,
        TouchGesture::ZoomOut,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TouchGesture::Tap => "Tap",
            TouchGesture::TapLeft => "TapLeft",
            TouchGesture::TapCenter => "TapCenter",
            TouchGesture::TapRight => "TapRight",
            TouchGesture::MultiTap => "MultiTap",
            TouchGesture::SwipeUp => "SwipeUp",
            TouchGesture::SwipeDown => "SwipeDown",
            TouchGesture::SwipeLeft => "SwipeLeft",
            TouchGesture::SwipeRight => "SwipeRight",
            TouchGesture::Swipe2Up => "Swipe2Up",
            TouchGesture::Swipe2Down => "Swipe2Down",
            TouchGesture::Swipe2Left => "Swipe2Left",
            TouchGesture::Swipe2Right => "Swipe2Right",
            TouchGesture::ZoomIn => "ZoomIn",
            TouchGesture::ZoomOut => "ZoomOut",
        }
    }

    /// The swipe in a cardinal direction with one or two fingers.
    pub fn swipe(dir: Dir, two: bool) -> TouchGesture {
        use TouchGesture::*;
        match (dir, two) {
            (Dir::Up, false) => SwipeUp,
            (Dir::Down, false) => SwipeDown,
            (Dir::Left, false) => SwipeLeft,
            (Dir::Right, false) => SwipeRight,
            (Dir::Up, true) => Swipe2Up,
            (Dir::Down, true) => Swipe2Down,
            (Dir::Left, true) => Swipe2Left,
            (_, true) => Swipe2Right,
            (_, false) => SwipeRight,
        }
    }
}

/// One half of a touchpad split into two trackpads.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Half {
    Left,
    Right,
}

impl Half {
    pub fn name(self) -> &'static str {
        match self {
            Half::Left => "Left",
            Half::Right => "Right",
        }
    }
}

/// What one of the two trackpads of a split touchpad reports.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TrackpadPart {
    Region(TouchRegion),
    Tap,
    /// The pad click while this half is touched.
    Click,
}

/// Direction of the controller's motion: nose up or down, the steering axis (roll or yaw, see
/// the profile's `[gyro]`) to the left or right, or a lean (roll, left grip down = left).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GyroDir {
    Up,
    Down,
    Left,
    Right,
    LeanLeft,
    LeanRight,
}

impl GyroDir {
    pub const ALL: [GyroDir; 6] = [
        GyroDir::Up,
        GyroDir::Down,
        GyroDir::Left,
        GyroDir::Right,
        GyroDir::LeanLeft,
        GyroDir::LeanRight,
    ];

    pub fn name(self) -> &'static str {
        match self {
            GyroDir::Up => "Up",
            GyroDir::Down => "Down",
            GyroDir::Left => "Left",
            GyroDir::Right => "Right",
            GyroDir::LeanLeft => "LeanLeft",
            GyroDir::LeanRight => "LeanRight",
        }
    }

    fn parse(s: &str) -> Option<GyroDir> {
        GyroDir::ALL
            .into_iter()
            .find(|d| d.name().eq_ignore_ascii_case(s))
    }
}

/// Region of a touchpad, used as a digital input.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchRegion {
    Up,
    Down,
    Left,
    Right,
    Center,
    UpLeft,
    UpRight,
    DownLeft,
    DownRight,
}

impl TouchRegion {
    pub const ALL: [TouchRegion; 9] = [
        TouchRegion::Up,
        TouchRegion::Down,
        TouchRegion::Left,
        TouchRegion::Right,
        TouchRegion::Center,
        TouchRegion::UpLeft,
        TouchRegion::UpRight,
        TouchRegion::DownLeft,
        TouchRegion::DownRight,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TouchRegion::Up => "Up",
            TouchRegion::Down => "Down",
            TouchRegion::Left => "Left",
            TouchRegion::Right => "Right",
            TouchRegion::Center => "Center",
            TouchRegion::UpLeft => "UpLeft",
            TouchRegion::UpRight => "UpRight",
            TouchRegion::DownLeft => "DownLeft",
            TouchRegion::DownRight => "DownRight",
        }
    }

    /// The region in a direction from the centre.
    pub fn of(dir: Dir) -> TouchRegion {
        match dir {
            Dir::Up => TouchRegion::Up,
            Dir::Down => TouchRegion::Down,
            Dir::Left => TouchRegion::Left,
            Dir::Right => TouchRegion::Right,
            Dir::UpLeft => TouchRegion::UpLeft,
            Dir::UpRight => TouchRegion::UpRight,
            Dir::DownLeft => TouchRegion::DownLeft,
            Dir::DownRight => TouchRegion::DownRight,
        }
    }

    pub fn is_diagonal(self) -> bool {
        matches!(
            self,
            TouchRegion::UpLeft
                | TouchRegion::UpRight
                | TouchRegion::DownLeft
                | TouchRegion::DownRight
        )
    }
}

/// Anything that can trigger a mapping.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Control {
    Button(PadButton),
    StickDir(Stick, Dir),
    /// Stick deflected into a zone, in a direction or (`None`) in any direction.
    StickZone(Stick, Option<Dir>, Zone),
    TriggerPress(Trigger),
    TriggerZone(Trigger, Zone),
    TriggerFull(Trigger),
    Touch(TouchRegion),
    /// A touchpad gesture (a short press).
    Gesture(TouchGesture),
    /// One trackpad of a touchpad split into halves.
    Trackpad(Half, TrackpadPart),
    Gyro(GyroDir),
    /// Controller moved or tilted into a zone, in a direction or (`None`) in any direction.
    GyroZone(Option<GyroDir>, Zone),
    Key(Key),
    Mouse(MouseButton),
    Wheel(WheelDir),
    MouseMove(Dir),
}

impl Control {
    /// True for inputs that are only active for an instant (wheel notches), which
    /// therefore cannot be held.
    pub fn is_momentary(self) -> bool {
        matches!(self, Control::Wheel(_))
    }

    /// True if the control comes from a gamepad (as opposed to keyboard/mouse).
    pub fn is_pad(self) -> bool {
        !matches!(
            self,
            Control::Key(_) | Control::Mouse(_) | Control::Wheel(_) | Control::MouseMove(_)
        )
    }

    /// Every digital control derived from a stick: directions (with diagonals) and zones.
    pub fn of_stick(s: Stick) -> Vec<Control> {
        let dirs = Dir::ALL8.map(|d| Control::StickDir(s, d));
        let zones = Zone::ALL.into_iter().flat_map(move |z| {
            std::iter::once(None)
                .chain(Dir::ALL8.map(Some))
                .map(move |d| Control::StickZone(s, d, z))
        });
        dirs.into_iter().chain(zones).collect()
    }

    /// Every touchpad control: regions, gestures and the trackpads of a split pad.
    pub fn of_touchpad() -> Vec<Control> {
        let regions = TouchRegion::ALL.map(Control::Touch);
        let gestures = TouchGesture::ALL.map(Control::Gesture);
        let halves = [Half::Left, Half::Right].into_iter().flat_map(|h| {
            TouchRegion::ALL
                .map(TrackpadPart::Region)
                .into_iter()
                .chain([TrackpadPart::Tap, TrackpadPart::Click])
                .map(move |p| Control::Trackpad(h, p))
        });
        regions.into_iter().chain(gestures).chain(halves).collect()
    }

    /// Every gyro control: directions and zones.
    pub fn of_gyro() -> Vec<Control> {
        let dirs = GyroDir::ALL.map(Control::Gyro);
        let zones = Zone::ALL.into_iter().flat_map(|z| {
            std::iter::once(None)
                .chain(GyroDir::ALL.map(Some))
                .map(move |d| Control::GyroZone(d, z))
        });
        dirs.into_iter().chain(zones).collect()
    }

    /// Every digital control derived from a trigger: press, zones and full pull.
    pub fn of_trigger(t: Trigger) -> Vec<Control> {
        let zones = Zone::ALL.map(|z| Control::TriggerZone(t, z));
        [Control::TriggerPress(t), Control::TriggerFull(t)]
            .into_iter()
            .chain(zones)
            .collect()
    }
}

pub fn pad_button_name(b: PadButton) -> &'static str {
    match b {
        PadButton::A => "A",
        PadButton::B => "B",
        PadButton::X => "X",
        PadButton::Y => "Y",
        PadButton::LB => "LB",
        PadButton::RB => "RB",
        PadButton::Back => "Back",
        PadButton::Start => "Start",
        PadButton::LS => "LS",
        PadButton::RS => "RS",
        PadButton::DpadUp => "DpadUp",
        PadButton::DpadDown => "DpadDown",
        PadButton::DpadLeft => "DpadLeft",
        PadButton::DpadRight => "DpadRight",
        PadButton::Guide => "Guide",
        PadButton::Touchpad => "Touchpad",
        PadButton::Misc => "Misc",
        PadButton::PaddleUL => "PaddleUL",
        PadButton::PaddleUR => "PaddleUR",
        PadButton::PaddleLL => "PaddleLL",
        PadButton::PaddleLR => "PaddleLR",
        PadButton::Misc2 => "Misc2",
    }
}

/// Parse a gamepad button name, accepting common PlayStation/Nintendo aliases.
pub fn parse_pad_button(s: &str) -> Option<PadButton> {
    let alias = match s.to_ascii_lowercase().as_str() {
        "cross" => Some(PadButton::A),
        "circle" => Some(PadButton::B),
        "square" => Some(PadButton::X),
        "triangle" => Some(PadButton::Y),
        "l1" => Some(PadButton::LB),
        "r1" => Some(PadButton::RB),
        "l3" => Some(PadButton::LS),
        "r3" => Some(PadButton::RS),
        "view" | "select" | "share" | "create" | "minus" => Some(PadButton::Back),
        "menu" | "options" | "plus" => Some(PadButton::Start),
        "home" | "ps" | "xbox" => Some(PadButton::Guide),
        "up" => Some(PadButton::DpadUp),
        "down" => Some(PadButton::DpadDown),
        "left" => Some(PadButton::DpadLeft),
        "right" => Some(PadButton::DpadRight),
        "mute" | "capture" => Some(PadButton::Misc),
        "c" | "chat" => Some(PadButton::Misc2),
        "gl" => Some(PadButton::PaddleUL),
        "gr" => Some(PadButton::PaddleUR),
        _ => None,
    };
    alias.or_else(|| {
        PadButton::ALL
            .into_iter()
            .find(|b| pad_button_name(*b).eq_ignore_ascii_case(s))
    })
}

fn stick_name(s: Stick) -> &'static str {
    match s {
        Stick::Left => "Left",
        Stick::Right => "Right",
    }
}

fn trigger_name(t: Trigger) -> &'static str {
    match t {
        Trigger::Left => "LT",
        Trigger::Right => "RT",
    }
}

fn wheel_name(w: WheelDir) -> &'static str {
    match w {
        WheelDir::Up => "Up",
        WheelDir::Down => "Down",
        WheelDir::Left => "Left",
        WheelDir::Right => "Right",
    }
}

pub fn mouse_button_name(b: MouseButton) -> &'static str {
    match b {
        MouseButton::Left => "Left",
        MouseButton::Right => "Right",
        MouseButton::Middle => "Middle",
        MouseButton::X1 => "X1",
        MouseButton::X2 => "X2",
    }
}

pub fn parse_mouse_button(s: &str) -> Option<MouseButton> {
    MouseButton::ALL
        .into_iter()
        .find(|b| mouse_button_name(*b).eq_ignore_ascii_case(s))
        .or(match s.to_ascii_lowercase().as_str() {
            "back" | "4" => Some(MouseButton::X1),
            "forward" | "5" => Some(MouseButton::X2),
            "1" => Some(MouseButton::Left),
            "2" => Some(MouseButton::Right),
            "3" => Some(MouseButton::Middle),
            _ => None,
        })
}

pub fn parse_wheel(s: &str) -> Option<WheelDir> {
    [
        WheelDir::Up,
        WheelDir::Down,
        WheelDir::Left,
        WheelDir::Right,
    ]
    .into_iter()
    .find(|w| wheel_name(*w).eq_ignore_ascii_case(s))
}

impl fmt::Display for Control {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Control::Button(b) => f.write_str(pad_button_name(b)),
            Control::StickDir(s, d) => write!(f, "{}Stick{}", stick_name(s), d.name()),
            Control::StickZone(s, d, z) => write!(
                f,
                "{}Stick{}{}",
                stick_name(s),
                d.map_or("", Dir::name),
                z.name()
            ),
            Control::TriggerPress(t) => f.write_str(trigger_name(t)),
            Control::TriggerZone(t, z) => write!(f, "{}{}", trigger_name(t), z.name()),
            Control::TriggerFull(t) => write!(f, "{}Full", trigger_name(t)),
            Control::Touch(r) => write!(f, "Touch{}", r.name()),
            Control::Gesture(g) => write!(f, "Touch{}", g.name()),
            Control::Trackpad(h, p) => match p {
                TrackpadPart::Region(r) => write!(f, "Trackpad{}{}", h.name(), r.name()),
                TrackpadPart::Tap => write!(f, "Trackpad{}Tap", h.name()),
                TrackpadPart::Click => write!(f, "Trackpad{}Click", h.name()),
            },
            Control::Gyro(d) => write!(f, "Gyro{}", d.name()),
            Control::GyroZone(d, z) => write!(f, "Gyro{}{}", d.map_or("", GyroDir::name), z.name()),
            Control::Key(k) => write!(f, "Key:{}", k.name()),
            Control::Mouse(b) => write!(f, "Mouse:{}", mouse_button_name(b)),
            Control::Wheel(w) => write!(f, "Wheel:{}", wheel_name(w)),
            Control::MouseMove(d) => write!(f, "MouseMove:{}", d.name()),
        }
    }
}

impl FromStr for Control {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || format!("unknown control `{s}`");
        let s = s.trim();
        if let Some((prefix, rest)) = s.split_once(':') {
            return match prefix.to_ascii_lowercase().as_str() {
                "key" => rest.parse::<Key>().map(Control::Key),
                "mouse" => parse_mouse_button(rest).map(Control::Mouse).ok_or_else(err),
                "wheel" => parse_wheel(rest).map(Control::Wheel).ok_or_else(err),
                "mousemove" | "mouse_move" => {
                    Dir::parse(rest).map(Control::MouseMove).ok_or_else(err)
                }
                "pad" => s[4..].parse(),
                _ => Err(err()),
            };
        }
        let lower = s.to_ascii_lowercase();
        let trigger = |name: &str| match name {
            "lt" | "l2" | "zl" | "lefttrigger" => Some(Trigger::Left),
            "rt" | "r2" | "zr" | "righttrigger" => Some(Trigger::Right),
            _ => None,
        };
        if let Some(t) = trigger(&lower) {
            return Ok(Control::TriggerPress(t));
        }
        if let Some(t) = lower.strip_suffix("full").and_then(trigger) {
            return Ok(Control::TriggerFull(t));
        }
        if let Some((rest, z)) = Zone::strip_suffix(&lower)
            && let Some(t) = trigger(rest)
        {
            return Ok(Control::TriggerZone(t, z));
        }
        for (prefix, stick) in [("leftstick", Stick::Left), ("rightstick", Stick::Right)] {
            let Some(rest) = lower.strip_prefix(prefix) else {
                continue;
            };
            if let Some(d) = Dir::parse(rest) {
                return Ok(Control::StickDir(stick, d));
            }
            if let Some((rest, z)) = Zone::strip_suffix(rest) {
                if rest.is_empty() {
                    return Ok(Control::StickZone(stick, None, z));
                }
                if let Some(d) = Dir::parse(rest) {
                    return Ok(Control::StickZone(stick, Some(d), z));
                }
            }
        }
        let region = |rest: &str| {
            TouchRegion::ALL
                .into_iter()
                .find(|r| r.name().eq_ignore_ascii_case(rest))
        };
        if let Some(rest) = lower.strip_prefix("touch") {
            if let Some(r) = region(rest) {
                return Ok(Control::Touch(r));
            }
            if let Some(g) = TouchGesture::ALL
                .into_iter()
                .find(|g| g.name().eq_ignore_ascii_case(rest))
            {
                return Ok(Control::Gesture(g));
            }
        }
        if let Some(rest) = lower.strip_prefix("trackpad") {
            for half in [Half::Left, Half::Right] {
                let Some(part) = rest.strip_prefix(&half.name().to_ascii_lowercase()) else {
                    continue;
                };
                let part = match part {
                    "tap" => Some(TrackpadPart::Tap),
                    "click" => Some(TrackpadPart::Click),
                    other => region(other).map(TrackpadPart::Region),
                };
                if let Some(p) = part {
                    return Ok(Control::Trackpad(half, p));
                }
            }
        }
        if let Some(rest) = lower.strip_prefix("gyro") {
            if let Some(d) = GyroDir::parse(rest) {
                return Ok(Control::Gyro(d));
            }
            if let Some((rest, z)) = Zone::strip_suffix(rest) {
                if rest.is_empty() {
                    return Ok(Control::GyroZone(None, z));
                }
                if let Some(d) = GyroDir::parse(rest) {
                    return Ok(Control::GyroZone(Some(d), z));
                }
            }
        }
        parse_pad_button(s).map(Control::Button).ok_or_else(err)
    }
}

impl Serialize for Control {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Control {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_kinds() {
        let samples = [
            Control::Button(PadButton::A),
            Control::Button(PadButton::PaddleLR),
            Control::Button(PadButton::Misc2),
            Control::StickDir(Stick::Left, Dir::Up),
            Control::StickDir(Stick::Right, Dir::Left),
            Control::StickDir(Stick::Left, Dir::DownRight),
            Control::StickZone(Stick::Right, Some(Dir::UpLeft), Zone::Med),
            Control::Touch(TouchRegion::UpLeft),
            Control::MouseMove(Dir::DownLeft),
            Control::StickZone(Stick::Left, None, Zone::Low),
            Control::StickZone(Stick::Right, Some(Dir::Right), Zone::High),
            Control::StickZone(Stick::Left, Some(Dir::Up), Zone::Med),
            Control::TriggerPress(Trigger::Right),
            Control::TriggerZone(Trigger::Left, Zone::Med),
            Control::TriggerZone(Trigger::Right, Zone::High),
            Control::TriggerFull(Trigger::Left),
            Control::Touch(TouchRegion::Center),
            Control::Gesture(TouchGesture::TapLeft),
            Control::Gesture(TouchGesture::Swipe2Down),
            Control::Trackpad(Half::Left, TrackpadPart::Region(TouchRegion::UpRight)),
            Control::Trackpad(Half::Right, TrackpadPart::Tap),
            Control::Trackpad(Half::Right, TrackpadPart::Click),
            Control::Gyro(GyroDir::Down),
            Control::Gyro(GyroDir::LeanRight),
            Control::GyroZone(None, Zone::Low),
            Control::GyroZone(Some(GyroDir::LeanLeft), Zone::High),
            Control::Key(Key::Space),
            Control::Mouse(MouseButton::X2),
            Control::Wheel(WheelDir::Down),
            Control::MouseMove(Dir::Right),
        ];
        for c in samples {
            let s = c.to_string();
            assert_eq!(s.parse::<Control>().unwrap(), c, "{s}");
        }
    }

    #[test]
    fn aliases() {
        assert_eq!(
            "cross".parse::<Control>().unwrap(),
            Control::Button(PadButton::A)
        );
        assert_eq!(
            "R2".parse::<Control>().unwrap(),
            Control::TriggerPress(Trigger::Right)
        );
        assert_eq!("key:w".parse::<Control>().unwrap(), Control::Key(Key::W));
        assert_eq!(
            "pad:LB".parse::<Control>().unwrap(),
            Control::Button(PadButton::LB)
        );
        assert!("Bogus".parse::<Control>().is_err());
    }

    #[test]
    fn nintendo_aliases() {
        let parse = |s: &str| s.parse::<Control>().unwrap();
        assert_eq!(parse("ZL"), Control::TriggerPress(Trigger::Left));
        assert_eq!(parse("zrfull"), Control::TriggerFull(Trigger::Right));
        assert_eq!(parse("C"), Control::Button(PadButton::Misc2));
        assert_eq!(parse("GL"), Control::Button(PadButton::PaddleUL));
        assert_eq!(parse("gr"), Control::Button(PadButton::PaddleUR));
        assert_eq!(parse("capture"), Control::Button(PadButton::Misc));
    }

    #[test]
    fn zone_names() {
        let parse = |s: &str| s.parse::<Control>().unwrap();
        assert_eq!(
            Control::StickZone(Stick::Left, Some(Dir::Up), Zone::High).to_string(),
            "LeftStickUpHigh"
        );
        assert_eq!(
            Control::TriggerZone(Trigger::Right, Zone::Med).to_string(),
            "RTMed"
        );
        assert_eq!(
            parse("leftstickmedium"),
            Control::StickZone(Stick::Left, None, Zone::Med)
        );
        assert_eq!(
            parse("RightStickRightLow"),
            Control::StickZone(Stick::Right, Some(Dir::Right), Zone::Low)
        );
        assert_eq!(
            parse("R2High"),
            Control::TriggerZone(Trigger::Right, Zone::High)
        );
        assert_eq!(
            parse("ZLLow"),
            Control::TriggerZone(Trigger::Left, Zone::Low)
        );
        assert_eq!(
            parse("LeftTriggerFull"),
            Control::TriggerFull(Trigger::Left)
        );
        assert!("LeftStickHighUp".parse::<Control>().is_err());
        assert!("AHigh".parse::<Control>().is_err());
    }
}
