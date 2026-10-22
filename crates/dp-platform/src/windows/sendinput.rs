//! Keyboard and mouse output via `SendInput`.
//!
//! Keys are sent as scan codes (what DirectInput/raw-input games read), not virtual keys.
//! Every injected event is tagged with [`INJECTED_TAG`] so our own capture hooks skip it.

use dp_core::keys::{Key, MouseButton, WheelDir};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
    MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, SendInput, VK_NUMLOCK, VK_PAUSE,
};

use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ClipCursor, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN, WHEEL_DELTA,
};

use crate::KbmOutput;

/// Marker in `dwExtraInfo` identifying events injected by Doppelpad ("DPAD").
pub const INJECTED_TAG: usize = 0x4450_4144;

#[derive(Default)]
pub struct SendInputOutput {
    move_rem: (f32, f32),
    wheel_rem: [f32; 4],
}

fn send(inputs: &[INPUT]) {
    if inputs.is_empty() {
        return;
    }
    // SAFETY: slice of fully initialized INPUT structs.
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
    if sent as usize != inputs.len() {
        tracing::trace!("SendInput blocked (UIPI? target window may be elevated)");
    }
}

fn keyboard(vk: u16, scan: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: INJECTED_TAG,
            },
        },
    }
}

fn mouse(dx: i32, dy: i32, data: i32, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: INJECTED_TAG,
            },
        },
    }
}

impl KbmOutput for SendInputOutput {
    fn key(&mut self, key: Key, down: bool) {
        let up = if down { 0 } else { KEYEVENTF_KEYUP };
        let input = match key {
            // Pause and NumLock have irregular scan sequences; virtual keys are reliable.
            Key::Pause => keyboard(VK_PAUSE, 0, up),
            Key::NumLock => keyboard(VK_NUMLOCK, 0, up | KEYEVENTF_EXTENDEDKEY),
            _ => {
                let ext = if key.is_extended() {
                    KEYEVENTF_EXTENDEDKEY
                } else {
                    0
                };
                keyboard(0, key.scancode() & 0xFF, KEYEVENTF_SCANCODE | ext | up)
            }
        };
        send(&[input]);
    }

    fn mouse_button(&mut self, button: MouseButton, down: bool) {
        let (flags, data) = match (button, down) {
            (MouseButton::Left, true) => (MOUSEEVENTF_LEFTDOWN, 0),
            (MouseButton::Left, false) => (MOUSEEVENTF_LEFTUP, 0),
            (MouseButton::Right, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
            (MouseButton::Right, false) => (MOUSEEVENTF_RIGHTUP, 0),
            (MouseButton::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
            (MouseButton::Middle, false) => (MOUSEEVENTF_MIDDLEUP, 0),
            (MouseButton::X1, true) => (MOUSEEVENTF_XDOWN, 1),
            (MouseButton::X1, false) => (MOUSEEVENTF_XUP, 1),
            (MouseButton::X2, true) => (MOUSEEVENTF_XDOWN, 2),
            (MouseButton::X2, false) => (MOUSEEVENTF_XUP, 2),
        };
        send(&[mouse(0, 0, data, flags)]);
    }

    fn mouse_move(&mut self, dx: f32, dy: f32) {
        self.move_rem.0 += dx;
        self.move_rem.1 += dy;
        let (ix, iy) = (self.move_rem.0.trunc(), self.move_rem.1.trunc());
        if ix == 0.0 && iy == 0.0 {
            return;
        }
        self.move_rem.0 -= ix;
        self.move_rem.1 -= iy;
        send(&[mouse(ix as i32, iy as i32, 0, MOUSEEVENTF_MOVE)]);
    }

    fn mouse_to(&mut self, x: f32, y: f32) {
        // Absolute coordinates span the primary screen as 0–65535.
        let to = |v: f32| (v.clamp(0.0, 1.0) * 65535.0).round() as i32;
        send(&[mouse(
            to(x),
            to(y),
            0,
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
        )]);
    }

    fn clip(&mut self, rect: Option<[f32; 4]>) {
        // SAFETY: plain Win32 calls; the RECT outlives the call.
        unsafe {
            match rect {
                None => {
                    ClipCursor(std::ptr::null());
                }
                Some([x1, y1, x2, y2]) => {
                    let (w, h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
                    let px = |v: f32, size: i32| (v.clamp(0.0, 1.0) * size as f32).round() as i32;
                    let r = RECT {
                        left: px(x1.min(x2), w),
                        top: px(y1.min(y2), h),
                        right: px(x1.max(x2), w).max(px(x1.min(x2), w) + 1),
                        bottom: px(y1.max(y2), h).max(px(y1.min(y2), h) + 1),
                    };
                    ClipCursor(&r);
                }
            }
        }
    }

    fn wheel(&mut self, dir: WheelDir, detents: f32) {
        let rem = &mut self.wheel_rem[dir as usize];
        *rem += detents * WHEEL_DELTA as f32;
        let amount = rem.trunc();
        if amount == 0.0 {
            return;
        }
        *rem -= amount;
        let amount = amount as i32;
        let input = match dir {
            WheelDir::Up => mouse(0, 0, amount, MOUSEEVENTF_WHEEL),
            WheelDir::Down => mouse(0, 0, -amount, MOUSEEVENTF_WHEEL),
            WheelDir::Right => mouse(0, 0, amount, MOUSEEVENTF_HWHEEL),
            WheelDir::Left => mouse(0, 0, -amount, MOUSEEVENTF_HWHEEL),
        };
        send(&[input]);
    }
}
