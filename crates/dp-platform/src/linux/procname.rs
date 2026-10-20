//! Process names from `/proc`, for automatic profile switching.

use std::path::Path;

/// Executable name of `pid` as profiles match it: the Windows `.exe` name for Wine / Proton
/// games, otherwise the file name of the native binary (falling back to `comm`, which the
/// kernel truncates to 15 bytes).
pub fn process_name(pid: u32) -> Option<String> {
    let proc = Path::new("/proc").join(pid.to_string());
    if let Ok(cmdline) = std::fs::read(proc.join("cmdline"))
        && let Some(exe) = wine_exe(&cmdline)
    {
        return Some(exe);
    }
    if let Ok(target) = std::fs::read_link(proc.join("exe"))
        && let Some(name) = target.file_name()
    {
        let name = name.to_string_lossy();
        return Some(name.strip_suffix(" (deleted)").unwrap_or(&name).to_owned());
    }
    let comm = std::fs::read_to_string(proc.join("comm")).ok()?;
    let comm = comm.trim_end();
    (!comm.is_empty()).then(|| comm.to_owned())
}

/// Wine sets `argv[0]` of a Windows process to its Windows path (`C:\games\foo.exe`);
/// return that file name if the command line looks like one.
pub fn wine_exe(cmdline: &[u8]) -> Option<String> {
    let argv0 = cmdline.split(|&b| b == 0).next()?;
    let argv0 = String::from_utf8_lossy(argv0);
    let name = argv0.rsplit(['/', '\\']).next()?;
    (name.len() > 4 && name.to_ascii_lowercase().ends_with(".exe")).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wine_command_lines() {
        assert_eq!(
            wine_exe(b"C:\\Games\\Foo\\Game-Win64-Shipping.exe\0-dx12\0"),
            Some("Game-Win64-Shipping.exe".into())
        );
        assert_eq!(wine_exe(b"Z:/opt/game/GAME.EXE\0"), Some("GAME.EXE".into()));
        assert_eq!(wine_exe(b"/usr/bin/firefox\0--new-window\0"), None);
        assert_eq!(wine_exe(b".exe\0"), None);
        assert_eq!(wine_exe(b""), None);
    }
}
