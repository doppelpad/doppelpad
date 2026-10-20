//! XInput reader (Xbox 360 / One / Series controllers and anything else exposing XUSB).

use std::sync::Arc;
use std::time::{Duration, Instant};

use dp_core::pad::{Axis, PadButton, PadState, Rumble, axis_from_i16};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

use super::wide;
use crate::{DeviceId, DeviceInfo, GamepadBackend, PadFamily, PadSnapshot, VirtualIgnoreList};

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct XInputGamepad {
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    thumb_lx: i16,
    thumb_ly: i16,
    thumb_rx: i16,
    thumb_ry: i16,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct XInputState {
    packet_number: u32,
    gamepad: XInputGamepad,
}

#[repr(C)]
struct XInputVibration {
    left: u16,
    right: u16,
}

type GetStateFn = unsafe extern "system" fn(u32, *mut XInputState) -> u32;
type SetStateFn = unsafe extern "system" fn(u32, *mut XInputVibration) -> u32;

const ERROR_SUCCESS: u32 = 0;
const SLOTS: u32 = 4;
const RESCAN: Duration = Duration::from_secs(1);

// XInput button bits.
const BUTTONS: [(u16, PadButton); 15] = [
    (0x0001, PadButton::DpadUp),
    (0x0002, PadButton::DpadDown),
    (0x0004, PadButton::DpadLeft),
    (0x0008, PadButton::DpadRight),
    (0x0010, PadButton::Start),
    (0x0020, PadButton::Back),
    (0x0040, PadButton::LS),
    (0x0080, PadButton::RS),
    (0x0100, PadButton::LB),
    (0x0200, PadButton::RB),
    (0x0400, PadButton::Guide),
    (0x1000, PadButton::A),
    (0x2000, PadButton::B),
    (0x4000, PadButton::X),
    (0x8000, PadButton::Y),
];

pub struct XInputBackend {
    get_state: Option<GetStateFn>,
    set_state: Option<SetStateFn>,
    infos: [Option<Arc<DeviceInfo>>; SLOTS as usize],
    last_probe: [Option<Instant>; SLOTS as usize],
    ignored: Vec<u32>,
}

impl XInputBackend {
    pub fn new() -> Self {
        let mut get_state = None;
        let mut set_state = None;
        for dll in ["xinput1_4.dll", "xinput1_3.dll", "xinput9_1_0.dll"] {
            // SAFETY: plain library load; the handle is intentionally leaked for the process lifetime.
            let h = unsafe { LoadLibraryW(wide(dll).as_ptr()) };
            if h.is_null() {
                continue;
            }
            // Ordinal 100 is the undocumented XInputGetStateEx, which also reports the Guide button.
            // SAFETY: GetProcAddress with an ordinal is the documented MAKEINTRESOURCE pattern.
            let ex = unsafe { GetProcAddress(h, 100usize as *const u8) };
            let plain = unsafe { GetProcAddress(h, c"XInputGetState".as_ptr().cast()) };
            let set = unsafe { GetProcAddress(h, c"XInputSetState".as_ptr().cast()) };
            // SAFETY: the signatures match the XInput ABI.
            get_state = ex
                .or(plain)
                .map(|f| unsafe { std::mem::transmute::<_, GetStateFn>(f) });
            set_state = set.map(|f| unsafe { std::mem::transmute::<_, SetStateFn>(f) });
            if get_state.is_some() {
                break;
            }
        }
        if get_state.is_none() {
            tracing::warn!("XInput is not available; Xbox controllers will not be read");
        }
        XInputBackend {
            get_state,
            set_state,
            infos: Default::default(),
            last_probe: [None; 4],
            ignored: Vec::new(),
        }
    }

    fn info(slot: u32) -> Arc<DeviceInfo> {
        Arc::new(DeviceInfo {
            id: DeviceId(format!("xinput:{slot}")),
            name: format!("Xbox controller (XInput slot {})", slot + 1),
            family: PadFamily::Xbox,
            vendor_id: 0x045E,
            product_id: 0,
            backend: "xinput".into(),
            instance_paths: Vec::new(),
            joycon: None,
            has_motion: false,
            has_touchpad: false,
            has_rumble: true,
        })
    }
}

fn convert(g: &XInputGamepad) -> PadState {
    let mut s = PadState::default();
    for (bit, b) in BUTTONS {
        s.set(b, g.buttons & bit != 0);
    }
    s.set_axis(Axis::LeftX, axis_from_i16(g.thumb_lx));
    s.set_axis(Axis::LeftY, axis_from_i16(g.thumb_ly));
    s.set_axis(Axis::RightX, axis_from_i16(g.thumb_rx));
    s.set_axis(Axis::RightY, axis_from_i16(g.thumb_ry));
    s.set_axis(Axis::LeftTrigger, g.left_trigger as f32 / 255.0);
    s.set_axis(Axis::RightTrigger, g.right_trigger as f32 / 255.0);
    s
}

impl GamepadBackend for XInputBackend {
    fn name(&self) -> &'static str {
        "xinput"
    }

    fn poll(&mut self, out: &mut Vec<PadSnapshot>) {
        let Some(get_state) = self.get_state else {
            return;
        };
        let now = Instant::now();
        for slot in 0..SLOTS {
            let i = slot as usize;
            if self.ignored.contains(&slot) {
                self.infos[i] = None;
                continue;
            }
            // Probing empty slots is slow, so only rescan them periodically.
            if self.infos[i].is_none() && self.last_probe[i].is_some_and(|t| now - t < RESCAN) {
                continue;
            }
            let mut st = XInputState::default();
            // SAFETY: valid out-pointer to a correctly laid out XINPUT_STATE.
            let rc = unsafe { get_state(slot, &mut st) };
            if rc != ERROR_SUCCESS {
                if self.infos[i].take().is_some() {
                    tracing::info!(slot, "XInput controller disconnected");
                }
                self.last_probe[i] = Some(now);
                continue;
            }
            let info = self.infos[i].get_or_insert_with(|| {
                tracing::info!(slot, "XInput controller connected");
                Self::info(slot)
            });
            out.push(PadSnapshot {
                info: info.clone(),
                state: convert(&st.gamepad),
            });
        }
    }

    fn set_rumble(&mut self, id: &DeviceId, rumble: Rumble) {
        let (Some(set_state), Some(slot)) = (self.set_state, id.0.strip_prefix("xinput:")) else {
            return;
        };
        let Ok(slot) = slot.parse::<u32>() else {
            return;
        };
        let mut v = XInputVibration {
            left: (rumble.low.clamp(0.0, 1.0) * 65535.0) as u16,
            right: (rumble.high.clamp(0.0, 1.0) * 65535.0) as u16,
        };
        // SAFETY: valid pointer to XINPUT_VIBRATION.
        unsafe { set_state(slot, &mut v) };
    }

    fn ignore_virtual(&mut self, ignored: &VirtualIgnoreList) {
        self.ignored = ignored.xinput_slots.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_buttons_and_axes() {
        let g = XInputGamepad {
            buttons: 0x1000 | 0x0400 | 0x0001,
            left_trigger: 255,
            right_trigger: 0,
            thumb_lx: i16::MAX,
            thumb_ly: i16::MIN,
            thumb_rx: 0,
            thumb_ry: 0,
        };
        let s = convert(&g);
        assert!(
            s.pressed(PadButton::A) && s.pressed(PadButton::Guide) && s.pressed(PadButton::DpadUp)
        );
        assert!(!s.pressed(PadButton::B));
        assert_eq!(s.axis(Axis::LeftTrigger), 1.0);
        assert_eq!(s.axis(Axis::LeftX), 1.0);
        assert_eq!(s.axis(Axis::LeftY), -1.0);
    }
}
