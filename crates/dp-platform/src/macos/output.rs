//! Keyboard and mouse output through Quartz events (`CGEventPost`), which needs the
//! Accessibility permission. Without it macOS drops the events without a word, so a missing
//! permission is logged once.
//!
//! Every event carries [`INJECTED_TAG`] as its user data, so Doppelpad's own capture skips it.
//! Programs that read keyboard and mouse straight from the HID devices (the GameController
//! framework's GCKeyboard and GCMouse) do not see these events.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use dp_core::keys::{Key, MouseButton, WheelDir};
use objc2_core_foundation::{CFRetained, CGPoint};
use objc2_core_graphics::{
    CGDisplayBounds, CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID,
    CGEventTapLocation, CGEventType, CGMainDisplayID, CGMouseButton, CGScrollEventUnit,
    CGWarpMouseCursorPosition,
};

use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};

use super::keycodes::{self, flag};
use crate::KbmOutput;
use crate::remainder::{MotionRemainder, Remainder};

/// Marker in `kCGEventSourceUserData` identifying events posted by Doppelpad ("DPAD").
pub const INJECTED_TAG: i64 = 0x4450_4144;

/// Two clicks of a button this close in time and place make a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(500);
const DOUBLE_CLICK_DISTANCE: f64 = 4.0;

/// The last click, to count double and triple clicks.
struct Click {
    button: MouseButton,
    at: Instant,
    location: CGPoint,
    count: i64,
}

pub struct EventOutput {
    source: Option<CFRetained<CGEventSource>>,
    /// Modifier flags of the modifiers held down, Caps Lock included.
    flags: u64,
    keys: HashSet<Key>,
    buttons: HashSet<MouseButton>,
    click: Option<Click>,
    move_rem: MotionRemainder,
    wheel_rem: [Remainder; 4],
}

// SAFETY: the event source is a CoreFoundation object used by one thread at a time (`&mut
// self`); creating and posting events works from any thread.
unsafe impl Send for EventOutput {}

impl EventOutput {
    pub fn new() -> Self {
        if !super::permissions::granted() {
            tracing::warn!(
                "keyboard and mouse output needs the Accessibility permission (System Settings › \
                 Privacy & Security › Accessibility); until then macOS drops it"
            );
        }
        EventOutput {
            source: CGEventSource::new(CGEventSourceStateID::HIDSystemState),
            flags: 0,
            keys: HashSet::new(),
            buttons: HashSet::new(),
            click: None,
            move_rem: MotionRemainder::default(),
            wheel_rem: Default::default(),
        }
    }

    /// Tag an event as Doppelpad's and post it into the system's event stream.
    fn post(event: Option<CFRetained<CGEvent>>) {
        let Some(event) = event else {
            tracing::debug!("cannot create an input event");
            return;
        };
        CGEvent::set_integer_value_field(
            Some(&event),
            CGEventField::EventSourceUserData,
            INJECTED_TAG,
        );
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    }

    /// Where the cursor is now.
    fn cursor() -> CGPoint {
        CGEvent::new(None)
            .map(|e| CGEvent::location(Some(&e)))
            .unwrap_or(CGPoint { x: 0.0, y: 0.0 })
    }

    /// A mouse event of `kind` at `at`, for `button`.
    fn mouse(
        &self,
        kind: CGEventType,
        at: CGPoint,
        button: CGMouseButton,
    ) -> Option<CFRetained<CGEvent>> {
        let event = CGEvent::new_mouse_event(self.source.as_deref(), kind, at, button)?;
        CGEvent::set_flags(Some(&event), CGEventFlags(self.flags));
        Some(event)
    }

    /// The event type of a motion with the buttons held now: a drag while one is.
    fn motion_type(&self) -> (CGEventType, CGMouseButton) {
        if self.buttons.contains(&MouseButton::Left) {
            (CGEventType::LeftMouseDragged, CGMouseButton::Left)
        } else if self.buttons.contains(&MouseButton::Right) {
            (CGEventType::RightMouseDragged, CGMouseButton::Right)
        } else if !self.buttons.is_empty() {
            (CGEventType::OtherMouseDragged, CGMouseButton::Center)
        } else {
            (CGEventType::MouseMoved, CGMouseButton::Left)
        }
    }

    /// Count a press of `button` at `at` as the next click of a double or triple click, or
    /// as a first click.
    fn click_count(&mut self, button: MouseButton, at: CGPoint) -> i64 {
        let now = Instant::now();
        let count = match &self.click {
            Some(c)
                if c.button == button
                    && now.duration_since(c.at) < DOUBLE_CLICK
                    && (c.location.x - at.x).hypot(c.location.y - at.y) < DOUBLE_CLICK_DISTANCE =>
            {
                c.count + 1
            }
            _ => 1,
        };
        self.click = Some(Click {
            button,
            at: now,
            location: at,
            count,
        });
        count
    }
}

/// A media key (volume, mute, playback) as the system event the keyboard's own keys make.
pub fn media_event(kind: u32, down: bool) -> Option<CFRetained<CGEvent>> {
    let flags = NSEventModifierFlags(if down { 0xA00 } else { 0xB00 });
    let event = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
        NSEventType::SystemDefined,
        CGPoint { x: 0.0, y: 0.0 },
        flags,
        0.0,
        0,
        None,
        MEDIA_SUBTYPE,
        keycodes::media_data(kind, down) as isize,
        -1,
    )?;
    event.CGEvent().map(Into::into)
}

