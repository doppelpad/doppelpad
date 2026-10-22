//! Global keyboard/mouse capture.
//!
//! * `WH_KEYBOARD_LL` / `WH_MOUSE_LL` hooks observe every key and button and can
//!   swallow the ones the active profile remaps.
//! * Raw input (`WM_INPUT`) on a message-only window supplies unaccelerated mouse deltas
//!   for mouse-to-stick mappings.
//!
//! Limitation: hooks cannot tell which keyboard/mouse an event came from, and games
//! reading raw input directly still see swallowed mouse motion. Per-device interception
//! needs a filter driver (e.g. Interception) and is out of scope for this backend.

use std::ptr::{null, null_mut};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};
use dp_core::keys::{Key, MouseButton, WheelDir};
use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HID_USAGE_GENERIC_MOUSE, HID_USAGE_PAGE_GENERIC,
};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_NUMLOCK, VK_PAUSE};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, MOUSE_MOVE_ABSOLUTE, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER,
    RID_INPUT, RIDEV_INPUTSINK, RIM_TYPEMOUSE, RegisterRawInputDevices,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    HC_ACTION, HWND_MESSAGE, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, LLMHF_INJECTED, MSG,
    MSLLHOOKSTRUCT, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL,
    WH_MOUSE_LL, WHEEL_DELTA, WM_INPUT, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use super::register_class;
use super::sendinput::INJECTED_TAG;
use crate::{CaptureFilter, KbmCapture, KbmEvent, PlatformError, Result};

struct HookCtx {
    tx: Sender<KbmEvent>,
    filter: Arc<CaptureFilter>,
}

/// Hook procedures are plain functions, so their context is global. Only one capture
/// can be active per process.
static CTX: RwLock<Option<HookCtx>> = RwLock::new(None);

#[derive(Default)]
pub struct HookCapture {
    thread: Option<JoinHandle<()>>,
    thread_id: u32,
}

fn key_from_hook(k: &KBDLLHOOKSTRUCT) -> Option<Key> {
    match k.vkCode as u16 {
        VK_PAUSE => Some(Key::Pause),
        VK_NUMLOCK => Some(Key::NumLock),
        _ => {
            let ext = if k.flags & LLKHF_EXTENDED != 0 {
                0xE000
            } else {
                0
            };
            Key::from_scancode(ext | (k.scanCode as u16 & 0xFF))
        }
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to a KBDLLHOOKSTRUCT.
        let k = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let injected = k.flags & LLKHF_INJECTED != 0;
        let ours = injected && k.dwExtraInfo == INJECTED_TAG;
        if !ours && let Some(key) = key_from_hook(k) {
            let down = match wparam as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => Some(true),
                WM_KEYUP | WM_SYSKEYUP => Some(false),
                _ => None,
            };
            if let (Some(down), Ok(guard)) = (down, CTX.read())
                && let Some(ctx) = guard.as_ref()
                && ctx.filter.accepts(injected)
            {
                let _ = ctx.tx.try_send(KbmEvent::Key { key, down });
                if ctx.filter.key(key) {
                    return 1;
                }
            }
        }
    }
    // SAFETY: forwarding to the next hook is always valid.
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to an MSLLHOOKSTRUCT.
        let m = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        let injected = m.flags & LLMHF_INJECTED != 0;
        let ours = injected && m.dwExtraInfo == INJECTED_TAG;
        if !ours
            && let Ok(guard) = CTX.read()
            && let Some(ctx) = guard.as_ref()
            && ctx.filter.accepts(injected)
        {
            let hi = (m.mouseData >> 16) as u16;
            let event = match wparam as u32 {
                WM_LBUTTONDOWN => Some(KbmEvent::MouseButton {
                    button: MouseButton::Left,
                    down: true,
                }),
                WM_LBUTTONUP => Some(KbmEvent::MouseButton {
                    button: MouseButton::Left,
                    down: false,
                }),
                WM_RBUTTONDOWN => Some(KbmEvent::MouseButton {
                    button: MouseButton::Right,
                    down: true,
                }),
                WM_RBUTTONUP => Some(KbmEvent::MouseButton {
                    button: MouseButton::Right,
                    down: false,
                }),
                WM_MBUTTONDOWN => Some(KbmEvent::MouseButton {
                    button: MouseButton::Middle,
                    down: true,
                }),
                WM_MBUTTONUP => Some(KbmEvent::MouseButton {
                    button: MouseButton::Middle,
                    down: false,
                }),
                w @ (WM_XBUTTONDOWN | WM_XBUTTONUP) => Some(KbmEvent::MouseButton {
                    button: if hi == 1 {
                        MouseButton::X1
                    } else {
                        MouseButton::X2
                    },
                    down: w == WM_XBUTTONDOWN,
                }),
                WM_MOUSEWHEEL => {
                    let d = f32::from(hi as i16) / WHEEL_DELTA as f32;
                    Some(KbmEvent::Wheel {
                        dir: if d >= 0.0 {
                            WheelDir::Up
                        } else {
                            WheelDir::Down
                        },
                        detents: d.abs(),
                    })
                }
                WM_MOUSEHWHEEL => {
                    let d = f32::from(hi as i16) / WHEEL_DELTA as f32;
                    Some(KbmEvent::Wheel {
                        dir: if d >= 0.0 {
                            WheelDir::Right
                        } else {
                            WheelDir::Left
                        },
                        detents: d.abs(),
                    })
                }
                // Motion is reported from raw input; here we only decide whether to block the cursor.
                WM_MOUSEMOVE => {
                    if ctx.filter.suppress_mouse_move() {
                        return 1;
                    }
                    None
                }
                _ => None,
            };
            if let Some(ev) = event {
                let _ = ctx.tx.try_send(ev);
                let block = match ev {
                    KbmEvent::MouseButton { button, .. } => ctx.filter.mouse_button(button),
                    KbmEvent::Wheel { dir, .. } => ctx.filter.wheel(dir),
                    _ => false,
                };
                if block {
                    return 1;
                }
            }
        }
    }
    // SAFETY: forwarding to the next hook is always valid.
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn raw_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_INPUT {
        let mut raw = RAWINPUT::default();
        let mut size = size_of::<RAWINPUT>() as u32;
        // SAFETY: buffer is a RAWINPUT, large enough for mouse input.
        let n = unsafe {
            GetRawInputData(
                lparam as HRAWINPUT,
                RID_INPUT,
                (&mut raw as *mut RAWINPUT).cast(),
                &mut size,
                size_of::<RAWINPUTHEADER>() as u32,
            )
        };
        // hDevice == 0 means synthesized (SendInput), which includes our own output.
        if n != u32::MAX && raw.header.dwType == RIM_TYPEMOUSE && !raw.header.hDevice.is_null() {
            // SAFETY: dwType says the union holds mouse data.
            let m = unsafe { raw.data.mouse };
            if m.usFlags & MOUSE_MOVE_ABSOLUTE == 0
                && (m.lLastX != 0 || m.lLastY != 0)
                && let Ok(guard) = CTX.read()
                && let Some(ctx) = guard.as_ref()
            {
                let _ = ctx.tx.try_send(KbmEvent::MouseMove {
                    dx: m.lLastX,
                    dy: m.lLastY,
                });
            }
        }
    }
    // SAFETY: default handling for everything else.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// The hook thread: install the hooks and the raw-input window, report its thread id (or
