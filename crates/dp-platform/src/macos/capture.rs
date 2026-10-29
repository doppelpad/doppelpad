//! Global keyboard/mouse capture through a Quartz event tap, media keys included.
//!
//! The tap sits where hardware events enter the window server and runs on a thread of its own.
//! It sees every key, button, wheel and motion and can swallow the ones the active profile
//! remaps. An active tap needs the Accessibility permission.
//!
//! Limitations: a tap cannot tell keyboards apart; programs that read the HID devices
//! themselves (the GameController framework) still see swallowed input; while a password field
//! has secure input on, macOS keeps keys from every tap. Swallowed mouse motion is kept off the
//! cursor by detaching the cursor from the mouse while motion is remapped.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};
use dp_core::keys::{Key, MouseButton, WheelDir};
use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGAssociateMouseAndMouseCursorPosition, CGEvent, CGEventField, CGEventMask, CGEventTapLocation,
    CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType,
};

use objc2_app_kit::NSEvent;

use super::keycodes;
use super::output::{INJECTED_TAG, MEDIA_SUBTYPE};
use crate::{CaptureFilter, KbmCapture, KbmEvent, PlatformError, Result};

/// What the tap callback needs; lives on the tap thread's heap while the tap does.
struct TapCtx {
    tx: Sender<KbmEvent>,
    filter: Arc<CaptureFilter>,
    /// The tap, to turn it back on when macOS turns it off.
    port: Option<CFRetained<CFMachPort>>,
    /// The cursor is detached from the mouse (motion is swallowed).
    detached: bool,
}

/// The run loop of the tap thread, to stop it from another thread.
struct LoopRef(CFRetained<CFRunLoop>);

// SAFETY: CFRunLoopStop may be called from any thread; nothing else is done with it there.
unsafe impl Send for LoopRef {}

#[derive(Default)]
pub struct TapCapture {
    thread: Option<JoinHandle<()>>,
    run_loop: Option<LoopRef>,
}

/// The system events that carry the media keys (`NX_SYSDEFINED`), which CoreGraphics has no
/// name for.
const SYSTEM_DEFINED: CGEventType = CGEventType(14);

/// The event types the tap listens to.
fn mask() -> CGEventMask {
    [
        SYSTEM_DEFINED,
        CGEventType::KeyDown,
        CGEventType::KeyUp,
        CGEventType::FlagsChanged,
        CGEventType::LeftMouseDown,
        CGEventType::LeftMouseUp,
        CGEventType::RightMouseDown,
        CGEventType::RightMouseUp,
        CGEventType::OtherMouseDown,
        CGEventType::OtherMouseUp,
        CGEventType::MouseMoved,
        CGEventType::LeftMouseDragged,
        CGEventType::RightMouseDragged,
        CGEventType::OtherMouseDragged,
        CGEventType::ScrollWheel,
    ]
    .iter()
    .fold(0, |m, t| m | (1 << t.0))
}

/// The button of an "other" mouse event's button number.
fn other_button(number: i64) -> Option<MouseButton> {
    match number {
        2 => Some(MouseButton::Middle),
        3 => Some(MouseButton::X1),
        4 => Some(MouseButton::X2),
        _ => None,
    }
}

/// Wheel motion as events, in lines (fractions from trackpads included); positive is up and
/// left.
fn wheel_events(vertical: f64, horizontal: f64) -> impl Iterator<Item = KbmEvent> {
    let up = (vertical != 0.0).then(|| KbmEvent::Wheel {
        dir: if vertical > 0.0 {
            WheelDir::Up
        } else {
            WheelDir::Down
        },
        detents: vertical.abs() as f32,
    });
    let side = (horizontal != 0.0).then(|| KbmEvent::Wheel {
        dir: if horizontal > 0.0 {
            WheelDir::Left
        } else {
            WheelDir::Right
        },
        detents: horizontal.abs() as f32,
    });
    up.into_iter().chain(side)
}

