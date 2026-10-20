//! Keyboard / mouse input events, as delivered by platform capture backends.

use crate::keys::{Key, MouseButton, WheelDir};

/// Keyboard / mouse event captured from the user's physical devices.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum KbmEvent {
    Key {
        key: Key,
        down: bool,
    },
    MouseButton {
        button: MouseButton,
        down: bool,
    },
    /// Raw relative motion (counts / mickeys); positive y is down.
    MouseMove {
        dx: i32,
        dy: i32,
    },
    Wheel {
        dir: WheelDir,
        detents: f32,
    },
}
