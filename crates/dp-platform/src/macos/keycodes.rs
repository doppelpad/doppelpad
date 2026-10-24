//! Keys as macOS numbers them: virtual key codes (`kVK_*` in Carbon's `Events.h`), which name
//! a position on the ANSI keyboard, the modifier flags of keyboard events, and the media keys,
//! which are system events of their own (`NX_KEYTYPE_*` in IOKit's `ev_keymap.h`).

use dp_core::keys::Key;

/// The virtual key code of a key; `None` for keys a Mac keyboard does not have (Print Screen,
/// Scroll Lock, Pause, the launch and browser keys) or that are not plain keys on macOS (the
/// media and volume keys: see [`media_key`]).
pub fn keycode(key: Key) -> Option<u16> {
    use Key::*;
    Some(match key {
        A => 0x00,
        S => 0x01,
        D => 0x02,
        F => 0x03,
        H => 0x04,
        G => 0x05,
        Z => 0x06,
        X => 0x07,
        C => 0x08,
        V => 0x09,
        IntlBackslash => 0x0A,
        B => 0x0B,
        Q => 0x0C,
        W => 0x0D,
        E => 0x0E,
        R => 0x0F,
        Y => 0x10,
        T => 0x11,
        Digit1 => 0x12,
        Digit2 => 0x13,
        Digit3 => 0x14,
        Digit4 => 0x15,
        Digit6 => 0x16,
        Digit5 => 0x17,
        Equal => 0x18,
        Digit9 => 0x19,
        Digit7 => 0x1A,
        Minus => 0x1B,
        Digit8 => 0x1C,
        Digit0 => 0x1D,
        RightBracket => 0x1E,
        O => 0x1F,
        U => 0x20,
        LeftBracket => 0x21,
        I => 0x22,
        P => 0x23,
        Enter => 0x24,
        L => 0x25,
        J => 0x26,
        Apostrophe => 0x27,
        K => 0x28,
        Semicolon => 0x29,
        Backslash => 0x2A,
        Comma => 0x2B,
        Slash => 0x2C,
        N => 0x2D,
        M => 0x2E,
        Period => 0x2F,
        Tab => 0x30,
        Space => 0x31,
        Grave => 0x32,
        Backspace => 0x33,
        Escape => 0x35,
        RightMeta => 0x36,
        LeftMeta => 0x37,
        LeftShift => 0x38,
        CapsLock => 0x39,
        LeftAlt => 0x3A,
        LeftCtrl => 0x3B,
        RightShift => 0x3C,
        RightAlt => 0x3D,
        RightCtrl => 0x3E,
        F17 => 0x40,
        NumpadDecimal => 0x41,
        NumpadMultiply => 0x43,
        NumpadAdd => 0x45,
        // The Mac keypad's Clear key sits where Num Lock is.
        NumLock => 0x47,
        NumpadDivide => 0x4B,
        NumpadEnter => 0x4C,
        NumpadSubtract => 0x4E,
        F18 => 0x4F,
        F19 => 0x50,
        NumpadEqual => 0x51,
        Numpad0 => 0x52,
        Numpad1 => 0x53,
        Numpad2 => 0x54,
        Numpad3 => 0x55,
        Numpad4 => 0x56,
        Numpad5 => 0x57,
        Numpad6 => 0x58,
        Numpad7 => 0x59,
        F20 => 0x5A,
        Numpad8 => 0x5B,
        Numpad9 => 0x5C,
        IntlYen => 0x5D,
        IntlRo => 0x5E,
        NumpadComma => 0x5F,
        F5 => 0x60,
        F6 => 0x61,
        F7 => 0x62,
        F3 => 0x63,
        F8 => 0x64,
        F9 => 0x65,
        NonConvert => 0x66,
        F11 => 0x67,
        Kana => 0x68,
        F13 => 0x69,
        F16 => 0x6A,
        F14 => 0x6B,
        F10 => 0x6D,
        ContextMenu => 0x6E,
        F12 => 0x6F,
        F15 => 0x71,
        // Help, where PC keyboards have Insert.
        Insert => 0x72,
        Home => 0x73,
        PageUp => 0x74,
        Delete => 0x75,
        F4 => 0x76,
        End => 0x77,
        F2 => 0x78,
        PageDown => 0x79,
        F1 => 0x7A,
        Left => 0x7B,
        Right => 0x7C,
        Down => 0x7D,
        Up => 0x7E,
        _ => return None,
    })
}

