//! Linux backends: evdev input, uinput virtual pad and keyboard/mouse output, `EVIOCGRAB`
//! device hiding and capture, and X11 foreground detection.
//!
//! Needs read access to `/dev/input/event*` and write access to `/dev/uinput`; see
//! `docs/LINUX.md` for the udev rule. Everything Doppelpad creates through uinput carries a
//! `phys` starting with `doppelpad/`, which the input backends use to skip their own
//! devices (the Linux counterpart of the `VirtualIgnoreList` on Windows).
//!
//! The pure translation code (`codes`, `padmap`, `kbmmap`, `procname`) also compiles in
//! tests on other OSes, so it is unit-tested everywhere.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

mod codes;
mod kbmmap;
mod padmap;
mod procname;

#[cfg(target_os = "linux")]
mod capture;
#[cfg(target_os = "linux")]
mod dev;
#[cfg(target_os = "linux")]
mod foreground;
#[cfg(target_os = "linux")]
mod gamepad;
#[cfg(target_os = "linux")]
mod uinput;

#[cfg(target_os = "linux")]
pub fn platform() -> crate::Platform {
    let gamepads = gamepad::EvdevBackend::new();
    let hider = gamepads.hider();
    crate::Platform {
        gamepads: vec![Box::new(crate::joycon::JoyConCombiner::new(gamepads))],
        virtual_pads: Box::new(uinput::UinputFactory::default()),
        kbm_out: Box::new(uinput::UinputKbm::new()),
        kbm_capture: Box::new(capture::GrabCapture::default()),
        hider: Box::new(hider),
        foreground: Box::new(foreground::X11Foreground::default()),
    }
}
