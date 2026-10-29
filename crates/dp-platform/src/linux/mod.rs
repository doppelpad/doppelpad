//! Linux backends: evdev input, Switch 2 controllers on USB (hidraw) and over Bluetooth LE
//! (BlueZ), uinput virtual pad and keyboard/mouse output, `EVIOCGRAB` device hiding and
//! capture, and X11 foreground detection.
//!
//! Needs read access to `/dev/input/event*` and write access to `/dev/uinput`; see
//! `docs/LINUX.md` for the udev rule. Everything Doppelpad creates through uinput carries a
//! `phys` starting with `doppelpad/`, which the input backends use to skip their own
//! devices (the Linux counterpart of the `VirtualIgnoreList` on Windows).
//!
//! The pure translation code (`codes`, `padmap`, `x360`, `kbmmap`, `procname`, `sysfs`,
//! `appimage`, `att`, `gatt`) also compiles in tests on other OSes, so it is
//! unit-tested everywhere.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

mod appimage;
mod att;
mod codes;
mod gatt;
mod kbmmap;
mod padmap;
mod procname;
mod sysfs;
mod x360;

#[cfg(target_os = "linux")]
mod ble;
#[cfg(target_os = "linux")]
mod bluez;
#[cfg(target_os = "linux")]
mod capture;
#[cfg(target_os = "linux")]
mod dev;
#[cfg(target_os = "linux")]
mod foreground;
#[cfg(target_os = "linux")]
mod gamepad;
#[cfg(target_os = "linux")]
mod hid;
#[cfg(target_os = "linux")]
mod hidraw;
#[cfg(target_os = "linux")]
mod l2cap;
#[cfg(target_os = "linux")]
mod logind;
#[cfg(target_os = "linux")]
mod sysbus;
#[cfg(target_os = "linux")]
mod system;
#[cfg(target_os = "linux")]
mod uinput;
#[cfg(target_os = "linux")]
mod x11;

pub use appimage::host_command;

/// The distribution's name and the kernel version.
#[cfg(target_os = "linux")]
pub fn os_version() -> String {
    let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default();
    let name = read("/etc/os-release")
        .lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim_matches('"').to_string())
        .unwrap_or_else(|| std::env::consts::OS.to_string());
    match read("/proc/sys/kernel/osrelease").trim() {
        "" => name,
        kernel => format!("{name} (kernel {kernel})"),
    }
}

#[cfg(target_os = "linux")]
pub fn platform() -> crate::Platform {
    let switch2_search = crate::Switch2Search::default();
    let gamepads = gamepad::EvdevBackend::new();
    let hid = hid::backend(switch2_search.clone());
    let hider = hid::Hider::new(gamepads.hider(), &hid);
    crate::Platform {
        probe_drivers: || crate::DriverReport {
            virtual_pads: dev::check_uinput(),
            device_hiding: Ok(()),
            input_access: Ok(()),
            bluetooth: ble::check(),
            hiding_problems: Vec::new(),
            system_problems: sysbus::problems(),
        },
        gamepads: vec![
            Box::new(crate::joycon::JoyConCombiner::new(gamepads)),
            Box::new(hid),
        ],
        virtual_pads: Box::new(uinput::UinputFactory::default()),
        kbm_out: Box::new(uinput::UinputKbm::new()),
        kbm_capture: Box::new(capture::GrabCapture::default()),
        hider: Box::new(hider),
        foreground: Box::new(foreground::X11Foreground::default()),
        system: Box::new(system::LinuxSystem::default()),
        switch2_search,
    }
}