/// The key of a virtual key code.
pub fn key(code: u16) -> Option<Key> {
    Key::ALL.iter().copied().find(|&k| keycode(k) == Some(code))
}

/// The media key type of a key (`NX_KEYTYPE_*`): volume, mute and the playback keys.
pub fn media_key(key: Key) -> Option<u32> {
    Some(match key {
        Key::VolumeUp => 0,
        Key::VolumeDown => 1,
        Key::VolumeMute => 7,
        Key::MediaPlayPause => 16,
        Key::MediaNext => 17,
        Key::MediaPrevious => 18,
        _ => return None,
    })
}

/// The key of a media key type.
pub fn media_key_of(kind: u32) -> Option<Key> {
    Key::ALL
        .iter()
        .copied()
        .find(|&k| media_key(k) == Some(kind))
}

/// The `data1` of a media key's system event: its type, and whether it went down.
pub fn media_data(kind: u32, down: bool) -> i64 {
    i64::from(kind) << 16 | if down { 0xA00 } else { 0xB00 }
}

/// The media key type and whether it went down, from a system event's `data1`; `None` for
/// other values.
pub fn media_of_data(data: i64) -> Option<(u32, bool)> {
    let kind = u32::try_from((data >> 16) & 0xFFFF).ok()?;
    match (data >> 8) & 0xFF {
        0x0A => Some((kind, true)),
        0x0B => Some((kind, false)),
        _ => None,
    }
}

/// Modifier flags of keyboard events (`CGEventFlags`): the side-independent mask, and the
/// device bit that says which side.
pub mod flag {
    pub const CAPS_LOCK: u64 = 0x0001_0000;
    pub const SHIFT: u64 = 0x0002_0000;
    pub const CONTROL: u64 = 0x0004_0000;
    pub const OPTION: u64 = 0x0008_0000;
    pub const COMMAND: u64 = 0x0010_0000;
    pub const NUMERIC_PAD: u64 = 0x0020_0000;
    pub const FUNCTION: u64 = 0x0080_0000;

    pub const LEFT_CONTROL: u64 = 0x0000_0001;
    pub const LEFT_SHIFT: u64 = 0x0000_0002;
    pub const RIGHT_SHIFT: u64 = 0x0000_0004;
    pub const LEFT_COMMAND: u64 = 0x0000_0008;
    pub const RIGHT_COMMAND: u64 = 0x0000_0010;
    pub const LEFT_OPTION: u64 = 0x0000_0020;
    pub const RIGHT_OPTION: u64 = 0x0000_0040;
    pub const RIGHT_CONTROL: u64 = 0x0000_2000;
}

/// The flags a held modifier key sets (its mask and its side's device bit); `None` for other
/// keys. Caps Lock is a modifier too, but a toggle: see [`flag::CAPS_LOCK`].
pub fn modifier(key: Key) -> Option<u64> {
    use flag::*;
    Some(match key {
        Key::LeftShift => SHIFT | LEFT_SHIFT,
        Key::RightShift => SHIFT | RIGHT_SHIFT,
        Key::LeftCtrl => CONTROL | LEFT_CONTROL,
        Key::RightCtrl => CONTROL | RIGHT_CONTROL,
        Key::LeftAlt => OPTION | LEFT_OPTION,
        Key::RightAlt => OPTION | RIGHT_OPTION,
        Key::LeftMeta => COMMAND | LEFT_COMMAND,
        Key::RightMeta => COMMAND | RIGHT_COMMAND,
        _ => return None,
    })
}

