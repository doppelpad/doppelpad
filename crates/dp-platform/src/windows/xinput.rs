//! XInput reader (Xbox 360 / One / Series controllers and anything else exposing XUSB).

use std::sync::Arc;
use std::time::{Duration, Instant};

use dp_core::pad::{Axis, PadButton, PadState, Rumble, axis_from_i16};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

use super::wide;
use crate::{
    Battery, DeviceId, DeviceInfo, GamepadBackend, PadFamily, PadSnapshot, VirtualIgnoreList,
};

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

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct XInputBatteryInformation {
    battery_type: u8,
    battery_level: u8,
}

type GetStateFn = unsafe extern "system" fn(u32, *mut XInputState) -> u32;
type SetStateFn = unsafe extern "system" fn(u32, *mut XInputVibration) -> u32;
type GetBatteryFn = unsafe extern "system" fn(u32, u8, *mut XInputBatteryInformation) -> u32;
type PowerOffFn = unsafe extern "system" fn(u32) -> u32;

const ERROR_SUCCESS: u32 = 0;
const SLOTS: u32 = 4;
const RESCAN: Duration = Duration::from_secs(1);
/// Battery levels change slowly; asking costs a call per slot.
const BATTERY_EVERY: Duration = Duration::from_secs(10);
const BATTERY_DEVTYPE_GAMEPAD: u8 = 0;

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
    get_battery: Option<GetBatteryFn>,
    power_off: Option<PowerOffFn>,
    /// Last battery reading per slot and when it was taken.
    batteries: [Option<(Instant, Option<Battery>)>; SLOTS as usize],
    infos: [Option<Arc<DeviceInfo>>; SLOTS as usize],
    last_probe: [Option<Instant>; SLOTS as usize],
    ignored: Vec<u32>,
}

impl XInputBackend {
    pub fn new() -> Self {
        let mut get_state = None;
        let mut set_state = None;
        let mut get_battery = None;
        let mut power_off = None;
        for dll in ["xinput1_4.dll", "xinput1_3.dll", "xinput9_1_0.dll"] {
            // SAFETY: plain library load; the handle is intentionally leaked for the process lifetime.
            let h = unsafe { LoadLibraryW(wide(dll).as_ptr()) };
            if h.is_null() {
                continue;
            }
            // Ordinal 100 is the undocumented XInputGetStateEx, which also reports the Guide button.
            // SAFETY: GetProcAddress with an ordinal is the documented MAKEINTRESOURCE pattern.
            let ex = unsafe { GetProcAddress(h, 100usize as *const u8) };
            // SAFETY: `h` is a loaded module and the name a NUL-terminated string.
            let plain = unsafe { GetProcAddress(h, c"XInputGetState".as_ptr().cast()) };
            // SAFETY: as above.
            let set = unsafe { GetProcAddress(h, c"XInputSetState".as_ptr().cast()) };
            // SAFETY: the signatures match the XInput ABI.
            get_state = ex
                .or(plain)
                .map(|f| unsafe { std::mem::transmute::<_, GetStateFn>(f) });
            // SAFETY: the signature matches the XInput ABI.
            set_state = set.map(|f| unsafe { std::mem::transmute::<_, SetStateFn>(f) });
            // Missing from xinput9_1_0.dll.
            // SAFETY: `h` is a loaded module and the name a NUL-terminated string.
            let battery =
                unsafe { GetProcAddress(h, c"XInputGetBatteryInformation".as_ptr().cast()) };
            // SAFETY: the signature matches the XInput ABI.
            get_battery = battery.map(|f| unsafe { std::mem::transmute::<_, GetBatteryFn>(f) });
            // Ordinal 103 turns a wireless controller off (not in xinput9_1_0.dll).
            // SAFETY: ordinal lookup as above.
            let off = unsafe { GetProcAddress(h, 103usize as *const u8) };
            // SAFETY: the signature matches the XInput ABI.
            power_off = off.map(|f| unsafe { std::mem::transmute::<_, PowerOffFn>(f) });
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
            get_battery,
            power_off,
            batteries: Default::default(),
            infos: Default::default(),
            last_probe: [None; 4],
            ignored: Vec::new(),
        }
    }

