//! Pure translation between evdev gamepad events and [`PadState`]: the reader-side mappers
//! for gamepad, motion-sensor and touchpad nodes, the event encoding of the virtual Xbox 360
//! pad, and the force-feedback mixer behind its rumble. No I/O, so it is unit-tested on
//! every OS.

use dp_core::pad::{Axis, Motion, PadButton, PadState, Rumble, TouchPoint, axis_to_i16};

use super::codes::*;
pub use crate::nintendo::NINTENDO_VID;
use crate::nintendo::imu_to_pad;
use crate::sony::{SONY_VID, SonyModel, model_for};
use crate::{JoyConSide, PadFamily};

pub const MICROSOFT_VID: u16 = 0x045E;

/// Value range of one absolute axis (`struct input_absinfo`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct AbsRange {
    pub min: i32,
    pub max: i32,
    /// Units per physical unit (deg/s for gyros, g for accelerometers); 0 if unknown.
    pub resolution: i32,
}

impl AbsRange {
    #[cfg(test)]
    pub const fn new(min: i32, max: i32) -> Self {
        AbsRange {
            min,
            max,
            resolution: 0,
        }
    }

    /// `[-1, 1]` around the center of the range (evdev orientation: minimum is left/up).
    /// The center is the upper middle value (0 for `-32768..=32767`, 128 for `0..=255`), so
    /// a resting stick maps to exactly 0 and both ends to exactly ±1.
    pub fn centered(&self, v: i32) -> f32 {
        let (min, max, v) = (i64::from(self.min), i64::from(self.max), i64::from(v));
        let center = (min + max + 1).div_euclid(2);
        let span = if v >= center {
            max - center
        } else {
            center - min
        };
        if span <= 0 {
            return 0.0;
        }
        ((v - center) as f32 / span as f32).clamp(-1.0, 1.0)
    }

    /// `[0, 1]` from minimum to maximum.
    pub fn unit(&self, v: i32) -> f32 {
        let span = self.max as f32 - self.min as f32;
        if span <= 0.0 {
            return 0.0;
        }
        ((v as f32 - self.min as f32) / span).clamp(0.0, 1.0)
    }
}

/// Side of a single Joy-Con driven by hid-nintendo ("Nintendo Switch Left Joy-Con", also
/// inside the charging grip). NES controllers reuse the right Joy-Con's PID, hence the name.
pub fn joycon_side(vendor: u16, product: u16, name: &str) -> Option<JoyConSide> {
    if vendor != NINTENDO_VID || !name.contains("Joy-Con") || name.contains("Combined") {
        return None;
    }
    if name.contains("Left") || (product == 0x2006 && !name.contains("Right")) {
        Some(JoyConSide::Left)
    } else if name.contains("Right") || product == 0x2007 {
        Some(JoyConSide::Right)
    } else {
        None
    }
}

/// A device id that survives reconnects and reboots: vendor and product plus the device's
/// unique id (Bluetooth address, serial number) or else its physical path (USB port). The
/// event node, whose number changes, is the last resort.
pub fn stable_id(
    vendor: u16,
    product: u16,
    uniq: Option<&str>,
    phys: Option<&str>,
    node: &str,
) -> String {
    let key = [uniq, phys]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or(node);
    format!("evdev:{vendor:04x}:{product:04x}:{key}")
}

pub fn family_for(vendor: u16, product: u16) -> PadFamily {
    match vendor {
        SONY_VID => match model_for(vendor, product) {
            Some(SonyModel::DualShock4) => PadFamily::DualShock4,
            Some(_) => PadFamily::DualSense,
            None => PadFamily::Generic,
        },
        MICROSOFT_VID => PadFamily::Xbox,
        // Joy-Con (L) and joycond's combined Joy-Cons; the right Joy-Con shares its PID with
        // the NES controllers, see [`joycon_side`].
        NINTENDO_VID if matches!(product, 0x2006 | 0x2008) => PadFamily::JoyCon,
        NINTENDO_VID => PadFamily::SwitchPro,
        _ => PadFamily::Generic,
    }
}

/// How a driver numbers the four face buttons.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FaceLayout {
    /// `BTN_NORTH` / `BTN_WEST` are the top / left buttons, as `gamepad.rst` requires
    /// (hid-playstation, hid-sony, hid-nintendo).
    Positional,
    /// Xbox-style drivers (xpad and most others) report by label: the left "X" button is
    /// `BTN_X`, which is the same code as `BTN_NORTH`, and the top "Y" button is `BTN_Y`
    /// (= `BTN_WEST`).
    Labels,
}

