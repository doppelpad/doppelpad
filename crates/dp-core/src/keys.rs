//! Platform-neutral keyboard keys and mouse buttons.
//!
//! Each key carries its PC/AT "set 1" scan code (extended keys as `0xE0xx`) and its
//! Linux evdev code. DirectInput `DIK_*` codes are set-1 scan codes with the E0 prefix
//! folded into bit 7, so they convert losslessly via [`Key::from_dik`].

use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! keys {
    ($( $(#[$m:meta])* $v:ident = ($sc:expr, $ev:expr) ),+ $(,)?) => {
        /// A physical keyboard key, identified by position (US layout names).
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Key { $( $(#[$m])* $v ),+ }

        impl Key {
            pub const ALL: &'static [Key] = &[ $( Key::$v ),+ ];

            /// Canonical name, as used in profiles.
            pub const fn name(self) -> &'static str {
                match self { $( Key::$v => stringify!($v) ),+ }
            }

            /// Set-1 scan code; extended keys are `0xE0xx`.
            pub const fn scancode(self) -> u16 {
                match self { $( Key::$v => $sc ),+ }
            }

            /// Linux `KEY_*` code from `input-event-codes.h`.
            pub const fn evdev(self) -> u16 {
                match self { $( Key::$v => $ev ),+ }
            }
        }
    };
}

keys! {
    Escape = (0x01, 1),
    Digit1 = (0x02, 2), Digit2 = (0x03, 3), Digit3 = (0x04, 4), Digit4 = (0x05, 5),
    Digit5 = (0x06, 6), Digit6 = (0x07, 7), Digit7 = (0x08, 8), Digit8 = (0x09, 9),
    Digit9 = (0x0A, 10), Digit0 = (0x0B, 11),
    Minus = (0x0C, 12), Equal = (0x0D, 13), Backspace = (0x0E, 14), Tab = (0x0F, 15),
    Q = (0x10, 16), W = (0x11, 17), E = (0x12, 18), R = (0x13, 19), T = (0x14, 20),
    Y = (0x15, 21), U = (0x16, 22), I = (0x17, 23), O = (0x18, 24), P = (0x19, 25),
    LeftBracket = (0x1A, 26), RightBracket = (0x1B, 27), Enter = (0x1C, 28), LeftCtrl = (0x1D, 29),
    A = (0x1E, 30), S = (0x1F, 31), D = (0x20, 32), F = (0x21, 33), G = (0x22, 34),
    H = (0x23, 35), J = (0x24, 36), K = (0x25, 37), L = (0x26, 38),
    Semicolon = (0x27, 39), Apostrophe = (0x28, 40), Grave = (0x29, 41), LeftShift = (0x2A, 42),
    Backslash = (0x2B, 43),
    Z = (0x2C, 44), X = (0x2D, 45), C = (0x2E, 46), V = (0x2F, 47), B = (0x30, 48),
    N = (0x31, 49), M = (0x32, 50),
    Comma = (0x33, 51), Period = (0x34, 52), Slash = (0x35, 53), RightShift = (0x36, 54),
    NumpadMultiply = (0x37, 55), LeftAlt = (0x38, 56), Space = (0x39, 57), CapsLock = (0x3A, 58),
    F1 = (0x3B, 59), F2 = (0x3C, 60), F3 = (0x3D, 61), F4 = (0x3E, 62), F5 = (0x3F, 63),
    F6 = (0x40, 64), F7 = (0x41, 65), F8 = (0x42, 66), F9 = (0x43, 67), F10 = (0x44, 68),
    NumLock = (0x45, 69), ScrollLock = (0x46, 70),
    Numpad7 = (0x47, 71), Numpad8 = (0x48, 72), Numpad9 = (0x49, 73), NumpadSubtract = (0x4A, 74),
    Numpad4 = (0x4B, 75), Numpad5 = (0x4C, 76), Numpad6 = (0x4D, 77), NumpadAdd = (0x4E, 78),
    Numpad1 = (0x4F, 79), Numpad2 = (0x50, 80), Numpad3 = (0x51, 81), Numpad0 = (0x52, 82),
    NumpadDecimal = (0x53, 83),
    /// The extra key left of Z on ISO keyboards (`<>|`).
    IntlBackslash = (0x56, 86),
    F11 = (0x57, 87), F12 = (0x58, 88),
    F13 = (0x64, 183), F14 = (0x65, 184), F15 = (0x66, 185), F16 = (0x67, 186),
    F17 = (0x68, 187), F18 = (0x69, 188), F19 = (0x6A, 189), F20 = (0x6B, 190),
    F21 = (0x6C, 191), F22 = (0x6D, 192), F23 = (0x6E, 193), F24 = (0x76, 194),
    Kana = (0x70, 93), IntlRo = (0x73, 89), Convert = (0x79, 92), NonConvert = (0x7B, 94),
    IntlYen = (0x7D, 124), NumpadEqual = (0x0059, 117), NumpadComma = (0x7E, 121),
    MediaPrevious = (0xE010, 165), MediaNext = (0xE019, 163),
    NumpadEnter = (0xE01C, 96), RightCtrl = (0xE01D, 97),
    VolumeMute = (0xE020, 113), LaunchCalculator = (0xE021, 140), MediaPlayPause = (0xE022, 164),
    MediaStop = (0xE024, 166), VolumeDown = (0xE02E, 114), VolumeUp = (0xE030, 115),
    BrowserHome = (0xE032, 172),
    NumpadDivide = (0xE035, 98), PrintScreen = (0xE037, 99), RightAlt = (0xE038, 100),
    /// Pause/Break. Its real scan sequence is `E1 1D 45`; backends special-case it.
    Pause = (0xE045, 119),
    Home = (0xE047, 102), Up = (0xE048, 103), PageUp = (0xE049, 104),
    Left = (0xE04B, 105), Right = (0xE04D, 106),
    End = (0xE04F, 107), Down = (0xE050, 108), PageDown = (0xE051, 109),
    Insert = (0xE052, 110), Delete = (0xE053, 111),
    LeftMeta = (0xE05B, 125), RightMeta = (0xE05C, 126), ContextMenu = (0xE05D, 127),
    Power = (0xE05E, 116), Sleep = (0xE05F, 142), Wake = (0xE063, 143),
    BrowserSearch = (0xE065, 217), BrowserFavorites = (0xE066, 156), BrowserRefresh = (0xE067, 173),
    BrowserStop = (0xE068, 128), BrowserForward = (0xE069, 159), BrowserBack = (0xE06A, 158),
    LaunchMyComputer = (0xE06B, 157), LaunchMail = (0xE06C, 155), LaunchMediaSelect = (0xE06D, 226),
}

impl Key {
    /// Look up a key by its canonical name, case-insensitively. A few common
    /// aliases (`Ctrl`, `Shift`, `Alt`, `Win`, `Esc`, `Return`, digits `0`-`9`) are accepted.
    pub fn from_name(s: &str) -> Option<Key> {
        let alias = match s.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Some(Key::LeftCtrl),
            "shift" => Some(Key::LeftShift),
            "alt" => Some(Key::LeftAlt),
            "win" | "meta" | "super" | "cmd" => Some(Key::LeftMeta),
            "esc" => Some(Key::Escape),
            "return" => Some(Key::Enter),
            "del" => Some(Key::Delete),
            "ins" => Some(Key::Insert),
            "pgup" => Some(Key::PageUp),
            "pgdn" => Some(Key::PageDown),
            "menu" | "apps" => Some(Key::ContextMenu),
            d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => {
                let n = d.as_bytes()[0] - b'0';
                Some(if n == 0 {
                    Key::Digit0
                } else {
                    Key::ALL[Key::Digit1 as usize + n as usize - 1]
                })
            }
            _ => None,
        };
        alias.or_else(|| {
            Key::ALL
                .iter()
                .copied()
                .find(|k| k.name().eq_ignore_ascii_case(s))
        })
    }

    /// Look up a key by set-1 scan code (`0xE0xx` for extended keys).
    pub fn from_scancode(sc: u16) -> Option<Key> {
        Key::ALL.iter().copied().find(|k| k.scancode() == sc)
    }

    /// Look up a key by Linux evdev code.
    pub fn from_evdev(code: u16) -> Option<Key> {
        Key::ALL.iter().copied().find(|k| k.evdev() == code)
    }

    /// Convert a DirectInput `DIK_*` code.
    ///
    /// DirectInput swaps NumLock and Pause relative to the E0 convention, and
    /// uses `0x8D` for the numpad equals key; everything else is `scan | 0x80` for E0 keys.
    pub fn from_dik(dik: u16) -> Option<Key> {
        match dik {
            0x45 => Some(Key::NumLock),
            0xC5 => Some(Key::Pause),
            0x8D => Some(Key::NumpadEqual),
            0x94 => Some(Key::Kana),
            d if d & 0x80 != 0 => Key::from_scancode(0xE000 | (d & 0x7F)),
            d => Key::from_scancode(d),
        }
    }

    /// Whether the scan code needs the E0 (extended) prefix.
    pub const fn is_extended(self) -> bool {
        self.scancode() & 0xFF00 == 0xE000
    }

    pub const fn is_modifier(self) -> bool {
        matches!(
            self,
            Key::LeftCtrl
                | Key::RightCtrl
                | Key::LeftShift
                | Key::RightShift
                | Key::LeftAlt
                | Key::RightAlt
                | Key::LeftMeta
                | Key::RightMeta
        )
    }
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl std::str::FromStr for Key {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Key::from_name(s).ok_or_else(|| format!("unknown key `{s}`"))
    }
}

impl Serialize for Key {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Mouse buttons.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    /// "Back" side button (XButton1).
    X1,
    /// "Forward" side button (XButton2).
    X2,
}

impl MouseButton {
    pub const ALL: [MouseButton; 5] = [
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::X1,
        MouseButton::X2,
    ];
}

/// Mouse wheel directions, usable as digital inputs or outputs.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WheelDir {
    Up,
    Down,
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_roundtrip() {
        for &k in Key::ALL {
            assert_eq!(Key::from_name(k.name()), Some(k), "{k}");
        }
    }

    #[test]
    fn scancodes_unique() {
        for (i, a) in Key::ALL.iter().enumerate() {
            for b in &Key::ALL[i + 1..] {
                assert_ne!(a.scancode(), b.scancode(), "{a} vs {b}");
                assert_ne!(a.evdev(), b.evdev(), "{a} vs {b}");
            }
        }
    }

    #[test]
    fn dik_conversion() {
        assert_eq!(Key::from_dik(0x1C), Some(Key::Enter));
        assert_eq!(Key::from_dik(0x01), Some(Key::Escape));
        assert_eq!(Key::from_dik(0x39), Some(Key::Space));
        assert_eq!(Key::from_dik(0xC8), Some(Key::Up));
        assert_eq!(Key::from_dik(0xD0), Some(Key::Down));
        assert_eq!(Key::from_dik(0xCB), Some(Key::Left));
        assert_eq!(Key::from_dik(0xCD), Some(Key::Right));
        assert_eq!(Key::from_dik(0x9D), Some(Key::RightCtrl));
        assert_eq!(Key::from_dik(0xDB), Some(Key::LeftMeta));
        assert_eq!(Key::from_dik(0xC5), Some(Key::Pause));
        assert_eq!(Key::from_dik(0x45), Some(Key::NumLock));
    }

    #[test]
    fn aliases() {
        assert_eq!("ctrl".parse::<Key>().unwrap(), Key::LeftCtrl);
        assert_eq!("5".parse::<Key>().unwrap(), Key::Digit5);
        assert_eq!("0".parse::<Key>().unwrap(), Key::Digit0);
        assert_eq!("space".parse::<Key>().unwrap(), Key::Space);
    }
}
