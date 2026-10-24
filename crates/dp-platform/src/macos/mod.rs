//! macOS backends: IOKit HID input, with controllers hidden by seizing them.
//!
//! Reading controllers needs no permission. The pure parts (`descriptor`) also compile in
//! tests on other OSes, so they are unit-tested everywhere.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

mod descriptor;

#[cfg(target_os = "macos")]
mod hid;
#[cfg(target_os = "macos")]
mod hidio;
#[cfg(target_os = "macos")]
mod props;

/// The macOS version and build, as `sw_vers` shows them.
#[cfg(target_os = "macos")]
pub fn os_version() -> String {
    let sysctl = |name: &str| {
        std::process::Command::new("/usr/sbin/sysctl")
            .args(["-n", name])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    match (sysctl("kern.osproductversion"), sysctl("kern.osversion")) {
        (Some(v), Some(build)) => format!("macOS {v} ({build})"),
        (Some(v), None) => format!("macOS {v}"),
        _ => "macOS".to_owned(),
    }
}

/// Virtual controllers: none, for now.
#[cfg(target_os = "macos")]
struct NoVirtualPads;

#[cfg(target_os = "macos")]
const NO_VIRTUAL_PADS: &str = "virtual controllers on macOS (they need an entitlement from Apple)";

#[cfg(target_os = "macos")]
impl crate::VirtualPadFactory for NoVirtualPads {
    fn available(&self) -> crate::Result<()> {
        Err(crate::PlatformError::Unsupported(NO_VIRTUAL_PADS))
    }

    fn create(
        &mut self,
        _kind: crate::VirtualPadKind,
    ) -> crate::Result<Box<dyn crate::VirtualPad>> {
        Err(crate::PlatformError::Unsupported(NO_VIRTUAL_PADS))
    }
}

#[cfg(target_os = "macos")]
pub fn platform() -> crate::Platform {
    use crate::NullPlatform;

    let gamepads = hid::backend();
    let hider = hid::SeizeHider::new(&gamepads);
    crate::Platform {
        probe_drivers: || crate::DriverReport {
            virtual_pads: Err(crate::PlatformError::Unsupported(NO_VIRTUAL_PADS)),
            device_hiding: Ok(()),
            hiding_problems: Vec::new(),
        },
        gamepads: vec![Box::new(crate::joycon::JoyConCombiner::new(gamepads))],
        virtual_pads: Box::new(NoVirtualPads),
        kbm_out: Box::new(NullPlatform),
        kbm_capture: Box::new(NullPlatform),
        hider: Box::new(hider),
        foreground: Box::new(NullPlatform),
        system: Box::new(NullPlatform),
    }
}
