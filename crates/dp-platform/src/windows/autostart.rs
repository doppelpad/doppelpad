//! Starting a program when the user signs in: a value under the user's `Run` key. Task
//! Manager's Startup tab does not delete that value to turn it off; it marks it disabled
//! under `StartupApproved\Run`, so both are read.

use std::io;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
};

use super::{registry, wide};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

/// Whether Windows starts the program registered as `name` when the user signs in.
pub fn is_enabled(name: &str) -> bool {
    enabled_in(RUN, APPROVED, name)
}

/// Start `command` (the quoted executable path plus arguments) when the user signs in.
pub fn enable(name: &str, command: &str) -> io::Result<()> {
    enable_in(RUN, APPROVED, name, command)
}

/// Stop starting `name` when the user signs in. Not being registered is fine.
pub fn disable(name: &str) -> io::Result<()> {
    disable_in(RUN, APPROVED, name)
}

fn enabled_in(run: &str, approved: &str, name: &str) -> bool {
    registry::exists(HKEY_CURRENT_USER, run, name) && !disabled_in_task_manager(approved, name)
}

fn enable_in(run: &str, approved: &str, name: &str, command: &str) -> io::Result<()> {
    let (key, value, data) = (wide(run), wide(name), wide(command));
    // SAFETY: NUL-terminated names; `data` is passed with its size in bytes, NUL included.
    check(unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            REG_SZ,
            data.as_ptr().cast(),
            (data.len() * size_of::<u16>()) as u32,
        )
    })?;
    // An entry turned off in Task Manager stays off while its mark is there.
    delete(approved, name)
}

fn disable_in(run: &str, approved: &str, name: &str) -> io::Result<()> {
    delete(run, name)?;
    delete(approved, name)
}

/// Task Manager stores a flag and a timestamp per entry; an odd first byte means disabled.
fn disabled_in_task_manager(key: &str, name: &str) -> bool {
    let mut data = [0u8; 64];
    registry::get_binary(HKEY_CURRENT_USER, key, name, &mut data)
        .is_some_and(|n| n > 0 && data[0] & 1 == 1)
}

fn delete(key: &str, name: &str) -> io::Result<()> {
    let (key, name) = (wide(key), wide(name));
    // SAFETY: NUL-terminated names.
    match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) } {
        ERROR_FILE_NOT_FOUND => Ok(()),
        r => check(r),
    }
}

fn check(r: WIN32_ERROR) -> io::Result<()> {
    match r {
        ERROR_SUCCESS => Ok(()),
        e => Err(io::Error::from_raw_os_error(e as i32)),
    }
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::System::Registry::{REG_BINARY, RegDeleteTreeW};

    use super::*;

    /// A scratch key under HKCU, removed again even if the test fails.
    struct Scratch(String);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let key = wide(&self.0);
            // SAFETY: NUL-terminated name.
            unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, key.as_ptr()) };
        }
    }

    fn mark(key: &str, name: &str, flag: u8) {
        let (key, name) = (wide(key), wide(name));
        let data = [flag, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        // SAFETY: NUL-terminated names; `data` is passed with its size.
        let r = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_BINARY,
                data.as_ptr().cast(),
                data.len() as u32,
            )
        };
        assert_eq!(r, ERROR_SUCCESS);
    }

    #[test]
    fn enable_disable_and_task_manager_marks() {
        let root = Scratch(format!(r"Software\Doppelpad-test-{}", std::process::id()));
        let run = format!(r"{}\Run", root.0);
        let approved = format!(r"{}\StartupApproved", root.0);
        let enabled = || enabled_in(&run, &approved, "Doppelpad");

        assert!(!enabled());
        enable_in(
            &run,
            &approved,
            "Doppelpad",
            r#""C:\Doppelpad.exe" --hidden"#,
        )
        .unwrap();
        assert!(enabled());

        mark(&approved, "Doppelpad", 3);
        assert!(!enabled(), "turned off in Task Manager");
        mark(&approved, "Doppelpad", 2);
        assert!(enabled(), "turned on again in Task Manager");
        mark(&approved, "Doppelpad", 3);
        enable_in(
            &run,
            &approved,
            "Doppelpad",
            r#""C:\Doppelpad.exe" --hidden"#,
        )
        .unwrap();
        assert!(enabled(), "enabling clears Task Manager's mark");

        disable_in(&run, &approved, "Doppelpad").unwrap();
        assert!(!enabled());
        disable_in(&run, &approved, "Doppelpad").unwrap();
    }
}
