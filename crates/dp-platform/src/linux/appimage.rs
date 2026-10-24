//! Programs started from the AppImage. Its start scripts point `LD_LIBRARY_PATH`, `PATH`,
//! `XDG_DATA_DIRS`, GTK's and GIO's module folders and more into the AppImage, whose
//! libraries come from Ubuntu 22.04. A program Doppelpad starts would inherit that and load
//! those libraries instead of its own, which most programs don't survive (Konsole on
//! SteamOS: "version `XZ_5.4' not found"). So they get the environment without the
//! AppImage's parts.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

/// Variables that only mean something inside the AppImage. Its GTK start script sets
/// `GTK_THEME` to Adwaita, over the user's own.
const APPIMAGE_ONLY: [&str; 5] = ["APPDIR", "APPIMAGE", "ARGV0", "OWD", "GTK_THEME"];

/// `program` with the environment it would have outside the AppImage.
pub fn host_command(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    let vars = std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)));
    for (name, value) in host_env(vars) {
        match value {
            Some(value) => cmd.env(name, value),
            None => cmd.env_remove(name),
        };
    }
    cmd
}

/// The changes that undo the AppImage's environment, `None` for a variable to remove:
/// lists lose their entries inside `APPDIR`, and a variable left without any is removed.
/// Outside an AppImage (no `APPDIR`) there are none.
fn host_env(vars: impl IntoIterator<Item = (String, String)>) -> Vec<(String, Option<String>)> {
    let vars: Vec<_> = vars.into_iter().collect();
    let Some(appdir) = vars
        .iter()
        .find(|(name, _)| name == "APPDIR")
        .filter(|(_, value)| value.starts_with('/') && !value.trim_matches('/').is_empty())
        .map(|(_, value)| Path::new(value))
    else {
        return Vec::new();
    };
    let inside = |entry: &str| Path::new(entry).starts_with(appdir);
    let mut changes = Vec::new();
    for (name, value) in &vars {
        if APPIMAGE_ONLY.contains(&name.as_str()) {
            changes.push((name.clone(), None));
        } else if value.split(':').any(inside) {
            // An empty entry, as the start script leaves at the end, means the current folder.
            let kept: Vec<_> = value
                .split(':')
                .filter(|entry| !entry.is_empty() && !inside(entry))
                .collect();
            changes.push((name.clone(), (!kept.is_empty()).then(|| kept.join(":"))));
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(vars: &[(&str, &str)]) -> Vec<(String, String)> {
        vars.iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn appimage_parts_are_left_out() {
        // As the AppImage built by the release workflow starts on SteamOS.
        let m = "/tmp/.mount_DoppelMHMFdF";
        let vars = env(&[
            ("APPDIR", m),
            ("APPIMAGE", "/home/deck/Doppelpad_0.5.0_amd64.AppImage"),
            ("OWD", "/home/deck"),
            ("GTK_THEME", "Adwaita:dark"),
            (
                "PATH",
                &format!("{m}/usr/bin/:{m}/usr/sbin/:/usr/local/bin:/usr/bin"),
            ),
            (
                "LD_LIBRARY_PATH",
                &format!("{m}/usr/lib/:{m}/usr/lib/x86_64-linux-gnu/:"),
            ),
            (
                "XDG_DATA_DIRS",
                &format!("{m}/usr/share/:{m}/usr/share:/usr/share:/usr/local/share"),
            ),
            ("GIO_MODULE_DIR", &format!("{m}//usr/lib/gio/modules")),
            ("DISPLAY", ":0"),
            ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
            ("HOME", "/home/deck"),
        ]);
        let mut changes = host_env(vars);
        changes.sort();
        let set = |name: &str, value: Option<&str>| (name.to_string(), value.map(str::to_string));
        assert_eq!(
            changes,
            [
                set("APPDIR", None),
                set("APPIMAGE", None),
                set("GIO_MODULE_DIR", None),
                set("GTK_THEME", None),
                set("LD_LIBRARY_PATH", None),
                set("OWD", None),
                set("PATH", Some("/usr/local/bin:/usr/bin")),
                set("XDG_DATA_DIRS", Some("/usr/share:/usr/local/share")),
            ]
        );
    }

    #[test]
    fn nothing_changes_outside_an_appimage() {
        let vars = env(&[
            ("PATH", "/usr/bin"),
            ("GTK_THEME", "Breeze"),
            ("LD_LIBRARY_PATH", ":"),
        ]);
        assert!(host_env(vars).is_empty());
        // An APPDIR that would match every absolute path is not taken for an AppImage.
        let vars = env(&[("APPDIR", "/"), ("PATH", "/usr/bin")]);
        assert!(host_env(vars).is_empty());
    }

    #[test]
    fn similar_names_are_not_inside() {
        let vars = env(&[
            ("APPDIR", "/tmp/.mount_Dop"),
            ("PATH", "/tmp/.mount_Doppel/usr/bin:/tmp/.mount_Dop/usr/bin"),
        ]);
        assert_eq!(
            host_env(vars),
            [
                ("APPDIR".to_string(), None),
                (
                    "PATH".to_string(),
                    Some("/tmp/.mount_Doppel/usr/bin".to_string())
                ),
            ]
        );
    }
}