    /// The slot's battery, asked again every [`BATTERY_EVERY`].
    fn battery(&mut self, slot: u32, now: Instant) -> Option<Battery> {
        let get = self.get_battery?;
        let cached = &mut self.batteries[slot as usize];
        if let Some((at, b)) = *cached
            && now - at < BATTERY_EVERY
        {
            return b;
        }
        let mut info = XInputBatteryInformation::default();
        // SAFETY: valid out-pointer to a correctly laid out XINPUT_BATTERY_INFORMATION.
        let rc = unsafe { get(slot, BATTERY_DEVTYPE_GAMEPAD, &mut info) };
        let b = if rc == ERROR_SUCCESS {
            battery(info)
        } else {
            None
        };
        *cached = Some((now, b));
        b
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

/// Battery type and level as XInput reports them.
fn battery(info: XInputBatteryInformation) -> Option<Battery> {
    let level = [5, 30, 60, 100][info.battery_level.min(3) as usize];
    match info.battery_type {
        0 => None, // disconnected
        1 => Some(Battery {
            level: 100,
            charging: false,
            wired: true,
        }),
        _ => Some(Battery {
            level,
            charging: false,
            wired: false,
        }),
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
    s.set_axis(Axis::LeftTrigger, f32::from(g.left_trigger) / 255.0);
    s.set_axis(Axis::RightTrigger, f32::from(g.right_trigger) / 255.0);
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
                self.batteries[i] = None;
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
            let info = info.clone();
            out.push(PadSnapshot {
                info,
                state: convert(&st.gamepad),
                battery: self.battery(slot, now),
                raw: None,
            });
        }
    }

    fn set_rumble(&mut self, id: &DeviceId, rumble: Rumble) {
        let (Some(set_state), Some(slot)) = (self.set_state, slot_of(id)) else {
            return;
        };
        let mut v = XInputVibration {
            left: (rumble.low.clamp(0.0, 1.0) * 65535.0) as u16,
            right: (rumble.high.clamp(0.0, 1.0) * 65535.0) as u16,
        };
        // SAFETY: valid pointer to XINPUT_VIBRATION.
        unsafe { set_state(slot, &mut v) };
    }

    fn power_off(&mut self, id: &DeviceId) -> bool {
        let (Some(off), Some(slot)) = (self.power_off, slot_of(id)) else {
            return false;
        };
        // A wired pad cannot be turned off; XInput reports its battery as wired.
        let wired = self.batteries[slot as usize]
            .and_then(|(_, b)| b)
            .is_some_and(|b| b.wired);
        // SAFETY: plain call with a slot index.
        !wired && unsafe { off(slot) } == ERROR_SUCCESS
    }

    fn ignore_virtual(&mut self, ignored: &VirtualIgnoreList) {
        self.ignored = ignored.xinput_slots.clone();
    }
}

/// The XInput slot behind an `xinput:<n>` id.
fn slot_of(id: &DeviceId) -> Option<u32> {
    id.0.strip_prefix("xinput:")?
        .parse()
        .ok()
        .filter(|s| *s < SLOTS)
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

    /// What the virtual Xbox 360 pad sends reads back as the same state.
    #[test]
    fn virtual_pad_round_trips() {
        let read_back = |s: &PadState| {
            let g = super::super::vigem::to_xgamepad(s);
            convert(&XInputGamepad {
                buttons: g.buttons.raw,
                left_trigger: g.left_trigger,
                right_trigger: g.right_trigger,
                thumb_lx: g.thumb_lx,
                thumb_ly: g.thumb_ly,
                thumb_rx: g.thumb_rx,
                thumb_ry: g.thumb_ry,
            })
        };
        assert_eq!(BUTTONS.len(), 15);
        for (bit, b) in BUTTONS {
            let mut s = PadState::default();
            s.set(b, true);
            assert_eq!(
                super::super::vigem::to_xgamepad(&s).buttons.raw,
                bit,
                "{b:?}"
            );
            assert_eq!(read_back(&s).buttons, s.buttons, "{b:?}");
        }
        let mut s = PadState::default();
        s.set_axis(Axis::LeftX, 1.0);
        s.set_axis(Axis::LeftY, -0.5);
        s.set_axis(Axis::RightX, -1.0);
        s.set_axis(Axis::LeftTrigger, 0.5);
        s.set_axis(Axis::RightTrigger, 1.0);
        let back = read_back(&s);
        for a in [
            Axis::LeftX,
            Axis::LeftY,
            Axis::RightX,
            Axis::RightY,
            Axis::LeftTrigger,
            Axis::RightTrigger,
        ] {
            assert!((back.axis(a) - s.axis(a)).abs() < 0.005, "{a:?}");
        }
    }
}
