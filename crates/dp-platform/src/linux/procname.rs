//! Process names from `/proc`, for automatic profile switching, and whether a program runs.

use std::path::Path;

use crate::wine::wine_exe;

/// Executable name of `pid` as profiles match it: the Windows `.exe` name for Wine / Proton
/// games, otherwise the file name of the native binary (falling back to `comm`, which the
/// kernel truncates to 15 bytes).
pub fn process_name(pid: u32) -> Option<String> {
    let proc = Path::new("/proc").join(pid.to_string());
    let cmdline = std::fs::read(proc.join("cmdline")).unwrap_or_default();
    if let Some(exe) = wine_exe(&cmdline) {
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
    if let Some(name) = argv0_name(&cmdline, comm) {
        return Some(name);
    }
    (!comm.is_empty()).then(|| comm.to_owned())
}

/// Whether a process of this name runs (any case), as `comm` names it: for telling which
/// program holds a controller.
pub fn running(name: &str) -> bool {
    let Ok(procs) = std::fs::read_dir("/proc") else {
        return false;
    };
    procs.flatten().any(|p| {
        p.file_name()
            .to_str()
            .is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
            && std::fs::read_to_string(p.path().join("comm"))
                .is_ok_and(|comm| comm.trim_end().eq_ignore_ascii_case(name))
    })
}

/// The kernel cuts `comm` to this many bytes.
const COMM_LEN: usize = 15;

/// The file name in `argv[0]` when `comm` is its start cut to [`COMM_LEN`], so the whole name.
/// For when `exe` cannot be read: from a container or sandbox, other processes' `exe` links
/// are off limits, but their command lines are not.
pub fn argv0_name(cmdline: &[u8], comm: &str) -> Option<String> {
    let argv0 = cmdline.split(|&b| b == 0).next()?;
    let argv0 = String::from_utf8_lossy(argv0);
    let name = argv0.rsplit('/').next()?;
    (comm.len() >= COMM_LEN && name.len() > comm.len() && name.starts_with(comm))
        .then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_names_from_argv0() {
        let silksong =
            b"/run/media/sd/steamapps/common/Hollow Knight Silksong/Hollow Knight Silksong\0";
        assert_eq!(
            argv0_name(silksong, "Hollow Knight S"),
            Some("Hollow Knight Silksong".into())
        );
        // Not cut short: comm is the whole name already.
        assert_eq!(argv0_name(b"/usr/bin/firefox\0", "firefox"), None);
        // A program that named itself after the start of its file name keeps that name.
        assert_eq!(argv0_name(b"/opt/app/app-launcher\0", "app"), None);
        // A script under an interpreter: argv[0] names the interpreter.
        assert_eq!(argv0_name(b"/usr/bin/python3\0game.py\0", "game.py"), None);
        assert_eq!(argv0_name(b"", "x"), None);
        assert_eq!(argv0_name(b"/bin/sh\0", ""), None);
    }
}
