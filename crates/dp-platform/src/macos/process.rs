//! Processes: their names as profiles match them, and whether one runs.

use std::ffi::c_void;

use crate::wine::wine_exe;

/// Executable name of `pid` as profiles match it: the Windows `.exe` name for games under
/// Wine (CrossOver, Whisky, the Game Porting Toolkit), otherwise the file name of the
/// program, which for an app is the one in its bundle's `Contents/MacOS`.
pub fn process_name(pid: u32) -> Option<String> {
    if let Some(exe) = command_line(pid).as_deref().and_then(wine_exe) {
        return Some(exe);
    }
    let path = executable(pid)?;
    path.rsplit('/').next().map(str::to_owned)
}

/// The path of the program `pid` runs.
fn executable(pid: u32) -> Option<String> {
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: the buffer and its length match.
    let n = unsafe {
        libc::proc_pidpath(
            i32::try_from(pid).ok()?,
            buf.as_mut_ptr().cast::<c_void>(),
            buf.len() as u32,
        )
    };
    let n = usize::try_from(n).ok().filter(|&n| n > 0)?;
    Some(String::from_utf8_lossy(&buf[..n]).into_owned())
}

/// The arguments of `pid`, separated by NUL bytes as `/proc/<pid>/cmdline` has them on Linux.
fn command_line(pid: u32) -> Option<Vec<u8>> {
    let mut mib = [
        libc::CTL_KERN,
        libc::KERN_PROCARGS2,
        i32::try_from(pid).ok()?,
    ];
    let mut len: libc::size_t = 0;
    // SAFETY: asks for the size only.
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || len == 0 {
        return None;
    }
    let mut buf = vec![0u8; len];
    // SAFETY: the buffer holds `len` bytes, which the kernel fills and updates.
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            buf.as_mut_ptr().cast::<c_void>(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    buf.truncate(len);
    arguments(&buf)
}

/// The arguments in a `KERN_PROCARGS2` block: the argument count, the executable's path and
/// its padding, then the arguments (and after them the environment), each ending in NUL.
fn arguments(block: &[u8]) -> Option<Vec<u8>> {
    let argc = usize::try_from(i32::from_ne_bytes(block.get(..4)?.try_into().ok()?)).ok()?;
    let rest = &block[4..];
    let path_end = rest.iter().position(|&b| b == 0)?;
    let rest = &rest[path_end..];
    let start = rest.iter().position(|&b| b != 0)?;
    let mut out = Vec::new();
    for arg in rest[start..].split(|&b| b == 0).take(argc) {
        out.extend_from_slice(arg);
        out.push(0);
    }
    Some(out)
}

/// The ids of all processes.
fn pids() -> Vec<u32> {
    // SAFETY: asks for the count only.
    let n = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    let Ok(n) = usize::try_from(n) else {
        return Vec::new();
    };
    // Room for processes started meanwhile.
    let mut pids: Vec<libc::pid_t> = vec![0; n + 64];
    // SAFETY: the buffer and its length in bytes match.
    let n = unsafe {
        libc::proc_listallpids(
            pids.as_mut_ptr().cast::<c_void>(),
            (pids.len() * size_of::<libc::pid_t>()) as i32,
        )
    };
    pids.truncate(usize::try_from(n).unwrap_or(0));
    pids.into_iter()
        .filter_map(|p| u32::try_from(p).ok())
        .collect()
}

/// Whether a process whose program has this file name runs (any case).
pub fn running(name: &str) -> bool {
    pids().into_iter().any(|pid| {
        executable(pid)
            .and_then(|p| p.rsplit('/').next().map(|n| n.eq_ignore_ascii_case(name)))
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_of_a_procargs_block() {
        let mut block = 2i32.to_ne_bytes().to_vec();
        block.extend_from_slice(b"/Applications/CrossOver.app/wine64-preloader\0\0\0\0");
        block.extend_from_slice(b"C:\\Games\\Game.exe\0-windowed\0HOME=/Users/me\0");
        assert_eq!(
            arguments(&block).as_deref(),
            Some(&b"C:\\Games\\Game.exe\0-windowed\0"[..])
        );
        assert_eq!(arguments(&block[..2]), None);
    }

    #[test]
    fn this_process_is_found() {
        let me = std::process::id();
        let name = process_name(me).unwrap();
        assert!(!name.is_empty() && !name.contains('/'));
        assert!(running(&name));
        assert!(pids().contains(&me));
    }
}
