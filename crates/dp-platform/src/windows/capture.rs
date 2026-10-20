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

use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};
use dp_core::keys::{Key, MouseButton, WheelDir};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_NUMLOCK, VK_PAUSE};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT,
    RIDEV_INPUTSINK, RIM_TYPEMOUSE, RegisterRawInputDevices,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    HC_ACTION, HWND_MESSAGE, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, PostThreadMessageW,
    RegisterClassW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_INPUT,
    WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSW,
};

use super::sendinput::INJECTED_TAG;
use super::wide;
use crate::{CaptureFilter, KbmCapture, KbmEvent, PlatformError, Result};

struct HookCtx {
    tx: Sender<KbmEvent>,
    filter: Arc<CaptureFilter>,
}

/// Hook procedures are plain functions, so their context is global. Only one capture
/// can be active per process.
static CTX: RwLock<Option<HookCtx>> = RwLock::new(None);

const LLKHF_EXTENDED: u32 = 0x01;
const LLKHF_INJECTED: u32 = 0x10;
const LLMHF_INJECTED: u32 = 0x01;
const MOUSE_MOVE_ABSOLUTE: u16 = 0x01;

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
        let ours = k.flags & LLKHF_INJECTED != 0 && k.dwExtraInfo == INJECTED_TAG;
        if !ours && let Some(key) = key_from_hook(k) {
            let down = match wparam as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => Some(true),
                WM_KEYUP | WM_SYSKEYUP => Some(false),
                _ => None,
            };
            if let (Some(down), Ok(guard)) = (down, CTX.read())
                && let Some(ctx) = guard.as_ref()
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
        let ours = m.flags & LLMHF_INJECTED != 0 && m.dwExtraInfo == INJECTED_TAG;
        if !ours
            && let Ok(guard) = CTX.read()
            && let Some(ctx) = guard.as_ref()
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
                    let d = hi as i16 as f32 / 120.0;
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
                    let d = hi as i16 as f32 / 120.0;
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
        let mut raw: RAWINPUT = unsafe { std::mem::zeroed() };
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

impl KbmCapture for HookCapture {
    fn start(&mut self, filter: Arc<CaptureFilter>) -> Result<Receiver<KbmEvent>> {
        self.stop();
        let (tx, rx) = crossbeam_channel::bounded(4096);
        *CTX.write().unwrap() = Some(HookCtx { tx, filter });
        let (ready_tx, ready_rx) =
            crossbeam_channel::bounded::<std::result::Result<u32, String>>(1);

        let thread = std::thread::Builder::new()
            .name("dp-input-hooks".into())
            .spawn(move || unsafe {
                // SAFETY: all calls below are standard Win32 hook / window setup on this thread.
                let hinst = GetModuleHandleW(null());
                let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hinst, 0);
                let ms = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hinst, 0);
                if kb.is_null() || ms.is_null() {
                    let _ = ready_tx.send(Err(format!(
                        "SetWindowsHookEx failed: {}",
                        std::io::Error::last_os_error()
                    )));
                    return;
                }

                let class = wide("doppelpad-rawinput");
                let mut wc: WNDCLASSW = std::mem::zeroed();
                wc.lpfnWndProc = Some(raw_wndproc);
                wc.hInstance = hinst;
                wc.lpszClassName = class.as_ptr();
                RegisterClassW(&wc);
                let hwnd = CreateWindowExW(
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
                );
                if !hwnd.is_null() {
                    let rid = RAWINPUTDEVICE {
                        usUsagePage: 0x01,
                        usUsage: 0x02,
                        dwFlags: RIDEV_INPUTSINK,
                        hwndTarget: hwnd,
                    };
                    if RegisterRawInputDevices(&rid, 1, size_of::<RAWINPUTDEVICE>() as u32) == 0 {
                        tracing::warn!(
                            "raw mouse input unavailable: {}",
                            std::io::Error::last_os_error()
                        );
                    }
                }

                let _ = ready_tx.send(Ok(GetCurrentThreadId()));
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                    DispatchMessageW(&msg);
                }

                UnhookWindowsHookEx(kb);
                UnhookWindowsHookEx(ms);
                if !hwnd.is_null() {
                    DestroyWindow(hwnd);
                }
            })
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
