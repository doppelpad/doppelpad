//! Start with the system: the app registers itself to start when the user signs in (Windows)
//! or logs in (macOS), with the window hidden so only the tray icon shows. The system's own
//! entry is the only record, so turning it off in Task Manager or in the login items shows in
//! the settings too.

/// Command-line flag: start with the window hidden.
pub const HIDDEN_ARG: &str = "--hidden";
/// Name of the entry in Windows' list of programs to start.
#[cfg(windows)]
const NAME: &str = "Doppelpad";
/// Label of the launch agent on macOS.
#[cfg(target_os = "macos")]
const LABEL: &str = "dev.doppelpad.app";

/// The app was started with [`HIDDEN_ARG`].
pub fn started_hidden() -> bool {
    std::env::args().skip(1).any(|a| a == HIDDEN_ARG)
}

/// Whether the app starts with the system; `None` where it cannot (Linux) or should not
/// (portable mode: the copy may sit on a stick that is not always there).
pub fn enabled() -> Option<bool> {
    if dp_config::portable_dir().is_some() {
        return None;
    }
    #[cfg(windows)]
    {
        Some(dp_platform::windows::autostart::is_enabled(NAME))
    }
    #[cfg(target_os = "macos")]
    {
        Some(dp_platform::macos::autostart::is_enabled(LABEL))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    None
}

/// Register this executable to start hidden at sign-in, or remove it.
pub fn set(on: bool) -> std::io::Result<()> {
    if on && dp_config::portable_dir().is_some() {
        return Err(std::io::Error::other(
            "a portable copy does not start with Windows",
        ));
    }
    #[cfg(windows)]
    {
        use dp_platform::windows::autostart;
        tracing::info!("start with Windows: {}", if on { "on" } else { "off" });
        if on {
            let exe = std::env::current_exe()?;
            autostart::enable(NAME, &format!("\"{}\" {HIDDEN_ARG}", exe.display()))
        } else {
            autostart::disable(NAME)
        }
    }
    #[cfg(target_os = "macos")]
    {
        use dp_platform::macos::autostart;
        tracing::info!("start at login: {}", if on { "on" } else { "off" });
        if on {
            autostart::enable(LABEL, &std::env::current_exe()?, &[HIDDEN_ARG])
        } else {
            autostart::disable(LABEL)
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = on;
        Err(std::io::Error::other(
            "starting with the system is only supported on Windows and macOS",
        ))
    }
}
