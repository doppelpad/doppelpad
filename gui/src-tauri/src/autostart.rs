//! Start with the system: the app registers itself to start when the user signs in (Windows)
//! or logs in (macOS), with the window hidden so only the tray icon shows. The system's own
//! entry is the only record, so turning it off in Task Manager or in the login items shows in
//! the settings too.

use tauri::AppHandle;

use crate::error::{CmdError, CmdResult, ErrorCode};

/// Command-line flag: start with the window hidden.
pub const HIDDEN_ARG: &str = "--hidden";
/// Name of the entry in Windows' list of programs to start: the product name (a test checks it
/// against `tauri.conf.json`), which the uninstaller also removes by this name.
#[cfg(any(windows, test))]
const NAME: &str = "Doppelpad";

/// Label of the launch agent on macOS: the app's identifier.
#[cfg(target_os = "macos")]
fn label(app: &AppHandle) -> &str {
    &app.config().identifier
}

/// The app was started with [`HIDDEN_ARG`].
pub fn started_hidden() -> bool {
    std::env::args().skip(1).any(|a| a == HIDDEN_ARG)
}

/// Whether the app starts with the system; `None` where it cannot (Linux) or should not
/// (portable mode: the copy may sit on a stick that is not always there).
pub fn enabled(app: &AppHandle) -> Option<bool> {
    if dp_config::portable_dir().is_some() {
        return None;
    }
    #[cfg(windows)]
    {
        let _ = app;
        Some(dp_platform::windows::autostart::is_enabled(NAME))
    }
    #[cfg(target_os = "macos")]
    {
        Some(dp_platform::macos::autostart::is_enabled(label(app)))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = app;
        None
    }
}

/// Register this executable to start hidden at sign-in, or remove it.
pub fn set(app: &AppHandle, on: bool) -> CmdResult<()> {
    if on && dp_config::portable_dir().is_some() {
        return Err(CmdError::new(ErrorCode::PortableAutostart));
    }
    #[cfg(windows)]
    {
        use dp_platform::windows::autostart;
        let _ = app;
        tracing::info!("start with Windows: {}", if on { "on" } else { "off" });
        if on {
            let exe = std::env::current_exe()?;
            Ok(autostart::enable(
                NAME,
                &format!("\"{}\" {HIDDEN_ARG}", exe.display()),
            )?)
        } else {
            Ok(autostart::disable(NAME)?)
        }
    }
    #[cfg(target_os = "macos")]
    {
        use dp_platform::macos::autostart;
        tracing::info!("start at login: {}", if on { "on" } else { "off" });
        if on {
            Ok(autostart::enable(
                label(app),
                &std::env::current_exe()?,
                &[HIDDEN_ARG],
            )?)
        } else {
            Ok(autostart::disable(label(app))?)
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (app, on);
        Err(CmdError::new(ErrorCode::AutostartUnsupported))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_entry_is_named_after_the_product() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["productName"], super::NAME);
    }
}
