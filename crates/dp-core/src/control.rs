//! Names for every input a mapping can be triggered by ([`Control`]) and the
//! string syntax used for them in profiles.
//!
//! | Syntax                      | Meaning                                        |
//! |-----------------------------|------------------------------------------------|
//! | `A`, `LB`, `DpadUp`, ...    | gamepad button ([`PadButton`] name)            |
//! | `LeftStickUp`, ...          | stick pushed into its low zone in a direction  |
//! | `LeftStickLow`, ...         | stick deflected into a zone, any direction     |
//! | `LeftStickUpHigh`, ...      | stick deflected into a zone in a direction     |
//! | `LT`, `RT`                  | trigger pulled into its low zone               |
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
//! | `MouseMove:UpHigh`, `MouseMove:Low` | mouse moving at a zone's speed        |

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::keys::{Key, MouseButton, WheelDir};
use crate::names::named_enum;
use crate::pad::{PadButton, Stick, Trigger};

named_enum! {
/// Direction: the four cardinals, then the four diagonals for sources that tell them apart
/// (`ALL` is in this order).
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
}

impl Dir {
    /// The four cardinal directions.
    pub const CARDINALS: [Dir; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];
    pub const DIAGONALS: [Dir; 4] = [Dir::UpLeft, Dir::UpRight, Dir::DownLeft, Dir::DownRight];

    pub fn is_diagonal(self) -> bool {
        Dir::DIAGONALS.contains(&self)
    }

    /// The diagonal between a vertical and a horizontal cardinal.
    pub fn diagonal(vertical: Dir, horizontal: Dir) -> Dir {
        debug_assert!(
            matches!(vertical, Dir::Up | Dir::Down) && matches!(horizontal, Dir::Left | Dir::Right),
            "{vertical:?} {horizontal:?}"
        );
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

named_enum! {
/// Band of a stick's deflection or a trigger's pull. Bands are exclusive: a
/// control is in at most one zone at a time.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Zone {
    Low,
    Med,
    High,
}
}

impl Zone {
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

named_enum! {
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
}

impl TouchGesture {
    /// The swipe in a cardinal direction with one or two fingers.
    pub fn swipe(dir: Dir, two: bool) -> TouchGesture {
        use TouchGesture::*;
        debug_assert!(!dir.is_diagonal(), "{dir:?}");
        match (dir, two) {
            (Dir::Up, false) => SwipeUp,
            (Dir::Down, false) => SwipeDown,
            (Dir::Left, false) => SwipeLeft,
            (Dir::Up, true) => Swipe2Up,
            (Dir::Down, true) => Swipe2Down,
            (Dir::Left, true) => Swipe2Left,
            (_, true) => Swipe2Right,
            (_, false) => SwipeRight,
        }
    }
}

named_enum! {
/// One half of a touchpad split into two trackpads.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Half {
    Left,
    Right,
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

named_enum! {
/// Direction of the controller's motion: nose up or down, the steering axis (roll or yaw, see
/// the profile's `[gyro]`) to the left or right, or a lean (roll, left grip down = left).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GyroDir {
    Up,
    Down,
    Left,
    Right,
    LeanLeft,
    LeanRight,
}
}

named_enum! {
/// Region of a touchpad, used as a digital input.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
}

impl TouchRegion {
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

/// The kind of device a [`Control`] comes from.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InputDevice {
    Pad,
    Keyboard,
    Mouse,
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
    /// Physical mouse moving at a zone's speed, in a direction or (`None`) any.
    MouseZone(Option<Dir>, Zone),
}

impl Control {
    /// True for inputs that are only active for an instant (wheel notches), which
    /// therefore cannot be held.
    pub fn is_momentary(self) -> bool {
        matches!(self, Control::Wheel(_))
    }

