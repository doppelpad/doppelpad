//! Windows programs under Wine (Proton on Linux; CrossOver, Whisky and the Game Porting
//! Toolkit on macOS): profiles know them by their Windows `.exe` name.

/// Wine sets `argv[0]` of a Windows process to its Windows path (`C:\games\foo.exe`); return
/// that file name if the command line (arguments separated by NUL bytes) looks like one.
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