/// The subtype of system events that carry the media keys (`NX_SUBTYPE_AUX_CONTROL_BUTTONS`).
pub const MEDIA_SUBTYPE: i16 = 8;

impl KbmOutput for EventOutput {
    fn key(&mut self, key: Key, down: bool) {
        if let Some(kind) = keycodes::media_key(key) {
            Self::post(media_event(kind, down));
            return;
        }
        let Some(code) = keycodes::keycode(key) else {
            tracing::debug!(?key, "no such key on macOS");
            return;
        };
        let repeat = down && !self.keys.insert(key);
        if !down {
            self.keys.remove(&key);
        }
        let modifier = keycodes::modifier(key);
        if let Some(bits) = modifier {
            if down {
                self.flags |= bits;
            } else {
                self.flags &= !bits;
                // The mask stays while the other side's key is held.
                for k in &self.keys {
                    self.flags |= keycodes::modifier(*k).unwrap_or(0);
                }
            }
        } else if key == Key::CapsLock && down && !repeat {
            self.flags ^= flag::CAPS_LOCK;
        }
        let Some(event) = CGEvent::new_keyboard_event(self.source.as_deref(), code, down) else {
            return;
        };
        if modifier.is_some() || key == Key::CapsLock {
            // Modifier keys change the flags rather than type anything.
            CGEvent::set_type(Some(&event), CGEventType::FlagsChanged);
        }
        CGEvent::set_flags(
            Some(&event),
            CGEventFlags(self.flags | keycodes::key_flags(key)),
        );
        if repeat {
            CGEvent::set_integer_value_field(
                Some(&event),
                CGEventField::KeyboardEventAutorepeat,
                1,
            );
        }
        Self::post(Some(event));
    }

    fn mouse_button(&mut self, button: MouseButton, down: bool) {
        if down {
            self.buttons.insert(button);
        } else {
            self.buttons.remove(&button);
        }
        let (kind, cg_button, number) = match (button, down) {
            (MouseButton::Left, true) => (CGEventType::LeftMouseDown, CGMouseButton::Left, 0),
            (MouseButton::Left, false) => (CGEventType::LeftMouseUp, CGMouseButton::Left, 0),
            (MouseButton::Right, true) => (CGEventType::RightMouseDown, CGMouseButton::Right, 1),
            (MouseButton::Right, false) => (CGEventType::RightMouseUp, CGMouseButton::Right, 1),
            (b, true) => (
                CGEventType::OtherMouseDown,
                CGMouseButton::Center,
                keycodes::other_number(b),
            ),
            (b, false) => (
                CGEventType::OtherMouseUp,
                CGMouseButton::Center,
                keycodes::other_number(b),
            ),
        };
        let at = Self::cursor();
        let count = if down {
            self.click_count(button, at)
        } else {
            self.click.as_ref().map_or(1, |c| c.count)
        };
        let event = self.mouse(kind, at, cg_button);
        if let Some(e) = &event {
            CGEvent::set_integer_value_field(Some(e), CGEventField::MouseEventButtonNumber, number);
            CGEvent::set_integer_value_field(Some(e), CGEventField::MouseEventClickState, count);
        }
        Self::post(event);
    }

    fn mouse_move(&mut self, dx: f32, dy: f32) {
        let (x, y) = self.move_rem.take(dx, dy);
        if (x, y) == (0, 0) {
            return;
        }
        let from = Self::cursor();
        let to = CGPoint {
            x: from.x + f64::from(x),
            y: from.y + f64::from(y),
        };
        let (kind, button) = self.motion_type();
        let event = self.mouse(kind, to, button);
        if let Some(e) = &event {
            // Games that hold the cursor in place read the motion from these.
            CGEvent::set_integer_value_field(Some(e), CGEventField::MouseEventDeltaX, i64::from(x));
            CGEvent::set_integer_value_field(Some(e), CGEventField::MouseEventDeltaY, i64::from(y));
        }
        Self::post(event);
    }

    fn mouse_to(&mut self, x: f32, y: f32) {
        let screen = CGDisplayBounds(CGMainDisplayID());
        let to = CGPoint {
            x: screen.origin.x + f64::from(x.clamp(0.0, 1.0)) * screen.size.width,
            y: screen.origin.y + f64::from(y.clamp(0.0, 1.0)) * screen.size.height,
        };
        CGWarpMouseCursorPosition(to);
        // Warping moves the cursor without an event; programs learn of the move from this one.
        let (kind, button) = self.motion_type();
        Self::post(self.mouse(kind, to, button));
    }

    fn wheel(&mut self, dir: WheelDir, detents: f32) {
        let lines = self.wheel_rem[dir as usize].take(detents);
        if lines == 0 {
            return;
        }
        // Positive lines scroll up, and left on the second wheel.
        let (vertical, horizontal) = match dir {
            WheelDir::Up => (lines, 0),
            WheelDir::Down => (-lines, 0),
            WheelDir::Left => (0, lines),
            WheelDir::Right => (0, -lines),
        };
        let event = CGEvent::new_scroll_wheel_event2(
            self.source.as_deref(),
            CGScrollEventUnit::Line,
            2,
            vertical,
            horizontal,
            0,
        );
        if let Some(e) = &event {
            CGEvent::set_flags(Some(e), CGEventFlags(self.flags));
        }
        Self::post(event);
    }
}