    /// The device the control comes from.
    pub fn device(self) -> InputDevice {
        match self {
            Control::Key(_) => InputDevice::Keyboard,
            Control::Mouse(_)
            | Control::Wheel(_)
            | Control::MouseMove(_)
            | Control::MouseZone(..) => InputDevice::Mouse,
            Control::Button(_)
            | Control::StickDir(..)
            | Control::StickZone(..)
            | Control::TriggerPress(_)
            | Control::TriggerZone(..)
            | Control::TriggerFull(_)
            | Control::Touch(_)
            | Control::Gesture(_)
            | Control::Trackpad(..)
            | Control::Gyro(_)
            | Control::GyroZone(..) => InputDevice::Pad,
        }
    }

    /// True if the control comes from a gamepad.
    pub fn is_pad(self) -> bool {
        self.device() == InputDevice::Pad
    }

    /// True if the control comes from the keyboard or the mouse.
    pub fn is_kbm(self) -> bool {
        !self.is_pad()
    }

    /// Every digital control derived from a stick: directions (with diagonals) and zones.
    pub fn of_stick(s: Stick) -> impl Iterator<Item = Control> {
        let dirs = Dir::ALL.map(|d| Control::StickDir(s, d));
        let zones = Zone::ALL.into_iter().flat_map(move |z| {
            std::iter::once(None)
                .chain(Dir::ALL.map(Some))
                .map(move |d| Control::StickZone(s, d, z))
        });
        dirs.into_iter().chain(zones)
    }

    /// Every touchpad control: regions, gestures and the trackpads of a split pad.
    pub fn of_touchpad() -> impl Iterator<Item = Control> {
        let regions = TouchRegion::ALL.map(Control::Touch);
        let gestures = TouchGesture::ALL.map(Control::Gesture);
        let halves = Half::ALL.into_iter().flat_map(|h| {
            TouchRegion::ALL
                .map(TrackpadPart::Region)
                .into_iter()
                .chain([TrackpadPart::Tap, TrackpadPart::Click])
                .map(move |p| Control::Trackpad(h, p))
        });
        regions.into_iter().chain(gestures).chain(halves)
    }

    /// Every gyro control: directions and zones.
    pub fn of_gyro() -> impl Iterator<Item = Control> {
        let dirs = GyroDir::ALL.map(Control::Gyro);
        let zones = Zone::ALL.into_iter().flat_map(|z| {
            std::iter::once(None)
                .chain(GyroDir::ALL.map(Some))
                .map(move |d| Control::GyroZone(d, z))
        });
        dirs.into_iter().chain(zones)
    }

