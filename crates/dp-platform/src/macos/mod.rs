//! macOS backends: IOKit HID input, with controllers hidden by seizing them; keyboard and
//! mouse output through Quartz events and capture through an event tap; the foreground
//! application from the window server's window list; power, window and screenshot commands.
//!
//! Reading controllers needs no permission; keyboard and mouse need Accessibility (see
//! [`permissions`]). The pure translation code (`keycodes`) also compiles in tests on other
//! OSes, so it is unit-tested everywhere.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

mod keycodes;

#[cfg(target_os = "macos")]
mod capture;
#[cfg(target_os = "macos")]
mod foreground;
#[cfg(target_os = "macos")]
mod hid;
#[cfg(target_os = "macos")]
mod hidio;
#[cfg(target_os = "macos")]
mod output;
#[cfg(target_os = "macos")]
pub mod overlay;
#[cfg(target_os = "macos")]
pub mod permissions;
#[cfg(target_os = "macos")]
mod process;
#[cfg(target_os = "macos")]
mod props;
#[cfg(target_os = "macos")]
mod system;
#[cfg(target_os = "macos")]
mod windows;

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

/// The executable file name of the program in front (see `crate::foreground_program`).
#[cfg(target_os = "macos")]
pub fn foreground_program() -> Option<String> {
    process::process_name(windows::front()?.pid)
}

/// Whether a process with this executable file name runs (any case).
#[cfg(target_os = "macos")]
pub fn program_running(exe: &str) -> bool {
    process::running(exe)
}

/// The programs that show a window now, one entry per program: its executable file name and
/// the title of its frontmost window (its name where the title is not readable).
#[cfg(target_os = "macos")]
pub fn windowed_programs() -> Vec<crate::WindowedProgram> {
    let own = std::process::id();
    let mut out: Vec<crate::WindowedProgram> = Vec::new();
    for w in windows::on_screen() {
        if w.pid == own {
            continue;
        }
        let Some(exe) = process::process_name(w.pid) else {
            continue;
        };
        if out.iter().any(|p| p.exe.eq_ignore_ascii_case(&exe)) {
            continue;
        }
        let title = w.title.or(w.owner).unwrap_or_else(|| exe.clone());
        out.push(crate::WindowedProgram { exe, title });
    }
    out
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
    let gamepads = hid::backend();
    let hider = hid::SeizeHider::new(&gamepads);
    crate::Platform {
        probe_drivers: || crate::DriverReport {
            virtual_pads: Err(crate::PlatformError::Unsupported(NO_VIRTUAL_PADS)),
            device_hiding: Ok(()),
            input_access: permissions::check(),
            hiding_problems: Vec::new(),
        },
        gamepads: vec![Box::new(crate::joycon::JoyConCombiner::new(gamepads))],
        virtual_pads: Box::new(NoVirtualPads),
        kbm_out: Box::new(output::EventOutput::new()),
        kbm_capture: Box::new(capture::TapCapture::default()),
        hider: Box::new(hider),
        foreground: Box::new(foreground::Foreground::default()),
        system: Box::new(system::MacSystem::default()),
    }
}