/// The flags a Mac keyboard puts on a key's own events: the keypad flag on the keypad and the
/// arrows, the function flag on the arrows, the navigation keys and the F keys.
pub fn key_flags(key: Key) -> u64 {
    use Key::*;
    use flag::*;
    match key {
        Up | Down | Left | Right => NUMERIC_PAD | FUNCTION,
        Home | End | PageUp | PageDown | Delete | Insert | F1 | F2 | F3 | F4 | F5 | F6 | F7
        | F8 | F9 | F10 | F11 | F12 | F13 | F14 | F15 | F16 | F17 | F18 | F19 | F20 => FUNCTION,
        Numpad0 | Numpad1 | Numpad2 | Numpad3 | Numpad4 | Numpad5 | Numpad6 | Numpad7 | Numpad8
        | Numpad9 | NumpadDecimal | NumpadMultiply | NumpadAdd | NumpadDivide | NumpadEnter
        | NumpadSubtract | NumpadEqual | NumpadComma | NumLock => NUMERIC_PAD,
        _ => 0,
    }
}

/// Whether a modifier `key` is down according to the flags of its flags-changed event; `None`
/// for keys that are not modifiers.
pub fn modifier_down(key: Key, flags: u64) -> Option<bool> {
    let bits = modifier(key)?;
    // The side's device bit decides; the mask alone stays set while the other side is held.
    Some(flags & bits & !(flag::SHIFT | flag::CONTROL | flag::OPTION | flag::COMMAND) != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_round_trip() {
        let mapped: Vec<(Key, u16)> = Key::ALL
            .iter()
            .filter_map(|&k| keycode(k).map(|c| (k, c)))
            .collect();
        for &(k, c) in &mapped {
            assert_eq!(key(c), Some(k), "{k:?} -> {c:#x} -> back");
        }
        let mut codes: Vec<u16> = mapped.iter().map(|&(_, c)| c).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), mapped.len(), "two keys share a code");
        assert!(mapped.len() > 100, "the whole keyboard is there");
    }

    #[test]
    fn positions_of_the_ansi_keyboard() {
        assert_eq!(keycode(Key::A), Some(0x00));
        assert_eq!(keycode(Key::Q), Some(0x0C));
        assert_eq!(keycode(Key::Escape), Some(0x35));
        assert_eq!(keycode(Key::LeftMeta), Some(0x37), "Command");
        assert_eq!(keycode(Key::LeftAlt), Some(0x3A), "Option");
        assert_eq!(keycode(Key::Up), Some(0x7E));
        assert_eq!(keycode(Key::PrintScreen), None);
        assert_eq!(keycode(Key::MediaPlayPause), None);
        assert_eq!(key(0xFF), None);
    }

    #[test]
    fn media_keys() {
        assert_eq!(media_key(Key::MediaPlayPause), Some(16));
        assert_eq!(media_key(Key::A), None);
        assert_eq!(media_key_of(7), Some(Key::VolumeMute));
        assert_eq!(media_data(16, true), 0x10_0A00);
        assert_eq!(media_of_data(0x10_0A00), Some((16, true)));
        assert_eq!(media_of_data(media_data(1, false)), Some((1, false)));
        assert_eq!(media_of_data(0x10_0000), None);
    }

    #[test]
    fn modifiers_and_their_sides() {
        assert_eq!(modifier(Key::A), None);
        let both_shifts = modifier(Key::LeftShift).unwrap() | modifier(Key::RightShift).unwrap();
        assert_eq!(modifier_down(Key::LeftShift, both_shifts), Some(true));
        // Right Shift released while the left one stays down: the mask stays set.
        let left_only = modifier(Key::LeftShift).unwrap();
        assert_eq!(modifier_down(Key::RightShift, left_only), Some(false));
        assert_eq!(modifier_down(Key::LeftShift, left_only), Some(true));
        assert_eq!(modifier_down(Key::LeftCtrl, 0), Some(false));
        assert_eq!(modifier_down(Key::Space, left_only), None);
    }

    #[test]
    fn keys_carry_the_flags_of_a_mac_keyboard() {
        assert_eq!(key_flags(Key::Left), flag::NUMERIC_PAD | flag::FUNCTION);
        assert_eq!(key_flags(Key::F5), flag::FUNCTION);
        assert_eq!(key_flags(Key::Numpad5), flag::NUMERIC_PAD);
        assert_eq!(key_flags(Key::W), 0);
    }
}