    /// Every digital control derived from a trigger: press, zones and full pull.
    pub fn of_trigger(t: Trigger) -> impl Iterator<Item = Control> {
        let zones = Zone::ALL.map(|z| Control::TriggerZone(t, z));
        [Control::TriggerPress(t), Control::TriggerFull(t)]
            .into_iter()
            .chain(zones)
    }
}

/// Parse a gamepad button name, accepting common PlayStation/Nintendo aliases.
fn parse_pad_button(s: &str) -> Option<PadButton> {
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
    alias.or_else(|| PadButton::from_name(s))
}

/// A direction alone (`Up`) or a zone in any or one direction (`Low`, `UpHigh`).
enum DirZone<D> {
    Dir(D),
    Zone(Option<D>, Zone),
}

/// Lowercase `s` as a direction or a zone, `dir` reading the direction names.
fn dir_or_zone<D>(s: &str, dir: impl Fn(&str) -> Option<D>) -> Option<DirZone<D>> {
    if let Some(d) = dir(s) {
        return Some(DirZone::Dir(d));
    }
    let (rest, z) = Zone::strip_suffix(s)?;
    if rest.is_empty() {
        return Some(DirZone::Zone(None, z));
    }
    dir(rest).map(|d| DirZone::Zone(Some(d), z))
}

fn trigger_name(t: Trigger) -> &'static str {
    match t {
        Trigger::Left => "LT",
        Trigger::Right => "RT",
    }
}

fn parse_mouse_button(s: &str) -> Option<MouseButton> {
    MouseButton::from_name(s).or(match s.to_ascii_lowercase().as_str() {
        "back" | "4" => Some(MouseButton::X1),
        "forward" | "5" => Some(MouseButton::X2),
        "1" => Some(MouseButton::Left),
        "2" => Some(MouseButton::Right),
        "3" => Some(MouseButton::Middle),
        _ => None,
    })
}

impl fmt::Display for Control {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Control::Button(b) => f.write_str(b.name()),
            Control::StickDir(s, d) => write!(f, "{}Stick{}", s.name(), d.name()),
            Control::StickZone(s, d, z) => write!(
                f,
                "{}Stick{}{}",
                s.name(),
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
            Control::Mouse(b) => write!(f, "Mouse:{}", b.name()),
            Control::Wheel(w) => write!(f, "Wheel:{}", w.name()),
            Control::MouseMove(d) => write!(f, "MouseMove:{}", d.name()),
            Control::MouseZone(d, z) => {
                write!(f, "MouseMove:{}{}", d.map_or("", Dir::name), z.name())
            }
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
                "wheel" => WheelDir::from_name(rest)
                    .map(Control::Wheel)
                    .ok_or_else(err),
                "mousemove" | "mouse_move" => {
                    match dir_or_zone(&rest.to_ascii_lowercase(), Dir::from_name) {
                        Some(DirZone::Dir(d)) => Ok(Control::MouseMove(d)),
                        Some(DirZone::Zone(d, z)) => Ok(Control::MouseZone(d, z)),
                        None => Err(err()),
                    }
                }
                "pad" => rest.parse(),
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
            match dir_or_zone(rest, Dir::from_name) {
                Some(DirZone::Dir(d)) => return Ok(Control::StickDir(stick, d)),
                Some(DirZone::Zone(d, z)) => return Ok(Control::StickZone(stick, d, z)),
                None => {}
            }
        }
        if let Some(rest) = lower.strip_prefix("touch") {
            if let Some(r) = TouchRegion::from_name(rest) {
                return Ok(Control::Touch(r));
            }
            if let Some(g) = TouchGesture::from_name(rest) {
                return Ok(Control::Gesture(g));
            }
        }
        if let Some(rest) = lower.strip_prefix("trackpad") {
            for half in Half::ALL {
                let Some(part) = rest.strip_prefix(&half.name().to_ascii_lowercase()) else {
                    continue;
                };
                let part = match part {
                    "tap" => Some(TrackpadPart::Tap),
                    "click" => Some(TrackpadPart::Click),
                    other => TouchRegion::from_name(other).map(TrackpadPart::Region),
                };
                if let Some(p) = part {
                    return Ok(Control::Trackpad(half, p));
                }
            }
        }
        if let Some(rest) = lower.strip_prefix("gyro") {
            match dir_or_zone(rest, GyroDir::from_name) {
                Some(DirZone::Dir(d)) => return Ok(Control::Gyro(d)),
                Some(DirZone::Zone(d, z)) => return Ok(Control::GyroZone(d, z)),
                None => {}
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

    /// `ALL` is in declaration order: `PadButton::bit`, the engine's `d[dir as usize]` and the
    /// importer's `GyroDir::ALL[i]` count on it.
    #[test]
    fn all_is_in_declaration_order() {
        for (i, b) in PadButton::ALL.into_iter().enumerate() {
            assert_eq!(b as usize, i, "{b:?}");
        }
        for (i, d) in Dir::ALL.into_iter().enumerate() {
            assert_eq!(d as usize, i, "{d:?}");
        }
        for (i, d) in GyroDir::ALL.into_iter().enumerate() {
            assert_eq!(d as usize, i, "{d:?}");
        }
        assert_eq!(Dir::ALL[..4], Dir::CARDINALS);
        assert_eq!(PadButton::from_name("dpadup"), Some(PadButton::DpadUp));
        assert_eq!(PadButton::Misc2.name(), "Misc2");
    }

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
            Control::MouseZone(None, Zone::High),
            Control::MouseZone(Some(Dir::Left), Zone::Med),
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
