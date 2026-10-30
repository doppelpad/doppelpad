//! Event types and codes from the kernel's `include/uapi/linux/input-event-codes.h`.
//!
//! These numbers are stable kernel ABI. They are spelled out here instead of using the
//! `evdev` crate's typed constants so the pure mapping code compiles, and is unit-tested,
//! on every OS.

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;
pub const EV_MSC: u16 = 0x04;

pub const SYN_REPORT: u16 = 0x00;

/// `bustype` of a device connected over USB and over Bluetooth (`include/uapi/linux/input.h`).
pub const BUS_USB: u16 = 0x03;
pub const BUS_BLUETOOTH: u16 = 0x05;

/// How a device of `bustype` is connected.
pub fn transport(bustype: u16) -> crate::Transport {
    match bustype {
        BUS_USB => crate::Transport::Usb,
        BUS_BLUETOOTH => crate::Transport::Bluetooth,
        _ => crate::Transport::Other,
    }
}

/// Xbox Series "Share" button as reported by xpad.
pub const KEY_RECORD: u16 = 167;

// Mouse buttons.
pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;
pub const BTN_MIDDLE: u16 = 0x112;
pub const BTN_SIDE: u16 = 0x113;
pub const BTN_EXTRA: u16 = 0x114;
pub const BTN_FORWARD: u16 = 0x115;
pub const BTN_BACK: u16 = 0x116;

/// First joystick button; devices with it are joysticks, not keyboards.
pub const BTN_JOYSTICK: u16 = 0x120;

// Gamepad buttons (`Documentation/input/gamepad.rst`). `BTN_NORTH` is also named `BTN_X`
// and `BTN_WEST` `BTN_Y`, which is why label-reporting drivers put the left "X" button on
// `BTN_NORTH`; see `FaceLayout`.
pub const BTN_SOUTH: u16 = 0x130;
pub const BTN_EAST: u16 = 0x131;
pub const BTN_NORTH: u16 = 0x133;
pub const BTN_WEST: u16 = 0x134;
pub const BTN_Z: u16 = 0x135;
pub const BTN_TL: u16 = 0x136;
pub const BTN_TR: u16 = 0x137;
pub const BTN_TL2: u16 = 0x138;
pub const BTN_TR2: u16 = 0x139;
pub const BTN_SELECT: u16 = 0x13a;
pub const BTN_START: u16 = 0x13b;
pub const BTN_MODE: u16 = 0x13c;
pub const BTN_THUMBL: u16 = 0x13d;
pub const BTN_THUMBR: u16 = 0x13e;
pub const BTN_DPAD_UP: u16 = 0x220;
pub const BTN_DPAD_DOWN: u16 = 0x221;
pub const BTN_DPAD_LEFT: u16 = 0x222;
pub const BTN_DPAD_RIGHT: u16 = 0x223;
/// Back paddles (xpad Elite pads): upper left, upper right, lower left, lower right.
pub const BTN_GRIPL: u16 = 0x224;
pub const BTN_GRIPR: u16 = 0x225;
pub const BTN_GRIPL2: u16 = 0x226;
pub const BTN_GRIPR2: u16 = 0x227;
/// hid-playstation reports the DualSense Edge Fn1, Fn2, left and right paddle here.
pub const BTN_TRIGGER_HAPPY1: u16 = 0x2c0;
pub const BTN_TRIGGER_HAPPY2: u16 = 0x2c1;
pub const BTN_TRIGGER_HAPPY3: u16 = 0x2c2;
pub const BTN_TRIGGER_HAPPY4: u16 = 0x2c3;

// Relative axes.
pub const REL_X: u16 = 0x00;
pub const REL_Y: u16 = 0x01;
pub const REL_HWHEEL: u16 = 0x06;
pub const REL_WHEEL: u16 = 0x08;
pub const REL_WHEEL_HI_RES: u16 = 0x0b;
pub const REL_HWHEEL_HI_RES: u16 = 0x0c;

/// Units of `REL_WHEEL_HI_RES` / `REL_HWHEEL_HI_RES` per wheel detent.
pub const HI_RES_PER_DETENT: i32 = 120;

// Absolute axes.
pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;
pub const ABS_Z: u16 = 0x02;
pub const ABS_RX: u16 = 0x03;
pub const ABS_RY: u16 = 0x04;
pub const ABS_RZ: u16 = 0x05;
pub const ABS_GAS: u16 = 0x09;
pub const ABS_BRAKE: u16 = 0x0a;
pub const ABS_HAT0X: u16 = 0x10;
pub const ABS_HAT0Y: u16 = 0x11;
pub const ABS_MT_SLOT: u16 = 0x2f;
pub const ABS_MT_POSITION_X: u16 = 0x35;
pub const ABS_MT_POSITION_Y: u16 = 0x36;
pub const ABS_MT_TRACKING_ID: u16 = 0x39;
/// The last multi-touch axis.
pub const ABS_MT_TOOL_Y: u16 = 0x3d;
