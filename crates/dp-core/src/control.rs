//! Names for every input a mapping can be triggered by ([`Control`]) and the
//! string syntax used for them in profiles.
//!
//! | Syntax                      | Meaning                                        |
//! |-----------------------------|------------------------------------------------|
//! | `A`, `LB`, `DpadUp`, ...    | gamepad button ([`PadButton`] name)            |
//! | `LeftStickUp`, ...          | stick pushed past its deadzone in a direction  |
//! | `LT`, `RT`                  | trigger pulled past its deadzone               |
//! | `LTFull`, `RTFull`          | trigger fully pulled                           |
//! | `TouchUp`, `TouchCenter`... | finger resting in a touchpad region            |
//! | `GyroUp`, ...               | controller tilted / turned in a direction      |
//! | `Key:Space`                 | keyboard key ([`Key`] name)                    |
//! | `Mouse:Left`                | mouse button                                   |
//! | `Wheel:Up`                  | one wheel notch (momentary)                    |
//! | `MouseMove:Up`              | physical mouse moving in a direction           |

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::keys::{Key, MouseButton, WheelDir};
use crate::pad::{PadButton, Stick, Trigger};

/// Four-way direction.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];

    pub fn name(self) -> &'static str {
        match self {
            Dir::Up => "Up",
            Dir::Down => "Down",
            Dir::Left => "Left",
            Dir::Right => "Right",
        }
    }

    fn parse(s: &str) -> Option<Dir> {
        Dir::ALL
            .into_iter()
            .find(|d| d.name().eq_ignore_ascii_case(s))
    }

    /// Unit vector (x right, y up).
    pub fn vector(self) -> (f32, f32) {
        match self {
            Dir::Up => (0.0, 1.0),
            Dir::Down => (0.0, -1.0),
            Dir::Left => (-1.0, 0.0),
            Dir::Right => (1.0, 0.0),
        }
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
}

impl TouchRegion {
    pub const ALL: [TouchRegion; 5] = [
        TouchRegion::Up,
        TouchRegion::Down,
        TouchRegion::Left,
        TouchRegion::Right,
        TouchRegion::Center,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TouchRegion::Up => "Up",
            TouchRegion::Down => "Down",
            TouchRegion::Left => "Left",
            TouchRegion::Right => "Right",
            TouchRegion::Center => "Center",
        }
    }
}

/// Anything that can trigger a mapping.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Control {
    Button(PadButton),
    StickDir(Stick, Dir),
    TriggerPress(Trigger),
    TriggerFull(Trigger),
    Touch(TouchRegion),
    Gyro(Dir),
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
            Control::TriggerPress(t) => f.write_str(trigger_name(t)),
            Control::TriggerFull(t) => write!(f, "{}Full", trigger_name(t)),
            Control::Touch(r) => write!(f, "Touch{}", r.name()),
            Control::Gyro(d) => write!(f, "Gyro{}", d.name()),
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
        match lower.as_str() {
            "lt" | "l2" | "zl" | "lefttrigger" => return Ok(Control::TriggerPress(Trigger::Left)),
            "rt" | "r2" | "zr" | "righttrigger" => {
                return Ok(Control::TriggerPress(Trigger::Right));
            }
            "ltfull" | "l2full" | "zlfull" => return Ok(Control::TriggerFull(Trigger::Left)),
            "rtfull" | "r2full" | "zrfull" => return Ok(Control::TriggerFull(Trigger::Right)),
            _ => {}
        }
        for (prefix, stick) in [("leftstick", Stick::Left), ("rightstick", Stick::Right)] {
            if let Some(d) = lower.strip_prefix(prefix).and_then(Dir::parse) {
                return Ok(Control::StickDir(stick, d));
            }
        }
        if let Some(rest) = lower.strip_prefix("touch")
            && let Some(r) = TouchRegion::ALL
                .into_iter()
                .find(|r| r.name().eq_ignore_ascii_case(rest))
        {
            return Ok(Control::Touch(r));
        }
        if let Some(d) = lower.strip_prefix("gyro").and_then(Dir::parse) {
            return Ok(Control::Gyro(d));
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
            Control::TriggerPress(Trigger::Right),
            Control::TriggerFull(Trigger::Left),
            Control::Touch(TouchRegion::Center),
            Control::Gyro(Dir::Down),
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
}
