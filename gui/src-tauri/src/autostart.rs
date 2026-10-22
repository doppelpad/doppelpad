//! Start with Windows: the app registers itself to start when the user signs in, with the
//! window hidden so only the tray icon shows. Windows' own entry is the only record, so
//! turning it off in Task Manager shows in the settings too.

/// Command-line flag: start with the window hidden.
pub const HIDDEN_ARG: &str = "--hidden";
/// Name of the entry in Windows' list of programs to start.
#[cfg(windows)]
const NAME: &str = "Doppelpad";

/// The app was started with [`HIDDEN_ARG`].
pub fn started_hidden() -> bool {
    std::env::args().skip(1).any(|a| a == HIDDEN_ARG)
}

/// Whether the app starts with Windows; `None` where it cannot (not Windows) or should not
/// (portable mode: the copy may sit on a stick that is not always there).
pub fn enabled() -> Option<bool> {
    if dp_config::portable_dir().is_some() {
        return None;
    }
    #[cfg(windows)]
    {
        Some(dp_platform::windows::autostart::is_enabled(NAME))
    }
    #[cfg(not(windows))]
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
    #[cfg(not(windows))]
    {
        let _ = on;
        Err(std::io::Error::other(
            "starting with the system is only supported on Windows",
        ))
    }
}
