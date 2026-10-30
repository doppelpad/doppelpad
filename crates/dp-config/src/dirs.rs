//! Where Doppelpad keeps its files: the per-user folders, portable mode and the library.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::relocate;

/// The slot state, shared by the app and the daemon, in [`config_dir`].
pub const STATE_FILE: &str = "state.toml";
/// The controller hotkeys, in [`config_dir`].
pub const HOTKEYS_FILE: &str = "hotkeys.toml";
/// The gyro calibration of each controller, in [`config_dir`].
pub const CALIBRATION_FILE: &str = "calibration.toml";

/// Per-user configuration directory (`%APPDATA%\doppelpad`, `~/.config/doppelpad` or on macOS
/// `~/Library/Application Support/doppelpad`), or the portable folder (see [`portable_dir`]).
pub fn config_dir() -> PathBuf {
    dir_in(portable_dir(), Place::Roaming)
}

/// The file next to the executable that keeps everything beside it: portable mode.
pub const PORTABLE_MARKER: &str = "portable";

/// In portable mode (a [`PORTABLE_MARKER`] file next to the running executable), the folder
/// that holds the settings, configs, logs and history: `data` beside the executable. Decided
/// once per run.
pub fn portable_dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| portable_data_dir(std::env::current_exe().ok()?.parent()?))
        .as_deref()
}

/// `data` in `exe_dir` when `exe_dir` holds the portable marker.
fn portable_data_dir(exe_dir: &Path) -> Option<PathBuf> {
    exe_dir
        .join(PORTABLE_MARKER)
        .is_file()
        .then(|| exe_dir.join("data"))
}

/// Where a kind of file lives outside portable mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    /// Settings, which roam with the user (`%APPDATA%`, `$XDG_CONFIG_HOME`).
    Roaming,
    /// State that describes this machine (`%LOCALAPPDATA%`, `$XDG_STATE_HOME`).
    Local,
}

/// Where files live: in portable mode all in the portable folder, else in `place`. macOS has
/// no roaming folder: both go to Application Support, as both go to the portable folder.
fn dir_in(portable: Option<&Path>, place: Place) -> PathBuf {
    const MACOS: &[&str] = &["Library", "Application Support"];
    let macos = cfg!(target_os = "macos");
    match (portable, place) {
        (Some(p), _) => p.to_path_buf(),
        (None, Place::Local) => user_dir(
            "LOCALAPPDATA",
            "XDG_STATE_HOME",
            if macos { MACOS } else { &[".local", "state"] },
        ),
        (None, Place::Roaming) => user_dir(
            "APPDATA",
            "XDG_CONFIG_HOME",
            if macos { MACOS } else { &[".config"] },
        ),
    }
}

/// `doppelpad` in the folder the Windows variable `win_var` names, else the XDG variable
/// `xdg_var`, else `$HOME/<home_rel>`; a relative `doppelpad` when none is set. macOS ignores
/// the XDG variables: an app started from the Finder never sees them, so a command-line tool
/// that did would use other folders than the app.
fn user_dir(win_var: &str, xdg_var: &str, home_rel: &[&str]) -> PathBuf {
    let xdg = if cfg!(target_os = "macos") {
        None
    } else {
        std::env::var_os(xdg_var)
    };
    let base = if let Some(dir) = std::env::var_os(win_var) {
        PathBuf::from(dir)
    } else if let Some(dir) = xdg {
        PathBuf::from(dir)
    } else if let Some(home) = std::env::var_os("HOME") {
        home_rel.iter().fold(PathBuf::from(home), |p, c| p.join(c))
    } else {
        return PathBuf::from("doppelpad");
    };
    base.join("doppelpad")
}

/// Directory for profiles (the library): the folder the user chose (see [`relocate`]), else
/// [`default_profiles_dir`].
pub fn profiles_dir() -> PathBuf {
    relocate::chosen_dir(&config_dir()).unwrap_or_else(default_profiles_dir)
}

/// The library's place unless the user chose another: `profiles` in [`config_dir`].
pub fn default_profiles_dir() -> PathBuf {
    config_dir().join("profiles")
}

/// Per-user directory for log files: `%LOCALAPPDATA%\doppelpad\logs`, or
/// `$XDG_STATE_HOME/doppelpad/logs` (default `~/.local/state/doppelpad/logs`; on macOS
/// `~/Library/Application Support/doppelpad/logs`). Local rather than roaming: logs describe
/// this machine. `logs` in the portable folder in portable mode.
pub fn log_dir() -> PathBuf {
    dir_in(portable_dir(), Place::Local).join("logs")
}

/// Per-user directory for earlier versions of configs: `%LOCALAPPDATA%\doppelpad\history`,
/// `$XDG_STATE_HOME/doppelpad/history` or on macOS `~/Library/Application
/// Support/doppelpad/history`. Outside the profiles folder, so the library never lists them.
/// `history` in the portable folder in portable mode.
pub fn history_dir() -> PathBuf {
    dir_in(portable_dir(), Place::Local).join("history")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_mode_keeps_everything_beside_the_executable() {
        let exe_dir = tempfile::tempdir().unwrap();
        assert_eq!(portable_data_dir(exe_dir.path()), None, "no marker");
        std::fs::create_dir(exe_dir.path().join("data")).unwrap();
        assert_eq!(
            portable_data_dir(exe_dir.path()),
            None,
            "a data folder alone"
        );
        std::fs::write(exe_dir.path().join(PORTABLE_MARKER), "").unwrap();
        let data = portable_data_dir(exe_dir.path()).unwrap();
        assert_eq!(data, exe_dir.path().join("data"));
        assert_eq!(dir_in(Some(&data), Place::Roaming), data);
        assert_eq!(dir_in(Some(&data), Place::Local), data);
        assert!(dir_in(None, Place::Roaming).ends_with("doppelpad"));
        assert!(dir_in(None, Place::Local).ends_with("doppelpad"));
        assert_eq!(portable_dir(), None, "the test runner has no marker");
    }
}