impl FaceLayout {
    pub fn for_family(family: PadFamily) -> Self {
        match family {
            PadFamily::DualShock4
            | PadFamily::DualSense
            | PadFamily::SwitchPro
            | PadFamily::JoyCon
            | PadFamily::GameCube => FaceLayout::Positional,
            PadFamily::Xbox | PadFamily::Generic => FaceLayout::Labels,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Role {
    Stick(Axis),
    /// Stick axis whose evdev direction (down positive) is the opposite of ours.
    StickInverted(Axis),
    Trigger(Axis),
    HatX,
    HatY,
}

/// Turns the events of a gamepad node into a [`PadState`] (buttons and axes only; motion
/// and touch come from their own nodes).
#[derive(Clone, Debug)]
pub struct PadMapper {
    family: PadFamily,
    /// Single Joy-Con: its rail buttons (SL/SR) arrive on the shoulder codes.
    joycon: Option<JoyConSide>,
    face: FaceLayout,
    axes: Vec<(u16, Role, AbsRange)>,
    analog_triggers: bool,
    state: PadState,
}

impl PadMapper {
    /// `axes` are the absolute axes the device supports, with their ranges.
    pub fn new(family: PadFamily, axes: &[(u16, AbsRange)]) -> Self {
        let has = |code| axes.iter().any(|&(c, _)| c == code);
        let mut roles = vec![
            (ABS_X, Role::Stick(Axis::LeftX)),
            (ABS_Y, Role::StickInverted(Axis::LeftY)),
            (ABS_HAT0X, Role::HatX),
            (ABS_HAT0Y, Role::HatY),
        ];
        // xpad / hid-playstation / hid-nintendo put the right stick on RX/RY and analog
        // triggers on Z/RZ; generic HID pads without RX/RY use Z/RZ for the right stick.
        let right_on_z = !(has(ABS_RX) && has(ABS_RY));
        if right_on_z {
            roles.push((ABS_Z, Role::Stick(Axis::RightX)));
            roles.push((ABS_RZ, Role::StickInverted(Axis::RightY)));
        } else {
            roles.push((ABS_RX, Role::Stick(Axis::RightX)));
            roles.push((ABS_RY, Role::StickInverted(Axis::RightY)));
        }
        let (lt, rt) = if !right_on_z && has(ABS_Z) && has(ABS_RZ) {
            (ABS_Z, ABS_RZ)
        } else {
            (ABS_BRAKE, ABS_GAS)
        };
        let analog_triggers = has(lt) && has(rt);
        if analog_triggers {
            roles.push((lt, Role::Trigger(Axis::LeftTrigger)));
            roles.push((rt, Role::Trigger(Axis::RightTrigger)));
        }
        let axes = roles
            .into_iter()
            .filter_map(|(code, role)| {
                let &(_, range) = axes.iter().find(|&&(c, _)| c == code)?;
                Some((code, role, range))
            })
            .collect();
        PadMapper {
            family,
            joycon: None,
            face: FaceLayout::for_family(family),
            axes,
            analog_triggers,
            state: PadState::default(),
        }
    }

    /// Map a single Joy-Con, whose SL/SR come in as the shoulder codes of the other side
    /// (hid-nintendo).
    pub fn joycon(mut self, side: Option<JoyConSide>) -> Self {
        self.joycon = side;
        self
    }

    fn button(&self, code: u16) -> Option<PadButton> {
        let positional = self.face == FaceLayout::Positional;
        let sony = matches!(self.family, PadFamily::DualShock4 | PadFamily::DualSense);
        let nintendo = matches!(
            self.family,
            PadFamily::SwitchPro | PadFamily::JoyCon | PadFamily::GameCube
        );
        // joycond's combined Joy-Cons forward SL/SR as TRIGGER_HAPPY1-4.
        let joycond = self.family == PadFamily::JoyCon && self.joycon.is_none();
        let (left_jc, right_jc) = (
            self.joycon == Some(JoyConSide::Left),
            self.joycon == Some(JoyConSide::Right),
        );
        Some(match code {
            BTN_TR if left_jc => PadButton::PaddleUL,   // SL
            BTN_TR2 if left_jc => PadButton::PaddleLL,  // SR
            BTN_TL if right_jc => PadButton::PaddleLR,  // SL
            BTN_TL2 if right_jc => PadButton::PaddleUR, // SR
            BTN_SOUTH => PadButton::A,
            BTN_EAST => PadButton::B,
            BTN_NORTH if positional => PadButton::Y,
            BTN_NORTH => PadButton::X,
            BTN_WEST if positional => PadButton::X,
            BTN_WEST => PadButton::Y,
            BTN_TL => PadButton::LB,
            BTN_TR => PadButton::RB,
            BTN_SELECT => PadButton::Back,
            BTN_START => PadButton::Start,
            BTN_MODE => PadButton::Guide,
            BTN_THUMBL => PadButton::LS,
            BTN_THUMBR => PadButton::RS,
            BTN_DPAD_UP => PadButton::DpadUp,
            BTN_DPAD_DOWN => PadButton::DpadDown,
            BTN_DPAD_LEFT => PadButton::DpadLeft,
            BTN_DPAD_RIGHT => PadButton::DpadRight,
            BTN_GRIPL => PadButton::PaddleUL,
            BTN_GRIPR => PadButton::PaddleUR,
            BTN_GRIPL2 => PadButton::PaddleLL,
            BTN_GRIPR2 => PadButton::PaddleLR,
            BTN_TRIGGER_HAPPY1 if sony => PadButton::PaddleUL,
            BTN_TRIGGER_HAPPY2 if sony => PadButton::PaddleUR,
            BTN_TRIGGER_HAPPY3 if sony => PadButton::PaddleLL,
            BTN_TRIGGER_HAPPY4 if sony => PadButton::PaddleLR,
            BTN_TRIGGER_HAPPY1 if joycond => PadButton::PaddleUL, // left SL
            BTN_TRIGGER_HAPPY2 if joycond => PadButton::PaddleLL, // left SR
            BTN_TRIGGER_HAPPY3 if joycond => PadButton::PaddleLR, // right SL
            BTN_TRIGGER_HAPPY4 if joycond => PadButton::PaddleUR, // right SR
            KEY_RECORD => PadButton::Misc,
            BTN_Z if nintendo => PadButton::Misc, // Capture
            _ => return None,
        })
    }

    /// Feed one event (`SYN` events are ignored; read [`PadMapper::state`] after a report).
    pub fn event(&mut self, ty: u16, code: u16, value: i32) {
        match ty {
            EV_KEY => {
                let down = value != 0;
                if let Some(b) = self.button(code) {
                    self.state.set(b, down);
                } else if !self.analog_triggers && (code == BTN_TL2 || code == BTN_TR2) {
                    let axis = if code == BTN_TL2 {
                        Axis::LeftTrigger
                    } else {
                        Axis::RightTrigger
                    };
                    self.state.set_axis(axis, if down { 1.0 } else { 0.0 });
                }
            }
            EV_ABS => {
                let Some(&(_, role, range)) = self.axes.iter().find(|a| a.0 == code) else {
                    return;
                };
                let s = &mut self.state;
                match role {
                    Role::Stick(axis) => s.set_axis(axis, range.centered(value)),
                    Role::StickInverted(axis) => s.set_axis(axis, -range.centered(value)),
                    Role::Trigger(axis) => s.set_axis(axis, range.unit(value)),
                    Role::HatX => {
                        s.set(PadButton::DpadLeft, value < 0);
                        s.set(PadButton::DpadRight, value > 0);
                    }
                    Role::HatY => {
                        s.set(PadButton::DpadUp, value < 0);
                        s.set(PadButton::DpadDown, value > 0);
                    }
                }
            }
            _ => {}
        }
    }

    pub fn state(&self) -> PadState {
        self.state
    }
}

/// Default sensor resolutions of hid-playstation (`DS_GYRO_RES_PER_DEG_S`,
/// `DS_ACC_RES_PER_G`), used if a node does not report its own.
const GYRO_RES_PER_DEG_S: i32 = 1024;
const ACCEL_RES_PER_G: i32 = 8192;
/// hid-nintendo's resolutions.
const NINTENDO_GYRO_RES_PER_DEG_S: i32 = 14247;
const NINTENDO_ACCEL_RES_PER_G: i32 = 4096;

/// Motion-sensor node (`INPUT_PROP_ACCELEROMETER`): accelerometer on `ABS_X/Y/Z`,
/// gyroscope on `ABS_RX/RY/RZ` (pitch, yaw, roll), scaled by each axis' resolution.
#[derive(Clone, Debug)]
pub struct MotionMapper {
    /// Physical units per raw unit, indexed by `ABS_X ..= ABS_RZ`.
    scale: [f32; 6],
    motion: Motion,
    /// hid-nintendo axes (x toward the triggers, y left, z up) rather than ours.
    nintendo: bool,
}

impl MotionMapper {
    pub fn new(axes: &[(u16, AbsRange)]) -> Self {
        Self::with_defaults(axes, ACCEL_RES_PER_G, GYRO_RES_PER_DEG_S, false)
    }

    /// IMU node of a Switch controller (hid-nintendo).
    pub fn nintendo(axes: &[(u16, AbsRange)]) -> Self {
        Self::with_defaults(
            axes,
            NINTENDO_ACCEL_RES_PER_G,
            NINTENDO_GYRO_RES_PER_DEG_S,
            true,
        )
    }

    fn with_defaults(axes: &[(u16, AbsRange)], accel: i32, gyro: i32, nintendo: bool) -> Self {
        let mut scale = [0.0; 6];
        for (code, s) in (ABS_X..=ABS_RZ).zip(&mut scale) {
            let default = if code < ABS_RX { accel } else { gyro };
            let res = axes
                .iter()
                .find(|a| a.0 == code)
                .map(|a| a.1.resolution)
                .filter(|&r| r > 0)
                .unwrap_or(default);
            *s = 1.0 / res as f32;
        }
        MotionMapper {
            scale,
            motion: Motion::default(),
            nintendo,
        }
    }

    pub fn event(&mut self, ty: u16, code: u16, value: i32) {
        if ty != EV_ABS || code > ABS_RZ {
            return;
        }
        let v = value as f32 * self.scale[code as usize];
        match code {
            ABS_X | ABS_Y | ABS_Z => self.motion.accel[code as usize] = v,
            _ => self.motion.gyro[(code - ABS_RX) as usize] = v,
        }
    }

    pub fn motion(&self) -> Motion {
        if self.nintendo {
            Motion {
                gyro: imu_to_pad(self.motion.gyro),
                accel: imu_to_pad(self.motion.accel),
            }
        } else {
            self.motion
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
struct Slot {
    active: bool,
    x: i32,
    y: i32,
}

/// Touchpad node (multi-touch protocol B, two slots) plus its click button (`BTN_LEFT`).
#[derive(Clone, Debug)]
pub struct TouchMapper {
    x: AbsRange,
    y: AbsRange,
    slot: usize,
    slots: [Slot; 2],
    click: bool,
}

impl TouchMapper {
    pub fn new(x: AbsRange, y: AbsRange) -> Self {
        TouchMapper {
            x,
            y,
            slot: 0,
            slots: [Slot::default(); 2],
            click: false,
        }
    }

    pub fn event(&mut self, ty: u16, code: u16, value: i32) {
        match (ty, code) {
            (EV_KEY, BTN_LEFT) => self.click = value != 0,
            (EV_ABS, ABS_MT_SLOT) => self.slot = value.max(0) as usize,
            (EV_ABS, _) => {
                let Some(slot) = self.slots.get_mut(self.slot) else {
                    return;
                };
                match code {
                    ABS_MT_TRACKING_ID => slot.active = value >= 0,
                    ABS_MT_POSITION_X => slot.x = value,
                    ABS_MT_POSITION_Y => slot.y = value,
                    _ => {}
                }
            }
            _ => {}
        }
    }

    pub fn touch(&self) -> [Option<TouchPoint>; 2] {
        self.slots.map(|s| {
            s.active.then(|| TouchPoint {
                x: self.x.unit(s.x),
                y: self.y.unit(s.y),
            })
        })
    }

    pub fn click(&self) -> bool {
        self.click
    }
}

/// Name, ids and layout of a wired Xbox 360 pad as xpad registers it.
pub const X360_NAME: &str = "Microsoft X-Box 360 pad";
pub const X360_VID: u16 = MICROSOFT_VID;
pub const X360_PID: u16 = 0x028E;
pub const X360_VERSION: u16 = 0x0114;

/// Buttons of the virtual Xbox 360 pad. xpad reports by label, so the left "X" button is
/// `BTN_X` (= `BTN_NORTH`) and the top "Y" button `BTN_Y` (= `BTN_WEST`).
pub const X360_KEYS: [(PadButton, u16); 11] = [
    (PadButton::A, BTN_SOUTH),
    (PadButton::B, BTN_EAST),
    (PadButton::X, BTN_NORTH),
    (PadButton::Y, BTN_WEST),
    (PadButton::LB, BTN_TL),
    (PadButton::RB, BTN_TR),
    (PadButton::Back, BTN_SELECT),
    (PadButton::Start, BTN_START),
    (PadButton::Guide, BTN_MODE),
    (PadButton::LS, BTN_THUMBL),
    (PadButton::RS, BTN_THUMBR),
];

/// Absolute axes of the virtual Xbox 360 pad: `(code, min, max, fuzz, flat)` as in
/// xpad's `xpad_set_up_abs`.
pub const X360_AXES: [(u16, i32, i32, i32, i32); 8] = [
    (ABS_X, -32768, 32767, 16, 128),
    (ABS_Y, -32768, 32767, 16, 128),
    (ABS_RX, -32768, 32767, 16, 128),
    (ABS_RY, -32768, 32767, 16, 128),
    (ABS_Z, 0, 255, 0, 0),
    (ABS_RZ, 0, 255, 0, 0),
    (ABS_HAT0X, -1, 1, 0, 0),
    (ABS_HAT0Y, -1, 1, 0, 0),
];

pub const X360_EVENTS: usize = X360_KEYS.len() + X360_AXES.len();

/// The `(type, code, value)` of every key and axis a real Xbox 360 pad would report in `s`.
pub fn x360_events(s: &PadState) -> [(u16, u16, i32); X360_EVENTS] {
    let trigger = |a| (s.axis(a).clamp(0.0, 1.0) * 255.0).round() as i32;
    let dpad = |neg, pos| i32::from(s.pressed(pos)) - i32::from(s.pressed(neg));
    let values = [
        i32::from(axis_to_i16(s.axis(Axis::LeftX))),
        i32::from(axis_to_i16(-s.axis(Axis::LeftY))), // evdev: up is negative
        i32::from(axis_to_i16(s.axis(Axis::RightX))),
        i32::from(axis_to_i16(-s.axis(Axis::RightY))),
        trigger(Axis::LeftTrigger),
        trigger(Axis::RightTrigger),
        dpad(PadButton::DpadLeft, PadButton::DpadRight),
        dpad(PadButton::DpadUp, PadButton::DpadDown),
    ];
    let mut out = [(0, 0, 0); X360_EVENTS];
    for (o, &(b, code)) in out.iter_mut().zip(&X360_KEYS) {
        *o = (EV_KEY, code, i32::from(s.pressed(b)));
    }
    for (o, (&(code, ..), v)) in out[X360_KEYS.len()..]
        .iter_mut()
        .zip(X360_AXES.iter().zip(values))
    {
        *o = (EV_ABS, code, v);
    }
    out
}

#[derive(Copy, Clone, Debug)]
struct Effect {
    id: i16,
    strong: u16,
    weak: u16,
    length_ms: u16,
    delay_ms: u16,
    /// Start time and repeat count while playing.
    playing: Option<(u64, i32)>,
}

/// Rumble effects games upload to the virtual pad (`FF_RUMBLE` through uinput), and which
/// of them are playing. Times are milliseconds on any monotonic clock.
#[derive(Clone, Debug)]
pub struct FfMixer {
    effects: Vec<Effect>,
    gain: u16,
}

impl Default for FfMixer {
    fn default() -> Self {
        FfMixer {
            effects: Vec::new(),
            gain: u16::MAX,
        }
    }
}

impl FfMixer {
    /// A game uploaded (or updated) rumble effect `id`.
    pub fn upload(&mut self, id: i16, strong: u16, weak: u16, length_ms: u16, delay_ms: u16) {
        let new = Effect {
            id,
            strong,
            weak,
            length_ms,
            delay_ms,
            playing: None,
        };
        match self.effects.iter_mut().find(|e| e.id == id) {
            Some(e) => {
                *e = Effect {
                    playing: e.playing,
                    ..new
                }
            }
            None => self.effects.push(new),
        }
    }

    pub fn erase(&mut self, id: i16) {
        self.effects.retain(|e| e.id != id);
    }

    /// `EV_FF` with code `id`: play `count` times, or stop if `count` is 0.
    pub fn play(&mut self, id: i16, count: i32, now_ms: u64) {
        if let Some(e) = self.effects.iter_mut().find(|e| e.id == id) {
            e.playing = (count > 0).then_some((now_ms, count));
        }
    }

    /// `EV_FF` / `FF_GAIN`: overall strength, `0..=0xFFFF`.
    pub fn set_gain(&mut self, gain: u16) {
        self.gain = gain;
    }

    /// Sum of all effects playing at `now_ms` (an effect of length 0 plays until stopped).
    pub fn rumble(&self, now_ms: u64) -> Rumble {
        let (mut strong, mut weak) = (0u32, 0u32);
        for e in &self.effects {
            let Some((start, count)) = e.playing else {
                continue;
            };
            let Some(t) = now_ms.checked_sub(start + u64::from(e.delay_ms)) else {
                continue;
            };
            if e.length_ms != 0 && t >= u64::from(e.length_ms) * count as u64 {
                continue;
            }
            strong += u32::from(e.strong);
            weak += u32::from(e.weak);
        }
        let gain = f32::from(self.gain) / 65535.0;
        Rumble {
            low: strong.min(65535) as f32 / 65535.0 * gain,
            high: weak.min(65535) as f32 / 65535.0 * gain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_ids_prefer_unique_then_physical() {
        let node = "/dev/input/event17";
        assert_eq!(
            stable_id(
                0x054c,
                0x0ce6,
                Some("a0:b1:c2:d3:e4:f5"),
                Some("usb-1/input0"),
                node
            ),
            "evdev:054c:0ce6:a0:b1:c2:d3:e4:f5"
        );
        assert_eq!(
            stable_id(
                0x045e,
                0x028e,
                Some(""),
                Some("usb-0000:00:14.0-2/input0"),
                node
            ),
            "evdev:045e:028e:usb-0000:00:14.0-2/input0"
        );
        assert_eq!(
            stable_id(1, 2, None, None, node),
            "evdev:0001:0002:/dev/input/event17"
        );
    }

    const STICK: AbsRange = AbsRange::new(-32768, 32767);
    const BYTE: AbsRange = AbsRange::new(0, 255);
    const HAT: AbsRange = AbsRange::new(-1, 1);

    fn xpad() -> PadMapper {
        PadMapper::new(
            PadFamily::Xbox,
            &[
                (ABS_X, STICK),
                (ABS_Y, STICK),
                (ABS_RX, STICK),
                (ABS_RY, STICK),
                (ABS_Z, BYTE),
                (ABS_RZ, BYTE),
                (ABS_HAT0X, HAT),
                (ABS_HAT0Y, HAT),
            ],
        )
    }

    fn dualsense() -> PadMapper {
        PadMapper::new(
            PadFamily::DualSense,
            &[
                (ABS_X, BYTE),
                (ABS_Y, BYTE),
                (ABS_RX, BYTE),
                (ABS_RY, BYTE),
                (ABS_Z, BYTE),
                (ABS_RZ, BYTE),
                (ABS_HAT0X, HAT),
                (ABS_HAT0Y, HAT),
            ],
        )
    }

    #[test]
    fn ranges() {
        assert_eq!(STICK.centered(32767), 1.0);
        assert_eq!(STICK.centered(-32768), -1.0);
        assert_eq!(STICK.centered(0), 0.0);
        assert_eq!(BYTE.centered(128), 0.0);
        assert_eq!(BYTE.centered(0), -1.0);
        assert_eq!(BYTE.centered(255), 1.0);
        assert_eq!(HAT.centered(-1), -1.0);
        assert_eq!(BYTE.unit(255), 1.0);
        assert_eq!(AbsRange::new(5, 5).centered(5), 0.0);
    }

    #[test]
    fn xpad_sticks_triggers_hat() {
        let mut m = xpad();
        m.event(EV_ABS, ABS_Y, -32768); // pushed up
        m.event(EV_ABS, ABS_RX, 32767);
        m.event(EV_ABS, ABS_Z, 255);
        m.event(EV_ABS, ABS_HAT0X, 1);
        m.event(EV_ABS, ABS_HAT0Y, -1);
        let s = m.state();
        assert_eq!(
            s.axis(Axis::LeftY),
            1.0,
            "evdev up is negative, ours positive"
        );
        assert_eq!(s.axis(Axis::RightX), 1.0);
        assert_eq!(s.axis(Axis::LeftTrigger), 1.0);
        assert!(s.pressed(PadButton::DpadRight) && s.pressed(PadButton::DpadUp));
        assert!(!s.pressed(PadButton::DpadLeft) && !s.pressed(PadButton::DpadDown));
        m.event(EV_ABS, ABS_HAT0X, 0);
        assert!(!m.state().pressed(PadButton::DpadRight));
    }

    #[test]
    fn face_buttons_by_layout() {
        let mut x = xpad();
        x.event(EV_KEY, BTN_NORTH, 1); // xpad: BTN_X = left "X" button
        assert!(x.state().pressed(PadButton::X));
        let mut ds = dualsense();
        ds.event(EV_KEY, BTN_NORTH, 1); // hid-playstation: triangle (top)
        ds.event(EV_KEY, BTN_WEST, 1); // square (left)
        ds.event(EV_KEY, BTN_SOUTH, 1);
        let s = ds.state();
        assert!(s.pressed(PadButton::Y) && s.pressed(PadButton::X) && s.pressed(PadButton::A));
        ds.event(EV_KEY, BTN_WEST, 0);
        assert!(!ds.state().pressed(PadButton::X));
    }

    #[test]
    fn misc_buttons_and_paddles() {
        let mut x = xpad();
        for code in [BTN_MODE, KEY_RECORD, BTN_GRIPL2, BTN_SELECT, BTN_THUMBR] {
            x.event(EV_KEY, code, 1);
        }
        let s = x.state();
        for b in [
            PadButton::Guide,
            PadButton::Misc,
            PadButton::PaddleLL,
            PadButton::Back,
            PadButton::RS,
        ] {
            assert!(s.pressed(b), "{b:?}");
        }
        // TRIGGER_HAPPY is only a paddle on Sony pads.
        x.event(EV_KEY, BTN_TRIGGER_HAPPY1, 1);
        assert!(!x.state().pressed(PadButton::PaddleUL));
        let mut ds = dualsense();
        ds.event(EV_KEY, BTN_TRIGGER_HAPPY4, 1);
        assert!(ds.state().pressed(PadButton::PaddleLR));
    }

    #[test]
    fn digital_triggers_without_trigger_axes() {
        // hid-nintendo: ZL/ZR are buttons, no trigger axes.
        let mut m = PadMapper::new(
            PadFamily::SwitchPro,
            &[
                (ABS_X, STICK),
                (ABS_Y, STICK),
                (ABS_RX, STICK),
                (ABS_RY, STICK),
            ],
        );
        m.event(EV_KEY, BTN_TR2, 1);
        m.event(EV_KEY, BTN_EAST, 1); // Nintendo "A" sits on the right
        m.event(EV_KEY, BTN_Z, 1);
        let s = m.state();
        assert_eq!(s.axis(Axis::RightTrigger), 1.0);
        assert!(s.pressed(PadButton::B) && s.pressed(PadButton::Misc));
        // With analog triggers the digital L2/R2 bits are ignored.
        let mut ds = dualsense();
        ds.event(EV_KEY, BTN_TL2, 1);
        assert_eq!(ds.state().axis(Axis::LeftTrigger), 0.0);
    }

    #[test]
    fn generic_pad_right_stick_on_z() {
        let mut m = PadMapper::new(
            PadFamily::Generic,
            &[(ABS_X, BYTE), (ABS_Y, BYTE), (ABS_Z, BYTE), (ABS_RZ, BYTE)],
        );
        m.event(EV_ABS, ABS_Z, 255);
        m.event(EV_ABS, ABS_RZ, 0);
        let s = m.state();
        assert_eq!(s.axis(Axis::RightX), 1.0);
        assert_eq!(s.axis(Axis::RightY), 1.0);
        assert_eq!(s.axis(Axis::LeftTrigger), 0.0);
    }

    #[test]
    fn motion_uses_resolution() {
        let gyro = AbsRange {
            resolution: 1024,
            ..AbsRange::new(-2048 * 1024, 2048 * 1024)
        };
        let accel = AbsRange {
            resolution: 8192,
            ..AbsRange::new(-4 * 8192, 4 * 8192)
        };
        let mut m = MotionMapper::new(&[(ABS_Y, accel), (ABS_RY, gyro)]);
        m.event(EV_ABS, ABS_Y, 8192);
        m.event(EV_ABS, ABS_RY, -90 * 1024);
        m.event(EV_ABS, ABS_RX, 512); // no absinfo: default resolution
        let mo = m.motion();
        assert_eq!(mo.accel, [0.0, 1.0, 0.0]);
        assert_eq!(mo.gyro, [0.5, -90.0, 0.0]);
    }

    #[test]
    fn touch_slots_and_click() {
        let mut t = TouchMapper::new(AbsRange::new(0, 1919), AbsRange::new(0, 1079));
        t.event(EV_ABS, ABS_MT_SLOT, 1);
        t.event(EV_ABS, ABS_MT_TRACKING_ID, 7);
        t.event(EV_ABS, ABS_MT_POSITION_X, 1919);
        t.event(EV_ABS, ABS_MT_POSITION_Y, 0);
        t.event(EV_KEY, BTN_LEFT, 1);
        let p = t.touch();
        assert!(p[0].is_none());
        assert_eq!(p[1], Some(TouchPoint { x: 1.0, y: 0.0 }));
        assert!(t.click());
        t.event(EV_ABS, ABS_MT_TRACKING_ID, -1);
        t.event(EV_ABS, ABS_MT_SLOT, 5); // out of range: ignored
        t.event(EV_ABS, ABS_MT_TRACKING_ID, 3);
        assert_eq!(t.touch(), [None, None]);
    }

    #[test]
    fn x360_encoding() {
        let neutral = x360_events(&PadState::default());
        assert!(neutral.iter().all(|&(_, _, v)| v == 0));

        let mut s = PadState::default();
        s.set(PadButton::X, true);
        s.set(PadButton::DpadUp, true);
        s.set_axis(Axis::LeftY, 1.0);
        s.set_axis(Axis::RightTrigger, 1.0);
        let ev = x360_events(&s);
        let get = |ty, code| ev.iter().find(|e| e.0 == ty && e.1 == code).unwrap().2;
        assert_eq!(get(EV_KEY, BTN_NORTH), 1, "Xbox X is BTN_X");
        assert_eq!(get(EV_KEY, BTN_WEST), 0);
        assert_eq!(get(EV_ABS, ABS_Y), -32768);
        assert_eq!(get(EV_ABS, ABS_RZ), 255);
        assert_eq!(get(EV_ABS, ABS_HAT0Y), -1);
    }

    #[test]
    fn x360_roundtrips_through_reader() {
        let mut s = PadState::default();
        s.set(PadButton::Y, true);
        s.set(PadButton::DpadLeft, true);
        s.set_axis(Axis::RightY, -1.0);
        s.set_axis(Axis::LeftTrigger, 1.0);
        let mut m = xpad();
        for (ty, code, v) in x360_events(&s) {
            m.event(ty, code, v);
        }
        assert_eq!(m.state(), s);
    }

    #[test]
    fn rumble_mixing() {
        let mut ff = FfMixer::default();
        ff.upload(0, 0xFFFF, 0, 100, 0);
        ff.upload(1, 0, 0x8000, 0, 50);
        assert_eq!(ff.rumble(0), Rumble::default());
        ff.play(0, 1, 1000);
        ff.play(1, 1, 1000);
        let r = ff.rumble(1010);
        assert_eq!((r.low, r.high), (1.0, 0.0), "effect 1 is still delayed");
        let r = ff.rumble(1060);
        assert!(r.low == 1.0 && (r.high - 0.5).abs() < 1e-3);
        let r = ff.rumble(1100);
        assert_eq!(r.low, 0.0, "effect 0 ran out");
        ff.set_gain(0x8000);
        assert!((ff.rumble(5000).high - 0.25).abs() < 1e-3);
        ff.play(1, 0, 5000);
        assert_eq!(ff.rumble(5001), Rumble::default());
        ff.erase(1);
        ff.play(1, 1, 6000);
        assert_eq!(ff.rumble(6001), Rumble::default());
    }

    #[test]
    fn families() {
        assert_eq!(family_for(0x054C, 0x0CE6), PadFamily::DualSense);
        assert_eq!(family_for(0x054C, 0x09CC), PadFamily::DualShock4);
        assert_eq!(family_for(0x045E, 0x028E), PadFamily::Xbox);
        assert_eq!(family_for(0x057E, 0x2009), PadFamily::SwitchPro);
        assert_eq!(family_for(0x1234, 0x5678), PadFamily::Generic);
    }

    #[test]
    fn joycon_sides() {
        assert_eq!(family_for(0x057E, 0x2006), PadFamily::JoyCon);
        assert_eq!(family_for(0x057E, 0x2008), PadFamily::JoyCon);
        let side = |pid, name| joycon_side(0x057E, pid, name);
        assert_eq!(
            side(0x2006, "Nintendo Switch Left Joy-Con"),
            Some(JoyConSide::Left)
        );
        assert_eq!(
            side(0x2007, "Nintendo Switch Right Joy-Con"),
            Some(JoyConSide::Right)
        );
        assert_eq!(
            side(0x200E, "Nintendo Switch Left Joy-Con"),
            Some(JoyConSide::Left)
        );
        assert_eq!(side(0x2007, "NES Controller (R)"), None);
        assert_eq!(side(0x2008, "Nintendo Switch Combined Joy-Cons"), None);
        assert_eq!(joycon_side(0x054C, 0x2006, "Joy-Con"), None);
    }

    fn joycon(side: JoyConSide) -> PadMapper {
        PadMapper::new(PadFamily::JoyCon, &[(ABS_X, STICK), (ABS_Y, STICK)]).joycon(Some(side))
    }

    #[test]
    fn joycon_rail_buttons() {
        let mut l = joycon(JoyConSide::Left);
        l.event(EV_KEY, BTN_TR, 1); // SL
        l.event(EV_KEY, BTN_TR2, 1); // SR
        l.event(EV_KEY, BTN_TL2, 1); // ZL
        l.event(EV_KEY, BTN_Z, 1); // Capture
        let s = l.state();
        assert!(s.pressed(PadButton::PaddleUL) && s.pressed(PadButton::PaddleLL));
        assert!(!s.pressed(PadButton::RB));
        assert_eq!(s.axis(Axis::RightTrigger), 0.0, "SR is not ZR");
        assert_eq!(s.axis(Axis::LeftTrigger), 1.0);
        assert!(s.pressed(PadButton::Misc));

        let mut r = joycon(JoyConSide::Right);
        r.event(EV_KEY, BTN_TL, 1); // SL
        r.event(EV_KEY, BTN_TL2, 1); // SR
        r.event(EV_KEY, BTN_TR, 1); // R
        let s = r.state();
        assert!(s.pressed(PadButton::PaddleLR) && s.pressed(PadButton::PaddleUR));
        assert!(s.pressed(PadButton::RB) && !s.pressed(PadButton::LB));
        assert_eq!(s.axis(Axis::LeftTrigger), 0.0);
    }

    #[test]
    fn joycond_paddles() {
        let mut m = PadMapper::new(PadFamily::JoyCon, &[(ABS_X, STICK), (ABS_Y, STICK)]);
        m.event(EV_KEY, BTN_TRIGGER_HAPPY1, 1);
        m.event(EV_KEY, BTN_TRIGGER_HAPPY4, 1);
        m.event(EV_KEY, BTN_TL, 1);
        let s = m.state();
        assert!(s.pressed(PadButton::PaddleUL) && s.pressed(PadButton::PaddleUR));
        assert!(s.pressed(PadButton::LB), "a pair's L is a shoulder");
    }

    #[test]
    fn nintendo_motion_frame() {
        let accel = AbsRange {
            resolution: 4096,
            ..AbsRange::new(-8 * 4096, 8 * 4096)
        };
        let mut m = MotionMapper::nintendo(&[(ABS_Z, accel)]);
        m.event(EV_ABS, ABS_Z, 4096); // device z (up)
        m.event(EV_ABS, ABS_RY, 14247); // 1 deg/s about the device's y axis (default res)
        let mo = m.motion();
        assert_eq!(mo.accel, [0.0, 1.0, 0.0], "gravity on our y");
        assert!(
            (mo.gyro[0] + 1.0).abs() < 1e-4,
            "device y is our -x: {:?}",
            mo.gyro
        );
    }
}