/// the error), then pump messages until WM_QUIT.
fn hook_thread(ready: &Sender<std::result::Result<u32, String>>) {
    // SAFETY: returns this program's module handle.
    let hinst = unsafe { GetModuleHandleW(null()) };
    // SAFETY: hook procedures of this module, for all threads.
    let kb = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hinst, 0) };
    // SAFETY: as above.
    let ms = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hinst, 0) };
    if kb.is_null() || ms.is_null() {
        let err = std::io::Error::last_os_error();
        for hook in [kb, ms] {
            if !hook.is_null() {
                // SAFETY: a hook installed above.
                unsafe { UnhookWindowsHookEx(hook) };
            }
        }
        *CTX.write().unwrap() = None;
        let _ = ready.send(Err(format!("SetWindowsHookEx failed: {err}")));
        return;
    }

    let class = register_class("doppelpad-rawinput", raw_wndproc);
    // SAFETY: a registered class; a message-only window of this thread.
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            hinst,
            null(),
        )
    };
    if !hwnd.is_null() {
        let rid = RAWINPUTDEVICE {
            usUsagePage: HID_USAGE_PAGE_GENERIC,
            usUsage: HID_USAGE_GENERIC_MOUSE,
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        };
        // SAFETY: one device entry with its size.
        if unsafe { RegisterRawInputDevices(&rid, 1, size_of::<RAWINPUTDEVICE>() as u32) } == 0 {
            tracing::warn!(
                "raw mouse input unavailable: {}",
                std::io::Error::last_os_error()
            );
        }
    }

    // SAFETY: plain query.
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));
    let mut msg = MSG::default();
    // SAFETY: pumps this thread's messages into a valid MSG.
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {
        // SAFETY: a message GetMessageW filled in.
        unsafe { DispatchMessageW(&msg) };
    }

    // SAFETY: the hooks and the window made above, released once.
    unsafe {
        UnhookWindowsHookEx(kb);
        UnhookWindowsHookEx(ms);
        if !hwnd.is_null() {
            DestroyWindow(hwnd);
        }
    }
}

impl KbmCapture for HookCapture {
    fn start(&mut self, filter: Arc<CaptureFilter>) -> Result<Receiver<KbmEvent>> {
        self.stop();
        let (tx, rx) = crossbeam_channel::bounded(4096);
        *CTX.write().unwrap() = Some(HookCtx { tx, filter });
        let (ready_tx, ready_rx) =
            crossbeam_channel::bounded::<std::result::Result<u32, String>>(1);

        let thread = std::thread::Builder::new()
            .name("dp-input-hooks".into())
            .spawn(move || hook_thread(&ready_tx))
            .map_err(|e| PlatformError::Other(e.to_string()))?;

        match ready_rx.recv() {
            Ok(Ok(id)) => {
                self.thread_id = id;
                self.thread = Some(thread);
                Ok(rx)
            }
            Ok(Err(e)) => Err(PlatformError::Other(e)),
            Err(_) => Err(PlatformError::Other("input hook thread exited".into())),
        }
    }

    fn stop(&mut self) {
        if let Some(t) = self.thread.take() {
            // SAFETY: posting WM_QUIT to a thread we created.
            unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
            let _ = t.join();
        }
        *CTX.write().unwrap() = None;
    }
}

impl Drop for HookCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