/// The keyboard or mouse events an event tap event stands for.
fn events_of(kind: CGEventType, event: &CGEvent) -> Vec<KbmEvent> {
    let field = |f| CGEvent::integer_value_field(Some(event), f);
    let button = |button, down| vec![KbmEvent::MouseButton { button, down }];
    match kind {
        CGEventType::KeyDown | CGEventType::KeyUp => {
            let code = field(CGEventField::KeyboardEventKeycode) as u16;
            match keycodes::key(code) {
                Some(key) => vec![KbmEvent::Key {
                    key,
                    down: kind == CGEventType::KeyDown,
                }],
                None => Vec::new(),
            }
        }
        CGEventType::FlagsChanged => {
            let code = field(CGEventField::KeyboardEventKeycode) as u16;
            let flags = CGEvent::flags(Some(event)).0;
            match keycodes::key(code) {
                // Caps Lock reports each press once: a press and a release.
                Some(Key::CapsLock) => vec![
                    KbmEvent::Key {
                        key: Key::CapsLock,
                        down: true,
                    },
                    KbmEvent::Key {
                        key: Key::CapsLock,
                        down: false,
                    },
                ],
                Some(key) => match keycodes::modifier_down(key, flags) {
                    Some(down) => vec![KbmEvent::Key { key, down }],
                    None => Vec::new(),
                },
                None => Vec::new(),
            }
        }
        CGEventType::LeftMouseDown => button(MouseButton::Left, true),
        CGEventType::LeftMouseUp => button(MouseButton::Left, false),
        CGEventType::RightMouseDown => button(MouseButton::Right, true),
        CGEventType::RightMouseUp => button(MouseButton::Right, false),
        CGEventType::OtherMouseDown | CGEventType::OtherMouseUp => {
            match other_button(field(CGEventField::MouseEventButtonNumber)) {
                Some(b) => button(b, kind == CGEventType::OtherMouseDown),
                None => Vec::new(),
            }
        }
        CGEventType::MouseMoved
        | CGEventType::LeftMouseDragged
        | CGEventType::RightMouseDragged
        | CGEventType::OtherMouseDragged => {
            let (dx, dy) = (
                field(CGEventField::MouseEventDeltaX),
                field(CGEventField::MouseEventDeltaY),
            );
            if (dx, dy) == (0, 0) {
                Vec::new()
            } else {
                vec![KbmEvent::MouseMove {
                    dx: dx as i32,
                    dy: dy as i32,
                }]
            }
        }
        SYSTEM_DEFINED => {
            let media = NSEvent::eventWithCGEvent(event)
                .filter(|e| e.subtype().0 == MEDIA_SUBTYPE)
                .and_then(|e| keycodes::media_of_data(e.data1() as i64));
            match media.and_then(|(kind, down)| Some((keycodes::media_key_of(kind)?, down))) {
                Some((key, down)) => vec![KbmEvent::Key { key, down }],
                None => Vec::new(),
            }
        }
        CGEventType::ScrollWheel => {
            let lines = |f| CGEvent::double_value_field(Some(event), f);
            wheel_events(
                lines(CGEventField::ScrollWheelEventFixedPtDeltaAxis1),
                lines(CGEventField::ScrollWheelEventFixedPtDeltaAxis2),
            )
            .collect()
        }
        _ => Vec::new(),
    }
}

unsafe extern "C-unwind" fn tap_callback(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    info: *mut c_void,
) -> *mut CGEvent {
    // SAFETY: `info` is the tap thread's context, which outlives the tap; the callback runs on
    // that thread.
    let ctx = unsafe { &mut *info.cast::<TapCtx>() };
    // SAFETY: macOS hands over a valid event for the duration of the callback.
    let ev = unsafe { event.as_ref() };
    if kind == CGEventType::TapDisabledByTimeout || kind == CGEventType::TapDisabledByUserInput {
        // A callback that took too long, or the user's secure input: take the tap back.
        if let Some(port) = &ctx.port {
            CGEvent::tap_enable(port, true);
        }
        return event.as_ptr();
    }
    let field = |f| CGEvent::integer_value_field(Some(ev), f);
    if field(CGEventField::EventSourceUserData) == INJECTED_TAG {
        return event.as_ptr(); // our own output
    }
    // Hardware events come from no process; another program's events name it.
    let injected = field(CGEventField::EventSourceUnixProcessID) != 0;
    if !ctx.filter.accepts(injected) {
        return event.as_ptr();
    }
    let detach = ctx.filter.suppress_mouse_move();
    if detach != ctx.detached {
        CGAssociateMouseAndMouseCursorPosition(!detach);
        ctx.detached = detach;
    }
    let mut swallow = false;
    for e in events_of(kind, ev) {
        let _ = ctx.tx.try_send(e);
        swallow |= ctx.filter.suppresses(&e);
    }
    if swallow {
        std::ptr::null_mut()
    } else {
        event.as_ptr()
    }
}

/// The tap thread: make the tap, report whether that worked, run until stopped.
fn tap_thread(
    tx: Sender<KbmEvent>,
    filter: Arc<CaptureFilter>,
    ready: &Sender<std::result::Result<LoopRef, String>>,
) {
    let ctx = Ctx::new(TapCtx {
        tx,
        filter,
        port: None,
        detached: false,
    });
    // SAFETY: the callback matches, and the context stays valid until the tap is invalidated
    // below (`ctx` is dropped after that).
    let port = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::HIDEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            mask(),
            Some(tap_callback),
            ctx.0.as_ptr().cast(),
        )
    };
    let Some(port) = port else {
        let _ = ready.send(Err(
            "macOS refused the keyboard/mouse event tap: Doppelpad needs the Accessibility \
             permission (System Settings › Privacy & Security › Accessibility)"
                .into(),
        ));
        return;
    };
    let source = CFMachPort::new_run_loop_source(None, Some(&port), 0);
    let Some(run_loop) = CFRunLoop::current() else {
        port.invalidate();
        let _ = ready.send(Err("no run loop for the event tap".into()));
        return;
    };
    // SAFETY: the common modes are a constant CoreFoundation provides.
    run_loop.add_source(source.as_deref(), unsafe { kCFRunLoopCommonModes });
    CGEvent::tap_enable(&port, true);
    // SAFETY: the callback runs only inside the run loop below, on this thread.
    unsafe { (*ctx.0.as_ptr()).port = Some(port.clone()) };
    let _ = ready.send(Ok(LoopRef(run_loop)));
    CFRunLoop::run();

    CGEvent::tap_enable(&port, false);
    port.invalidate();
    // SAFETY: the tap is invalidated: no callback runs any more.
    if unsafe { (*ctx.0.as_ptr()).detached } {
        CGAssociateMouseAndMouseCursorPosition(true);
    }
}

/// The tap's context, owned through this pointer alone: the tap's callback reaches it through
/// the same pointer while the run loop runs, so no `Box` or `&mut` may stand for it meanwhile.
/// Freed on drop, which the tap thread does only once its tap is invalidated (or was never
/// added to a run loop).
struct Ctx(NonNull<TapCtx>);

impl Ctx {
    fn new(ctx: TapCtx) -> Ctx {
        Ctx(NonNull::from(Box::leak(Box::new(ctx))))
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        // SAFETY: made from a `Box` in `Ctx::new`, and no callback reaches it any more.
        drop(unsafe { Box::from_raw(self.0.as_ptr()) });
    }
}

impl KbmCapture for TapCapture {
    fn start(&mut self, filter: Arc<CaptureFilter>) -> Result<Receiver<KbmEvent>> {
        self.stop();
        let (tx, rx) = crossbeam_channel::bounded(4096);
        let (ready_tx, ready_rx) = crossbeam_channel::bounded(1);
        let thread = std::thread::Builder::new()
            .name("dp-input-tap".into())
            .spawn(move || tap_thread(tx, filter, &ready_tx))
            .map_err(|e| PlatformError::Other(e.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(run_loop)) => {
                self.run_loop = Some(run_loop);
                self.thread = Some(thread);
                Ok(rx)
            }
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(PlatformError::Other(e))
            }
            Err(_) => Err(PlatformError::Other("event tap thread exited".into())),
        }
    }

    fn stop(&mut self) {
        if let Some(l) = self.run_loop.take() {
            l.0.stop();
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for TapCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_lines_become_detents() {
        let events: Vec<KbmEvent> = wheel_events(-1.5, 0.25).collect();
        assert_eq!(
            events,
            [
                KbmEvent::Wheel {
                    dir: WheelDir::Down,
                    detents: 1.5
                },
                KbmEvent::Wheel {
                    dir: WheelDir::Left,
                    detents: 0.25
                }
            ]
        );
        assert_eq!(wheel_events(0.0, 0.0).count(), 0);
    }

    #[test]
    fn other_buttons() {
        assert_eq!(other_button(2), Some(MouseButton::Middle));
        assert_eq!(other_button(4), Some(MouseButton::X2));
        assert_eq!(other_button(7), None);
    }

    #[test]
    fn media_keys_read_back_as_keys() {
        let event = super::super::output::media_event(16, true).unwrap();
        assert_eq!(
            events_of(SYSTEM_DEFINED, &event),
            [KbmEvent::Key {
                key: Key::MediaPlayPause,
                down: true
            }]
        );
        let event = super::super::output::media_event(1, false).unwrap();
        assert_eq!(
            events_of(SYSTEM_DEFINED, &event),
            [KbmEvent::Key {
                key: Key::VolumeDown,
                down: false
            }]
        );
    }

    #[test]
    fn the_mask_covers_keys_buttons_motion_and_wheel() {
        let m = mask();
        for t in [
            CGEventType::KeyDown,
            CGEventType::FlagsChanged,
            CGEventType::ScrollWheel,
        ] {
            assert_ne!(m & (1 << t.0), 0);
        }
        assert_eq!(m & (1 << CGEventType::TabletPointer.0), 0);
    }
}
